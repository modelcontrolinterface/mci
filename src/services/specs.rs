use crate::{
    db::DbConnection,
    models::{NewSpec, Spec, UpdateSpec},
    schema::specs,
};
use aws_smithy_types::byte_stream::ByteStream;
use diesel::prelude::*;
use futures::stream::TryStreamExt;
use http_body_util::StreamBody;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;
use tokio_util::io::ReaderStream;

#[derive(Debug, Deserialize, Default)]
pub struct SpecFilter {
    pub query: Option<String>,
    pub enabled: Option<bool>,
    pub spec_type: Option<String>,
}

#[derive(Debug, Clone)]
pub enum SpecSource {
    Http(String),
    Path(String),
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SpecPayload {
    pub id: String,
    #[serde(rename = "type")]
    pub spec_type: String,
    pub spec_url: String,
    pub description: String,
}

impl SpecSource {
    pub fn parse(input: &str) -> Self {
        match input.split_once(':') {
            Some(("http" | "https", _)) => Self::Http(input.to_string()),
            Some(("path", path_data)) => Self::Path(path_data.to_string()),
            _ => Self::Path(input.to_string()),
        }
    }
}

fn build_http_client(timeout_secs: u64) -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .build()
}

async fn fetch_spec_from_path(path: &str) -> Result<SpecPayload, Box<dyn std::error::Error>> {
    let metadata = fs::metadata(path).await?;
    if !metadata.is_file() {
        return Err("Path is not a file".into());
    }

    let content = fs::read_to_string(path).await?;
    let spec_payload = serde_json::from_str::<SpecPayload>(&content)?;
    Ok(spec_payload)
}

async fn fetch_spec_from_url(
    url: &str,
    timeout_secs: u64,
) -> Result<SpecPayload, Box<dyn std::error::Error>> {
    let client = build_http_client(timeout_secs)?;
    let spec_payload = client
        .get(url)
        .header("User-Agent", "MCI/1.0")
        .send()
        .await?
        .error_for_status()?
        .json::<SpecPayload>()
        .await?;
    Ok(spec_payload)
}

pub async fn stream_content_from_path(
    path: &str,
) -> Result<ReaderStream<tokio::fs::File>, Box<dyn std::error::Error>> {
    let file = tokio::fs::File::open(path).await?;
    Ok(ReaderStream::new(file))
}

pub async fn stream_content_from_url(
    url: &str,
) -> Result<reqwest::Response, Box<dyn std::error::Error>> {
    let client = build_http_client(30)?;
    let response = client
        .get(url)
        .header("User-Agent", "MCI/1.0")
        .send()
        .await?
        .error_for_status()?;
    Ok(response)
}

fn create_spec(conn: &mut DbConnection, new_spec: NewSpec) -> QueryResult<Spec> {
    diesel::insert_into(specs::table)
        .values(&new_spec)
        .returning(Spec::as_returning())
        .get_result(conn)
}

pub fn get_spec(conn: &mut DbConnection, spec_id: &str) -> QueryResult<Spec> {
    specs::table
        .find(spec_id)
        .select(Spec::as_select())
        .first(conn)
}

pub fn list_specs(conn: &mut DbConnection, filter: SpecFilter) -> QueryResult<Vec<Spec>> {
    let mut query = specs::table.into_boxed();

    if let Some(search) = filter.query {
        let pattern = format!("%{}%", search);
        query = query.filter(
            specs::id
                .ilike(pattern.clone())
                .or(specs::description.ilike(pattern)),
        );
    }
    if let Some(enabled) = filter.enabled {
        query = query.filter(specs::enabled.eq(enabled));
    }
    if let Some(spec_type) = filter.spec_type {
        query = query.filter(specs::spec_type.eq(spec_type));
    }

    query.select(Spec::as_select()).load(conn)
}

pub fn update_spec(conn: &mut DbConnection, id: &str, update: UpdateSpec) -> QueryResult<Spec> {
    diesel::update(specs::table.find(id))
        .set(&update)
        .returning(Spec::as_returning())
        .get_result(conn)
}

pub fn delete_spec(conn: &mut DbConnection, id: &str) -> QueryResult<usize> {
    diesel::delete(specs::table.find(id)).execute(conn)
}

