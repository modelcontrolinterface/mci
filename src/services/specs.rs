use crate::{
    db::DbConnection,
    models::{NewSpec, Spec, UpdateSpec},
    schema::specs,
};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
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

    pub fn as_source_url(&self) -> String {
        match self {
            Self::Http(url) => url.clone(),
            Self::Path(path) => path.clone(),
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

fn create_spec_db(conn: &mut DbConnection, new_spec: NewSpec) -> QueryResult<Spec> {
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
    source: SpecSource,
) -> Result<Spec, Box<dyn std::error::Error>> {
    let source_url = source.as_source_url();

    let payload = match source {
        SpecSource::Http(url) => fetch_spec_from_url(&url, 30).await?,
        SpecSource::Path(path) => fetch_spec_from_path(&path).await?,
    };

    let new_spec = NewSpec {
        id: payload.id,
        spec_url: payload.spec_url,
        spec_type: payload.spec_type,
        source_url,
        description: payload.description,
    };

    let result = create_spec_db(conn, new_spec)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use diesel::Connection;
    use futures::StreamExt;
    use serde_json::json;
    use std::io::Write;
    use tempfile::NamedTempFile;
    use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};

    fn setup_test_db() -> DbConnection {
        let database_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/mci".to_string());
        let pool = db::create_pool(&database_url);
        let mut conn = pool.get().unwrap();
        db::run_migrations(&mut conn).expect("Migration failed");
        conn.begin_test_transaction().unwrap();
        conn
    }

    fn dummy_spec(id: &str) -> NewSpec {
        NewSpec {
            id: id.to_string(),
            spec_url: "https://example.com/spec".to_string(),
            spec_type: "openapi".to_string(),
            source_url: "https://example.com".to_string(),
            description: "Test description".to_string(),
        }
    }

    #[test]
    fn test_spec_source_parsing() {
        assert!(matches!(
            SpecSource::parse("http://e.com"),
            SpecSource::Http(_)
        ));
        assert!(matches!(
            SpecSource::parse("https://e.com"),
            SpecSource::Http(_)
        ));
        assert!(matches!(
            SpecSource::parse("path:/tmp/s"),
            SpecSource::Path(_)
        ));
        assert!(matches!(SpecSource::parse("/etc/s"), SpecSource::Path(_)));
    }

    #[test]
    fn test_crud_lifecycle() {
        let mut conn = setup_test_db();
        let id = "crud-test";

        let created = create_spec_db(&mut conn, dummy_spec(id)).unwrap();
        assert_eq!(created.id, id);

        let fetched = get_spec(&mut conn, id).unwrap();
        assert_eq!(fetched.id, id);

        let update = UpdateSpec {
            enabled: Some(false),
            ..Default::default()
        };
        let updated = update_spec(&mut conn, id, update).unwrap();
        assert!(!updated.enabled);

        assert_eq!(delete_spec(&mut conn, id).unwrap(), 1);
        assert!(get_spec(&mut conn, id).is_err());
    }

    #[test]
    fn test_list_specs() {
        let mut conn = setup_test_db();
        create_spec_db(&mut conn, dummy_spec("list-1")).unwrap();

        let filter = SpecFilter {
            query: Some("list".into()),
            ..Default::default()
        };
        let results = list_specs(&mut conn, filter).unwrap();
        assert!(!results.is_empty());
    }

    #[tokio::test]
    async fn test_fetch_spec_url() {
        let mock = MockServer::start().await;
        let body = json!({ "id": "net", "type": "oas", "spec_url": "u", "description": "d" });

        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&body))
            .mount(&mock)
            .await;

        let res = fetch_spec_from_url(&mock.uri(), 1).await.unwrap();
        assert_eq!(res.id, "net");
    }

    #[tokio::test]
    async fn test_stream_file() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"stream_data").unwrap();

        let mut stream = stream_content_from_path(file.path().to_str().unwrap())
            .await
            .unwrap();
        let mut data = Vec::new();
        while let Some(chunk) = stream.next().await {
            data.extend_from_slice(&chunk.unwrap());
        }

        assert_eq!(data, b"stream_data");
    }
}
