use crate::{
    db::DbConnection,
    models::{Definition, NewDefinition, UpdateDefinition},
    s3,
    schema::definitions,
    utils::{source_utils, stream_utils},
};
use anyhow::{Context, Result};
use aws_sdk_s3;
use aws_smithy_types::byte_stream::ByteStream;
use diesel::{associations::HasTable, prelude::*};
use futures::stream::TryStreamExt;
use http_body_util::StreamBody;
use reqwest;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;

#[derive(Debug, Deserialize)]
pub enum SortBy {
    Id,
    Name,
    Type,
}

#[derive(Debug, Deserialize)]
pub enum SortOrder {
    Asc,
    Desc,
}

#[derive(Debug, Deserialize, Default)]
pub struct DefinitionFilter {
    pub query: Option<String>,
    pub is_enabled: Option<bool>,
    pub r#type: Option<String>,
    pub limit: Option<i32>,
    pub offset: Option<i32>,
    pub sort_by: Option<SortBy>,
    pub sort_order: Option<SortOrder>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DefinitionPayload {
    pub id: String,
    pub name: String,
    pub r#type: String,
    pub description: String,
    pub file_url: String,
    pub digest: String,
    pub source_url: Option<String>,
}

async fn fetch_definition_from_path(path: &Path) -> Result<DefinitionPayload> {
    let content = fs::read_to_string(path)
        .await
        .context("Failed to read definition file")?;
    let definition_payload = serde_json::from_str::<DefinitionPayload>(&content)
        .context("Failed to parse definition JSON")?;

    Ok(definition_payload)
}

async fn fetch_definition_from_url(
    http_client: &reqwest::Client,
    url: &str,
) -> Result<DefinitionPayload> {
    let definition_payload = http_client
        .get(url)
        .header("User-Agent", "MCI/1.0")
        .send()
        .await
        .context("Failed to send HTTP request")?
        .error_for_status()
        .context("HTTP request returned error status")?
        .json::<DefinitionPayload>()
        .await
        .context("Failed to parse definition JSON from response")?;

    Ok(definition_payload)
}

async fn fetch_definition(
    http_client: &reqwest::Client,
    source: &source_utils::Source,
) -> Result<DefinitionPayload> {
    match source {
        source_utils::Source::Http(url) => fetch_definition_from_url(http_client, url).await,
        source_utils::Source::File(path) => fetch_definition_from_path(path).await,
    }
}

fn db_create_definition(
    conn: &mut DbConnection,
    new_definition: &NewDefinition,
) -> QueryResult<Definition> {
    diesel::insert_into(definitions::table)
        .values(new_definition)
        .returning(Definition::as_returning())
        .get_result(conn)
}

fn db_update_definition(
    conn: &mut DbConnection,
    definition_id: &str,
    update_definition: &UpdateDefinition,
) -> QueryResult<Definition> {
    diesel::update(definitions::table.find(definition_id))
        .set(update_definition)
        .returning(Definition::as_returning())
        .get_result(conn)
}

pub fn get_definition(conn: &mut DbConnection, definition_id: &str) -> QueryResult<Definition> {
    definitions::table
        .find(definition_id)
        .select(Definition::as_select())
        .first(conn)
}

pub fn list_definitions(
    conn: &mut DbConnection,
    filter: &DefinitionFilter,
) -> QueryResult<Vec<Definition>> {
    use crate::schema::definitions::dsl::*;

    let mut query = definitions::table().into_boxed();

    if let Some(ref search_query) = filter.query {
        query = query.filter(
            id.ilike(format!("%{}%", search_query))
                .or(name.ilike(format!("%{}%", search_query)))
                .or(description.ilike(format!("%{}%", search_query))),
        );
    }

    if let Some(enabled_filter) = filter.is_enabled {
        query = query.filter(is_enabled.eq(enabled_filter));
    }
    if let Some(ref definition_type_filter) = filter.r#type {
        query = query.filter(type_.eq(definition_type_filter));
    }

    match (&filter.sort_by, &filter.sort_order) {
        (Some(SortBy::Id), Some(SortOrder::Desc)) => query = query.order(id.desc()),
        (Some(SortBy::Id), _) => query = query.order(id.asc()),
        (Some(SortBy::Type), Some(SortOrder::Desc)) => query = query.order(type_.desc()),
        (Some(SortBy::Type), _) => query = query.order(type_.asc()),
        (Some(SortBy::Name), Some(SortOrder::Desc)) => query = query.order(name.desc()),
        (Some(SortBy::Name), _) => query = query.order(name.asc()),
        (None, _) => {}
    }

