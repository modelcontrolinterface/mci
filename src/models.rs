use crate::{schema::definitions, utils::regex_utils};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use validator::{Validate, ValidationError};

fn validate_digest(digest: &str) -> Result<(), ValidationError> {
    let (algorithm, hash) = digest.split_once(':').ok_or_else(|| {
        let mut error = ValidationError::new("invalid_digest_format");
        error.add_param(Cow::from("value"), &digest);
        error
    })?;
    let hash_regex = match algorithm {
        "sha256" => &regex_utils::SHA256,
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
    pub type_: String,
    pub is_enabled: bool,
    pub name: String,
    pub description: String,
    pub definition_object_key: String,
    pub configuration_object_key: String,
    pub secrets_object_key: String,
    pub digest: String,
    pub source_url: Option<String>,
}

#[derive(Insertable, Deserialize, Validate, Debug)]
#[diesel(table_name = definitions)]
pub struct NewDefinition {
    #[validate(length(min = 3, max = 64), regex(path = *regex_utils::NAMESPACE_ID))]
    pub id: String,

    #[validate(length(min = 3, max = 64), regex(path = *regex_utils::TYPE_IDENTIFIER))]
    pub type_: String,

    #[validate(length(min = 3, max = 64))]
    pub name: String,

    #[validate(length(max = 300))]
    pub description: String,

    pub definition_object_key: String,

    pub configuration_object_key: String,

    pub secrets_object_key: String,

    #[validate(custom(function = "validate_digest"))]
    pub digest: String,

    #[validate(url)]
    pub source_url: Option<String>,
}

#[derive(AsChangeset, Default, Deserialize, Validate, Debug)]
#[diesel(table_name = definitions)]
pub struct UpdateDefinition {
    pub is_enabled: Option<bool>,

    #[validate(length(min = 3, max = 64), regex(path = *regex_utils::TYPE_IDENTIFIER))]
    pub type_: Option<String>,

    #[validate(length(min = 3, max = 64))]
    pub name: Option<String>,

    #[validate(length(max = 300))]
    pub description: Option<String>,

    #[validate(custom(function = "validate_digest"))]
    pub digest: Option<String>,

    #[validate(url)]
    pub source_url: Option<String>,
}
