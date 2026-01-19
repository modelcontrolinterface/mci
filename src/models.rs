use diesel::prelude::*;
use crate::schema::specs;

#[derive(Queryable, Selectable)]
#[diesel(table_name = specs)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Spec {
    pub id: String,
    pub enabled: bool,
    pub spec_url: String,
    pub spec_type: String,
    pub source_url: String,
    pub description: String,
}

#[derive(Insertable)]
#[diesel(table_name = specs)]
pub struct NewSpec {
    pub id: String,
    pub enabled: bool,
    pub spec_url: String,
    pub spec_type: String,
    pub source_url: String,
    pub description: String,
}
