use crate::{
    schema::definitions,
    utils::regex::{NAMESPACE_ID_REGEX, TYPE_IDENTIFIER_REGEX, SHA256_REGEX},
};
use std::borrow::Cow;
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use validator::{Validate, ValidationError};

fn validate_digest(digest: &str) -> Result<(), ValidationError> {
    let (algorithm, hash) = digest
        .split_once(':')
        .ok_or_else(|| {
            let mut error = ValidationError::new("invalid_digest_format");
            error.add_param(Cow::from("value"), &digest);
            error
        })?;
    let hash_regex = match algorithm {
        "sha256" => &SHA256_REGEX,
        _ => {
            let mut error = ValidationError::new("unsupported_digest_algorithm");
            error.add_param(Cow::from("value"), &digest);
            error.add_param(Cow::from("algorithm"), &algorithm);
            return Err(error);
        }
    };

    if hash_regex.is_match(hash) {
        Ok(())
    } else {
        let mut error = ValidationError::new("invalid_hash_format");
        error.add_param(Cow::from("value"), &digest);
        error.add_param(Cow::from("algorithm"), &algorithm);
        Err(error)
    }
}

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
    pub digest: String,
    pub source_url: Option<String>,
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

    #[validate(custom(function = "validate_digest"))]
    pub digest: String,

    #[validate(url)]
    pub source_url: Option<String>,
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

    #[validate(custom(function = "validate_digest"))]
    pub digest: String,

    #[validate(url)]
    pub source_url: Option<String>,
}
