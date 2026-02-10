use crate::{
    schema::definitions,
    utils::regex::{NAMESPACE_ID_REGEX, TYPE_IDENTIFIER_REGEX},
};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Queryable, Selectable, Serialize)]
#[diesel(table_name = definitions)]
#[diesel(check_for_backend(diesel::pg::Pg))]
pub struct Definition {
    pub id: String,
    pub definition_type: String,
    pub is_enabled: bool,
    pub name: String,
    pub description: String,
    pub definition_file: String,
    pub source_url: String,
}

#[derive(Insertable, Deserialize, Validate, Debug)]
#[diesel(table_name = definitions)]
pub struct NewDefinition {
    #[validate(length(min = 3, max=64), regex(path = *NAMESPACE_ID_REGEX))]
    pub id: String,

    #[validate(length(min = 3, max=64), regex(path = *TYPE_IDENTIFIER_REGEX))]
    pub definition_type: String,

    #[validate(url)]
    pub definition_file: String,

    #[validate(length(min = 3, max = 64))]
    pub name: String,

    #[validate(length(max = 300))]
    pub description: String,

    #[validate(url)]
    pub source_url: String,
}

#[derive(AsChangeset, Default, Deserialize, Validate, Debug)]
#[diesel(table_name = definitions)]
pub struct UpdateDefinition {
    pub is_enabled: Option<bool>,

    #[validate(length(min = 3, max=64), regex(path = *TYPE_IDENTIFIER_REGEX))]
    pub definition_type: Option<String>,

    #[validate(length(min = 3, max = 64))]
    pub name: String,

    #[validate(length(max = 300))]
    pub description: String,

    #[validate(url)]
    pub source_url: String,
}
