use crate::{
    db::DbConnection,
    models::{NewSpec, Spec, UpdateSpec},
    schema::specs,
};
use diesel::prelude::*;
use serde::Deserialize;

#[derive(Debug, Deserialize, Default)]
pub struct SpecFilter {
    pub query: Option<String>,
    pub enabled: Option<bool>,
    pub spec_type: Option<String>,
}

pub fn get_spec(conn: &mut DbConnection, spec_id: &str) -> QueryResult<Spec> {
    specs::table
        .find(spec_id)
        .select(Spec::as_select())
        .first(conn)
}

pub fn list_specs(conn: &mut DbConnection, filter: SpecFilter) -> QueryResult<Vec<Spec>> {
    let mut db_query = specs::table.into_boxed();

    if let Some(search_term) = filter.query {
        let pattern = format!("%{}%", search_term);
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

pub fn update_spec(
    conn: &mut DbConnection,
    spec_id: &str,
    update: UpdateSpec,
) -> QueryResult<Spec> {
    diesel::update(specs::table.find(spec_id))
        .set(&update)
        .returning(Spec::as_returning())
        .get_result(conn)
}

pub fn delete_spec(conn: &mut DbConnection, spec_id: &str) -> QueryResult<usize> {
    diesel::delete(specs::table.find(spec_id)).execute(conn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db, models::NewSpec};
    use diesel::Connection;

    fn setup_test_db() -> DbConnection {
        let database_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/mci".to_string());
        let pool = db::create_pool(&database_url);
        let mut conn = pool.get().unwrap();

        db::run_migrations(&mut conn).expect("Failed to run migrations for test database");

        conn.begin_test_transaction().unwrap();

        conn
    }

    #[test]
    fn test_create_and_get_spec() {
        let mut conn = setup_test_db();

        let new_spec = NewSpec {
            id: "test-create-and-get".to_string(),
            spec_url: "https://example.com/spec".to_string(),
            spec_type: "openapi".to_string(),
            source_url: "https://example.com".to_string(),
            description: "Test spec".to_string(),
        };

        let created = create_spec(&mut conn, new_spec).unwrap();
        assert_eq!(created.id, "test-create-and-get");

        let retrieved = get_spec(&mut conn, "test-create-and-get").unwrap();
        assert_eq!(retrieved.id, created.id);
    }

    #[test]
    fn test_list_specs_with_filter() {
        let mut conn = setup_test_db();

        let spec1 = NewSpec {
            id: "spec-list-1".to_string(),
            spec_url: "https://example.com/1".to_string(),
            spec_type: "openapi".to_string(),
            source_url: "https://example.com".to_string(),
            description: "First spec".to_string(),
        };

        create_spec(&mut conn, spec1).unwrap();

        let filter = SpecFilter {
            query: Some("First".to_string()),
            enabled: None,
            spec_type: None,
        };
        let results = list_specs(&mut conn, filter).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "spec-list-1");
    }

    #[test]
    fn test_update_spec() {
        let mut conn = setup_test_db();

        let new_spec = NewSpec {
            id: "test-update".to_string(),
            spec_url: "https://example.com/spec".to_string(),
            spec_type: "openapi".to_string(),
            source_url: "https://example.com".to_string(),
            description: "Original".to_string(),
        };

        create_spec(&mut conn, new_spec).unwrap();

        let update = UpdateSpec {
            enabled: Some(false),
            spec_type: None,
            description: Some("Updated".to_string()),
        };
        let updated = update_spec(&mut conn, "test-update", update).unwrap();

        assert_eq!(updated.enabled, false);
        assert_eq!(updated.description, "Updated");
    }

    #[test]
    fn test_delete_spec() {
        let mut conn = setup_test_db();

        let new_spec = NewSpec {
            id: "test-delete".to_string(),
            spec_url: "https://example.com/spec".to_string(),
            spec_type: "openapi".to_string(),
            source_url: "https://example.com".to_string(),
            description: "To be deleted".to_string(),
        };

        create_spec(&mut conn, new_spec).unwrap();

        let deleted = delete_spec(&mut conn, "test-delete").unwrap();
        assert_eq!(deleted, 1);

        let result = get_spec(&mut conn, "test-delete");
        assert!(result.is_err());
    }
}