pub async fn fetch_and_create_spec(
    conn: &mut DbConnection,
    s3_client: &aws_sdk_s3::Client,
    source_input: &str,
) -> Result<Spec, Box<dyn std::error::Error>> {
    let source_url = SpecSource::parse(source_input);

    let payload = match &source_url {
        SpecSource::Http(url) => fetch_spec_from_url(url, 30).await?,
        SpecSource::Path(path) => fetch_spec_from_path(path).await?,
    };

    if get_spec(conn, &payload.id).is_ok() {
        return Err(format!("Conflict: Spec with ID '{}' already exists", payload.id).into());
    }

    let spec_url = SpecSource::parse(&payload.spec_url);

    let body = match spec_url {
        SpecSource::Http(url) => {
            let response = reqwest::get(&url).await?;
            let stream = response.bytes_stream();
            let frames = stream.map_ok(|bytes| hyper::body::Frame::data(bytes));
            let body = StreamBody::new(frames);

            ByteStream::from_body_1_x(body)
        }
        SpecSource::Path(path) => {
            ByteStream::from_path(Path::new(&path)).await?
        }
    };

    crate::s3::upload_stream(s3_client, "specifications", &payload.id, body).await?;

    let new_spec = NewSpec {
        id: payload.id,
        spec_url: payload.spec_url,
        spec_type: payload.spec_type,
        description: payload.description,
        source_url: source_input.to_string(),
    };

    Ok(create_spec(conn, new_spec)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use aws_sdk_s3::config::http::HttpResponse;
    use aws_sdk_s3::{Client as S3Client, config::{Credentials, Region, BehaviorVersion}};
    use diesel::Connection;
    use serde_json::json;
    use std::io::Write;
    use tempfile::NamedTempFile;
    use wiremock::matchers::{method};
    use hyper::StatusCode;
    use aws_smithy_runtime::client::http::test_util::StaticReplayClient;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn setup_test_db() -> DbConnection {
        let database_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/mci".to_string());
        let pool = db::create_pool(&database_url);
        let mut conn = pool.get().unwrap();

        db::run_migrations(&mut conn).expect("Migration failed");
        conn.begin_test_transaction().unwrap();

        conn
    }

    async fn setup_mock_s3() -> S3Client {
        let http_client = StaticReplayClient::new(vec![
            aws_smithy_runtime_api::client::http::HttpResponse::new(
                http::StatusCode::OK,
                SdkBody::empty()
            )
        ]);
        let config = aws_sdk_s3::config::Builder::new()
            .behavior_version(BehaviorVersion::latest())
            .region(Region::new("us-east-1"))
            .credentials_provider(Credentials::new("test", "test", None, None, "test"))
            .http_client(http_client)
            .build();

        S3Client::from_conf(config)
    }

    fn dummy_spec_fields() -> NewSpec {
        NewSpec {
            id: "".into(),
            spec_url: "http://test.com".into(),
            spec_type: "openapi".into(),
            source_url: "http://test.com".into(),
            description: "test".into(),
        }
    }

    fn create_valid_temp_spec(id: &str) -> (NamedTempFile, String) {
        let mut file = NamedTempFile::new().unwrap();
        let path = file.path().to_str().unwrap().to_string();
        let payload = json!({
            "id": id,
            "type": "openapi",
            "spec_url": format!("path:{}", path),
            "description": "Behavioral test spec"
        });

        writeln!(file, "{}", payload.to_string()).unwrap();

        (file, path)
    }

    #[test]
    fn test_create_and_get_lifecycle() {
        let mut conn = setup_test_db();
        let id = "lifecycle-id";
        let new_spec = NewSpec {
            id: id.into(),
            ..dummy_spec_fields()
        };

        let created = create_spec(&mut conn, new_spec).expect("Should insert record");
        assert_eq!(created.id, id);

        let fetched = get_spec(&mut conn, id).expect("Should fetch record");
        assert_eq!(fetched.id, id);
    }

    #[test]
    fn test_update_record_behavior() {
        let mut conn = setup_test_db();
        let id = "update-id";

        create_spec(&mut conn, NewSpec { id: id.into(), ..dummy_spec_fields() }).unwrap();

        let update = UpdateSpec {
            description: Some("new description".into()),
            enabled: Some(false),
            ..Default::default()
        };

        let updated = update_spec(&mut conn, id, update).expect("Should update");
        assert_eq!(updated.description, "new description");
        assert!(!updated.enabled);
    }

    #[test]
    fn test_delete_record_behavior() {
        let mut conn = setup_test_db();
        let id = "delete-id";
        create_spec(&mut conn, NewSpec { id: id.into(), ..dummy_spec_fields() }).unwrap();

        let rows_deleted = delete_spec(&mut conn, id).expect("Should delete");
        assert_eq!(rows_deleted, 1);
        assert!(get_spec(&mut conn, id).is_err());
    }

    #[test]
    fn test_complex_filtering_logic() {
        let mut conn = setup_test_db();

        let spec1 = NewSpec { id: "alpha".into(), spec_type: "openapi".into(), ..dummy_spec_fields() };
        let spec2 = NewSpec { id: "beta".into(), spec_type: "asyncapi".into(), ..dummy_spec_fields() };

        create_spec(&mut conn, spec1).unwrap();
        create_spec(&mut conn, spec2).unwrap();

        let filter_type = SpecFilter { spec_type: Some("openapi".into()), ..Default::default() };
        let results = list_specs(&mut conn, filter_type).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "alpha");

        let filter_query = SpecFilter { query: Some("bet".into()), ..Default::default() };
        let results = list_specs(&mut conn, filter_query).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "beta");
    }

    #[test]
    fn test_path_resolution_behavior() {
        let cases = vec![
            ("/absolute/path/spec.json", "/absolute/path/spec.json"),
            ("./relative/path/spec.json", "./relative/path/spec.json"),
            ("path:/prefixed/path/spec.json", "/prefixed/path/spec.json"),
        ];

        for (input, expected) in cases {
            if let SpecSource::Path(actual) = SpecSource::parse(input) {
                assert_eq!(actual, expected, "Failed resolution for: {}", input);
            } else {
                panic!("Input {} should have resolved to SpecSource::Path", input);
            }
        }
    }

    #[tokio::test]
    async fn test_fetch_and_create_success_flow() {
        let mut conn = setup_test_db();
        let s3 = setup_mock_s3().await;
        let id = "success-behavior-test";
        let (_file, path) = create_valid_temp_spec(id);
        let result = fetch_and_create_spec(&mut conn, &s3, &path).await;

        assert!(result.is_ok(), "Service flow failed: {:?}", result.err());
        let spec = result.unwrap();
        assert_eq!(spec.id, id);
        assert_eq!(spec.source_url, path);
    }

    #[tokio::test]
    async fn test_conflict_prevention_integrity() {
        let mut conn = setup_test_db();
        let s3 = setup_mock_s3().await;
        let id = "conflict-id";
        let (_file, path) = create_valid_temp_spec(id);
        let existing = NewSpec {
            id: id.to_string(),
            ..dummy_spec_fields()
        };

        create_spec(&mut conn, existing).unwrap();

        let result = fetch_and_create_spec(&mut conn, &s3, &path).await;
        assert!(result.is_err());

        let err_msg = result.err().unwrap().to_string();
        assert!(err_msg.contains("already exists"), "Expected conflict error, got: {}", err_msg);
    }

    #[tokio::test]
    async fn test_id_derived_from_content_not_input() {
        let mut conn = setup_test_db();
        let s3 = setup_mock_s3().await;
        let content_id = "derived-id-123";
        let (_file, path) = create_valid_temp_spec(content_id);
        let result = fetch_and_create_spec(&mut conn, &s3, &path).await.unwrap();

        assert_eq!(result.id, content_id);
        assert!(result.id != path);
    }

    #[tokio::test]
    async fn test_fetch_from_url_behavior() {
        let mock_server = MockServer::start().await;
        let payload = json!({
            "id": "web-spec",
            "type": "openapi",
            "spec_url": "http://example.com/raw",
            "description": "Remote spec"
        });

        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&payload))
            .mount(&mock_server)
            .await;

        let res = fetch_spec_from_url(&mock_server.uri(), 1).await;

        assert!(res.is_ok());
        assert_eq!(res.unwrap().id, "web-spec");
    }
}