    if let Some(limit_val) = filter.limit {
        query = query.limit(limit_val as i64);
    }
    if let Some(offset_val) = filter.offset {
        query = query.offset(offset_val as i64);
    }

    query.select(Definition::as_select()).load(conn)
}

pub fn delete_definition(conn: &mut DbConnection, definition_id: &str) -> QueryResult<usize> {
    diesel::delete(definitions::table.find(definition_id)).execute(conn)
}

pub fn update_definition(
    conn: &mut DbConnection,
    definition_id: &str,
    update_definition: &UpdateDefinition,
) -> QueryResult<Definition> {
    db_update_definition(conn, definition_id, update_definition)
}

pub async fn create_definition(
    conn: &mut DbConnection,
    http_client: &reqwest::Client,
    s3_client: &aws_sdk_s3::Client,
    payload: &DefinitionPayload,
) -> Result<Definition> {
    if get_definition(conn, &payload.id).is_ok() {
        anyhow::bail!(
            "Conflict: Definition with ID '{}' already exists",
            payload.id
        );
    }

    let definition_url = source_utils::Source::parse(&payload.file_url)?;
    let obj_key = payload.id.clone();

    let body = match &definition_url {
        source_utils::Source::Http(url) => {
            let response = stream_utils::stream_content_from_url(http_client, url)
                .await
                .context("Failed to fetch definition file from URL")?;

            let stream = response.bytes_stream();
            let frames = stream.map_ok(hyper::body::Frame::data);
            let body = StreamBody::new(frames);
            ByteStream::from_body_1_x(body)
        }
        source_utils::Source::File(path) => stream_utils::stream_content_from_path(path)
            .await
            .context("Failed to read definition file from path")?,
    };

    s3::put_stream(
        s3_client,
        "definitions",
        &obj_key,
        body,
        Some(&payload.digest),
    )
    .await
    .context("Failed to upload definition to S3")?;

    let new_definition = NewDefinition {
        id: payload.id.clone(),
        type_: payload.r#type.clone(),
        name: payload.name.clone(),
        description: payload.description.clone(),
        definition_object_key: obj_key.clone(),
        configuration_object_key: obj_key.clone(),
        secrets_object_key: obj_key.clone(),
        digest: payload.digest.clone(),
        source_url: payload.source_url.clone(),
    };

    db_create_definition(conn, &new_definition).context("Failed to save definition to database")
}

pub async fn create_definition_from_registry(
    conn: &mut DbConnection,
    http_client: &reqwest::Client,
    s3_client: &aws_sdk_s3::Client,
    source_input: &str,
) -> Result<Definition> {
    let source = source_utils::Source::parse(source_input)?;
    let mut payload = fetch_definition(http_client, &source)
        .await
        .context("Failed to load definition metadata")?;

    if payload.source_url.is_none() {
        payload.source_url = Some(source_input.to_string());
    }

    create_definition(conn, http_client, s3_client, &payload).await
}

