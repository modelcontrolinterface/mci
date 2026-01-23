use crate::{
    schema::specs,
    utils::regex::{NAMESPACE_ID_REGEX, TYPE_IDENTIFIER_REGEX},
};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Queryable, Selectable, Serialize)]
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

#[derive(Insertable, Deserialize, Validate, Debug)]
#[diesel(table_name = specs)]
pub struct NewSpec {
    #[validate(length(min = 3), regex(path = *NAMESPACE_ID_REGEX))]
    pub id: String,

    #[validate(url)]
    pub spec_url: String,

    #[validate(length(min = 3), regex(path = *TYPE_IDENTIFIER_REGEX))]
    pub spec_type: String,

    #[validate(url)]
    pub source_url: String,

    #[validate(length(max = 500))]
    pub description: String,
}

#[derive(AsChangeset, Default, Deserialize, Validate, Debug)]
#[diesel(table_name = specs)]
pub struct UpdateSpec {
    pub enabled: Option<bool>,

    #[validate(length(min = 3), regex(path = *TYPE_IDENTIFIER_REGEX))]
    pub spec_type: Option<String>,

    #[validate(length(max = 500))]
    pub description: Option<String>,
}
