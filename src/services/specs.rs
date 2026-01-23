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

#[derive(Debug, Deserialize, Serialize)]
pub struct SpecPayload {
    pub id: String,
    #[serde(rename = "type")]
    pub spec_type: String,
    pub spec_url: String,
    pub description: String,
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

pub fn get_spec(conn: &mut DbConnection, spec_id: &str) -> QueryResult<Spec> {
    specs::table
        .find(spec_id)
        .select(Spec::as_select())
        .first(conn)
}

pub fn list_specs(conn: &mut DbConnection, filter: SpecFilter) -> QueryResult<Vec<Spec>> {
    let mut db_query = specs::table.into_boxed();

    if let Some(search) = filter.query {
        let pattern = format!("%{}%", search);
        db_query = db_query.filter(
            specs::id
                .ilike(pattern.clone())
                .or(specs::description.ilike(pattern)),
        );
    }
    if let Some(enabled) = filter.enabled {
        db_query = db_query.filter(specs::enabled.eq(enabled));
    }
    if let Some(stype) = filter.spec_type {
        db_query = db_query.filter(specs::spec_type.eq(stype));
    }

    db_query.select(Spec::as_select()).load(conn)
}

pub fn create_spec(conn: &mut DbConnection, new_spec: NewSpec) -> QueryResult<Spec> {
    diesel::insert_into(specs::table)
        .values(&new_spec)
        .returning(Spec::as_returning())
        .get_result(conn)
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
    fn test_crud_lifecycle() {
        let mut conn = setup_test_db();
        let id = "lifecycle-test";
        let created = create_spec(&mut conn, dummy_spec(id)).unwrap();

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
    fn test_list_specs_filtering() {
        let mut conn = setup_test_db();

        create_spec(&mut conn, dummy_spec("filter-1")).unwrap();

        let filter = SpecFilter {
            query: Some("filter".to_string()),
            ..Default::default()
        };
        let results = list_specs(&mut conn, filter).unwrap();

        assert!(!results.is_empty());
    }

    #[tokio::test]
    async fn test_fetch_spec_from_url_success() {
        let mock_server = MockServer::start().await;
        let json_response = json!({
            "id": "test",
            "type": "openapi",
            "spec_url": "http://example.com",
            "description": "desc"
        });

        Mock::given(method("GET"))
            .and(path("/spec.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&json_response))
            .mount(&mock_server)
            .await;

        let url = format!("{}/spec.json", mock_server.uri());
        let res = fetch_spec_from_url(&url, 5).await.unwrap();

        assert_eq!(res.id, "test");
    }

    #[tokio::test]
    async fn test_fetch_spec_from_path_success() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("spec.json");
        let content = json!({
            "id": "file-spec",
            "type": "graphql",
            "spec_url": "http://local",
            "description": "local"
        });

        fs::write(&file_path, content.to_string()).await.unwrap();

        let res = fetch_spec_from_path(file_path.to_str().unwrap())
            .await
            .unwrap();

        assert_eq!(res.id, "file-spec");
    }
}
