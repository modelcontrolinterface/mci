use crate::{
    db::DbConnection,
    models::{NewSpec, Spec, UpdateSpec},
    schema::specs,
};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use tokio::fs;

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

    fn as_source_url(&self) -> String {
        match self {
            Self::Http(url) => url.clone(),
            Self::Path(path) => path.clone(),
        }
    }
}

async fn fetch_spec_from_url(
    url: &str,
    timeout_secs: u64,
) -> Result<SpecPayload, Box<dyn std::error::Error>> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .build()?;

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

async fn fetch_spec_from_path(path: &str) -> Result<SpecPayload, Box<dyn std::error::Error>> {
    let metadata = fs::metadata(path).await?;

    if !metadata.is_file() {
        return Err("Path is not a file".into());
    }

    let content = fs::read_to_string(path).await?;
    let spec_payload = serde_json::from_str::<SpecPayload>(&content)?;

    Ok(spec_payload)
}

fn create_spec_internal(conn: &mut DbConnection, new_spec: NewSpec) -> QueryResult<Spec> {
    diesel::insert_into(specs::table)
        .values(&new_spec)
        .returning(Spec::as_returning())
        .get_result(conn)
}

pub async fn fetch_spec(
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

    let result = create_spec_internal(conn, new_spec)?;

    Ok(result)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use diesel::Connection;
    use serde_json::json;
    use tempfile::TempDir;
    use wiremock::matchers::{method, path};
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
    fn test_spec_source_parse_http() {
        let source = SpecSource::parse("http://example.com/spec");
        match source {
            SpecSource::Http(url) => assert_eq!(url, "http://example.com/spec"),
            _ => panic!("Expected Http source"),
        }
    }

    #[test]
    fn test_spec_source_parse_https() {
        let source = SpecSource::parse("https://example.com/spec");
        match source {
            SpecSource::Http(url) => assert_eq!(url, "https://example.com/spec"),
            _ => panic!("Expected Http source"),
        }
    }

    #[test]
    fn test_spec_source_parse_explicit_path() {
        let source = SpecSource::parse("path:/var/tmp/spec.json");
        match source {
            SpecSource::Path(path) => assert_eq!(path, "/var/tmp/spec.json"),
            _ => panic!("Expected Path source"),
        }
    }

    #[test]
    fn test_spec_source_parse_implicit_path() {
        let test_cases = vec!["/etc/spec.yaml", "my_spec_file.json", "./relative/path.json"];

        for input in test_cases {
            let source = SpecSource::parse(input);
            match source {
                SpecSource::Path(path) => assert_eq!(path, input),
                _ => panic!("Expected Path source for input: {}", input),
            }
        }
    }

    #[test]
    fn test_create_and_get_spec() {
        let mut conn = setup_test_db();
        let id = "test-create";

        let created = create_spec_internal(&mut conn, dummy_spec(id)).unwrap();
        assert_eq!(created.id, id);

        let retrieved = get_spec(&mut conn, id).unwrap();
        assert_eq!(retrieved.id, id);
        assert_eq!(retrieved.spec_type, "openapi");
    }

    #[test]
    fn test_update_spec() {
        let mut conn = setup_test_db();
        let id = "test-update";

        create_spec_internal(&mut conn, dummy_spec(id)).unwrap();

        let update = UpdateSpec {
            enabled: Some(false),
            spec_type: Some("graphql".to_string()),
            ..Default::default()
        };

        let updated = update_spec(&mut conn, id, update).unwrap();
        assert!(!updated.enabled);
        assert_eq!(updated.spec_type, "graphql");
    }

    #[test]
    fn test_delete_spec() {
        let mut conn = setup_test_db();
        let id = "test-delete";

        create_spec_internal(&mut conn, dummy_spec(id)).unwrap();

        let deleted_count = delete_spec(&mut conn, id).unwrap();
        assert_eq!(deleted_count, 1);

        assert!(get_spec(&mut conn, id).is_err());
    }

    #[test]
    fn test_crud_lifecycle() {
        let mut conn = setup_test_db();
        let id = "lifecycle-test";

        let created = create_spec_internal(&mut conn, dummy_spec(id)).unwrap();
        assert_eq!(created.id, id);

        let update = UpdateSpec {
            enabled: Some(false),
            ..Default::default()
        };
        let updated = update_spec(&mut conn, id, update).unwrap();
        assert!(!updated.enabled);

        let deleted_count = delete_spec(&mut conn, id).unwrap();
        assert_eq!(deleted_count, 1);
        assert!(get_spec(&mut conn, id).is_err());
    }

    #[test]
    fn test_list_specs_with_query_filter() {
        let mut conn = setup_test_db();

        create_spec_internal(&mut conn, dummy_spec("filter-1")).unwrap();
        create_spec_internal(&mut conn, dummy_spec("other-2")).unwrap();

        let filter = SpecFilter {
            query: Some("filter".to_string()),
            ..Default::default()
        };

        let results = list_specs(&mut conn, filter).unwrap();
        assert!(!results.is_empty());
        assert!(results.iter().any(|s| s.id.contains("filter")));
    }

    #[test]
    fn test_list_specs_with_type_filter() {
        let mut conn = setup_test_db();

        let mut spec1 = dummy_spec("spec-1");
        spec1.spec_type = "openapi".to_string();
        create_spec_internal(&mut conn, spec1).unwrap();

        let mut spec2 = dummy_spec("spec-2");
        spec2.spec_type = "graphql".to_string();
        create_spec_internal(&mut conn, spec2).unwrap();

        let filter = SpecFilter {
            spec_type: Some("graphql".to_string()),
            ..Default::default()
        };

        let results = list_specs(&mut conn, filter).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].spec_type, "graphql");
    }

    #[tokio::test]
    async fn test_fetch_spec_from_url_success() {
        let mock_server = MockServer::start().await;
        let json_response = json!({
            "id": "remote-spec",
            "type": "openapi",
            "spec_url": "http://example.com/api",
            "description": "Remote spec"
        });

        Mock::given(method("GET"))
            .and(path("/spec.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&json_response))
            .mount(&mock_server)
            .await;

        let url = format!("{}/spec.json", mock_server.uri());
        let result = fetch_spec_from_url(&url, 5).await.unwrap();

        assert_eq!(result.id, "remote-spec");
        assert_eq!(result.spec_type, "openapi");
    }

    #[tokio::test]
    async fn test_fetch_spec_from_url_timeout() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/slow"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(std::time::Duration::from_secs(10))
                    .set_body_json(json!({"id": "test"})),
            )
            .mount(&mock_server)
            .await;

        let url = format!("{}/slow", mock_server.uri());
        let result = fetch_spec_from_url(&url, 1).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_fetch_spec_from_path_success() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("spec.json");
        let content = json!({
            "id": "file-spec",
            "type": "graphql",
            "spec_url": "http://local",
            "description": "Local spec"
        });

        fs::write(&file_path, content.to_string()).await.unwrap();

        let result = fetch_spec_from_path(file_path.to_str().unwrap())
            .await
            .unwrap();

        assert_eq!(result.id, "file-spec");
        assert_eq!(result.spec_type, "graphql");
    }

    #[tokio::test]
    async fn test_fetch_spec_from_path_not_a_file() {
        let temp_dir = TempDir::new().unwrap();
        let dir_path = temp_dir.path();

        let result = fetch_spec_from_path(dir_path.to_str().unwrap()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_fetch_spec_from_path_file_not_found() {
        let result = fetch_spec_from_path("/nonexistent/path/spec.json").await;
        assert!(result.is_err());
    }
}
