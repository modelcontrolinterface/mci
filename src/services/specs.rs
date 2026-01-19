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