pub async fn update_definition_from_source(
    conn: &mut DbConnection,
    http_client: &reqwest::Client,
    s3_client: &aws_sdk_s3::Client,
    definition_id: &str,
) -> Result<Definition> {
    let definition = get_definition(conn, definition_id)
        .context("Failed to fetch current definition from database")?;
    let source_url_str = definition
        .source_url
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Definition does not have a source_url to update from"))?;
    let source = source_utils::Source::parse(source_url_str)?;
    let remote_payload = fetch_definition(http_client, &source)
        .await
        .context("Failed to fetch updated definition metadata from source")?;

    if definition.digest == remote_payload.digest {
        return Ok(definition);
    }

    let definition_file_source = source_utils::Source::parse(&remote_payload.file_url)?;
    let obj_key = definition.id.clone();
    let body = match &definition_file_source {
        source_utils::Source::Http(url) => {
            let response = stream_utils::stream_content_from_url(http_client, url)
                .await
                .context("Failed to fetch updated definition file from URL")?;
            let stream = response.bytes_stream();
            let frames = stream.map_ok(hyper::body::Frame::data);
            let body = StreamBody::new(frames);

            ByteStream::from_body_1_x(body)
        }
        source_utils::Source::File(path) => stream_utils::stream_content_from_path(path)
            .await
            .context("Failed to read updated definition file from path")?,
    };

    s3::put_stream(
        s3_client,
        "definitions",
        &obj_key,
        body,
        Some(&remote_payload.digest),
    )
    .await
    .context("Failed to upload updated definition to S3")?;

    let update_data = UpdateDefinition {
        type_: Some(remote_payload.r#type),
        digest: Some(remote_payload.digest),
        name: Some(remote_payload.name),
        description: Some(remote_payload.description),
        ..Default::default()
    };

    db_update_definition(conn, definition_id, &update_data)
        .context("Failed to update definition in database")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::write;
    use tempfile::TempDir;
    use wiremock::matchers::{header, method, path as path_matcher};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn create_valid_payload() -> DefinitionPayload {
        DefinitionPayload {
            r#type: "test-type".to_string(),
            description: "test description".to_string(),
            file_url: "".to_string(),
            digest: "sha256:abc123".to_string(),
            source_url: None,
            id: "test-id".to_string(),
            name: "Test Definition".to_string(),
        }
    }

    #[cfg(test)]
    mod test_fetch_definition_from_path {
        use super::*;

        #[tokio::test]
        async fn test_valid_json_file() {
            let temp_dir = TempDir::new().unwrap();
            let file_path = temp_dir.path().join("definition.json");
            let payload = create_valid_payload();

            write(&file_path, serde_json::to_string(&payload).unwrap()).unwrap();

            let result = fetch_definition_from_path(&file_path).await;
            assert!(result.is_ok());

            let loaded = result.unwrap();
            assert_eq!(loaded.id, "test-id");
            assert_eq!(loaded.name, "Test Definition");
        }

        #[tokio::test]
        async fn test_invalid_json() {
            let temp_dir = TempDir::new().unwrap();
            let file_path = temp_dir.path().join("invalid.json");

            write(&file_path, "not valid json {").unwrap();

            let result = fetch_definition_from_path(&file_path).await;
            assert!(result.is_err());
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("Failed to parse definition JSON"));
        }

        #[tokio::test]
        async fn test_empty_file() {
            let temp_dir = TempDir::new().unwrap();
            let file_path = temp_dir.path().join("empty.json");

            write(&file_path, "").unwrap();

            let result = fetch_definition_from_path(&file_path).await;
            assert!(result.is_err());
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("Failed to parse definition JSON"));
        }

        #[tokio::test]
        async fn test_file_not_found() {
            let path = Path::new("/nonexistent/file.json");

            let result = fetch_definition_from_path(path).await;
            assert!(result.is_err());
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("Failed to read definition file"));
        }
    }

    #[cfg(test)]
    mod test_fetch_definition_from_url {
        use super::*;

        #[tokio::test]
        async fn test_successful_fetch() {
            let mock_server = MockServer::start().await;
            let payload = create_valid_payload();

            Mock::given(method("GET"))
                .and(path_matcher("/definition.json"))
                .and(header("User-Agent", "MCI/1.0"))
                .respond_with(ResponseTemplate::new(200).set_body_json(&payload))
                .mount(&mock_server)
                .await;

            let client = reqwest::Client::new();
            let url = format!("{}/definition.json", mock_server.uri());

            let result = fetch_definition_from_url(&client, &url).await;
            assert!(result.is_ok());

            let loaded = result.unwrap();
            assert_eq!(loaded.id, "test-id");
            assert_eq!(loaded.name, "Test Definition");
        }

        #[tokio::test]
        async fn test_404_not_found() {
            let mock_server = MockServer::start().await;

            Mock::given(method("GET"))
                .and(path_matcher("/notfound.json"))
                .respond_with(ResponseTemplate::new(404))
                .mount(&mock_server)
                .await;

            let client = reqwest::Client::new();
            let url = format!("{}/notfound.json", mock_server.uri());

            let result = fetch_definition_from_url(&client, &url).await;
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("error status"));
        }

        #[tokio::test]
        async fn test_500_server_error() {
            let mock_server = MockServer::start().await;

            Mock::given(method("GET"))
                .and(path_matcher("/error.json"))
                .respond_with(ResponseTemplate::new(500))
                .mount(&mock_server)
                .await;

            let client = reqwest::Client::new();
            let url = format!("{}/error.json", mock_server.uri());

            let result = fetch_definition_from_url(&client, &url).await;
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("error status"));
        }

        #[tokio::test]
        async fn test_invalid_json_response() {
            let mock_server = MockServer::start().await;

            Mock::given(method("GET"))
                .and(path_matcher("/invalid.json"))
                .respond_with(ResponseTemplate::new(200).set_body_string("not valid json {"))
                .mount(&mock_server)
                .await;

            let client = reqwest::Client::new();
            let url = format!("{}/invalid.json", mock_server.uri());

            let result = fetch_definition_from_url(&client, &url).await;
            assert!(result.is_err());
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("Failed to parse definition JSON"));
        }

        #[tokio::test]
        async fn test_connection_refused() {
            let client = reqwest::Client::new();

            let result =
                fetch_definition_from_url(&client, "http://localhost:59999/definition.json").await;
            assert!(result.is_err());
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("Failed to send HTTP request"));
        }

        #[tokio::test]
        async fn test_timeout() {
            let mock_server = MockServer::start().await;

            Mock::given(method("GET"))
                .and(path_matcher("/slow.json"))
                .respond_with(
                    ResponseTemplate::new(200).set_delay(std::time::Duration::from_secs(10)),
                )
                .mount(&mock_server)
                .await;

            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(1))
                .build()
                .unwrap();

            let url = format!("{}/slow.json", mock_server.uri());

            let result = fetch_definition_from_url(&client, &url).await;
            assert!(result.is_err());
        }

        #[tokio::test]
        async fn test_user_agent_is_set() {
            let mock_server = MockServer::start().await;
            let payload = create_valid_payload();

            Mock::given(method("GET"))
                .and(path_matcher("/definition.json"))
                .and(header("User-Agent", "MCI/1.0"))
                .respond_with(ResponseTemplate::new(200).set_body_json(&payload))
                .expect(1)
                .mount(&mock_server)
                .await;

            let client = reqwest::Client::new();
            let url = format!("{}/definition.json", mock_server.uri());

            let result = fetch_definition_from_url(&client, &url).await;
            assert!(result.is_ok());
        }
    }

    #[cfg(test)]
    mod test_fetch_definition {
        use super::*;
        use url::Url;

        #[tokio::test]
        async fn test_fetch_from_file_source() {
            let temp_dir = TempDir::new().unwrap();
            let file_path = temp_dir.path().join("definition.json");
            let payload = create_valid_payload();

            write(&file_path, serde_json::to_string(&payload).unwrap()).unwrap();

            let source = source_utils::Source::parse(file_path.to_str().unwrap()).unwrap();
            let client = reqwest::Client::new();

            let result = fetch_definition(&client, &source).await;
            assert!(result.is_ok());

            let loaded = result.unwrap();
            assert_eq!(loaded.id, "test-id");
        }

        #[tokio::test]
        async fn test_fetch_from_http_source() {
            let mock_server = MockServer::start().await;
            let payload = create_valid_payload();

            Mock::given(method("GET"))
                .and(path_matcher("/definition.json"))
                .respond_with(ResponseTemplate::new(200).set_body_json(&payload))
                .mount(&mock_server)
                .await;

            let url = format!("{}/definition.json", mock_server.uri());
            let source = source_utils::Source::parse(&url).unwrap();
            let client = reqwest::Client::new();

            let result = fetch_definition(&client, &source).await;
            assert!(result.is_ok());

            let loaded = result.unwrap();
            assert_eq!(loaded.id, "test-id");
        }

        #[tokio::test]
        async fn test_fetch_from_file_url_source() {
            let temp_dir = TempDir::new().unwrap();
            let file_path = temp_dir.path().join("definition.json");
            let payload = create_valid_payload();

            write(&file_path, serde_json::to_string(&payload).unwrap()).unwrap();

            let file_url = Url::from_file_path(&file_path).unwrap();
            let source = source_utils::Source::parse(file_url.as_str()).unwrap();
            let client = reqwest::Client::new();

            let result = fetch_definition(&client, &source).await;
            assert!(result.is_ok());

            let loaded = result.unwrap();
            assert_eq!(loaded.id, "test-id");
        }
    }
}
