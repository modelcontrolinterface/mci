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

#[derive(Serialize)]
pub struct Build {
    pub id: i32,
    pub name: String,
    pub status: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_digest_valid_sha256() {
        let digest = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        assert!(validate_digest(digest).is_ok());
    }

    #[test]
    fn test_validate_digest_missing_colon() {
        let digest = "sha256e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        let result = validate_digest(digest);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, "invalid_digest_format");
    }

    #[test]
    fn test_validate_digest_excess_colon() {
        let digest = "sha256::e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        let result = validate_digest(digest);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, "invalid_hash_format");
    }

    #[test]
    fn test_validate_digest_unsupported_algorithm() {
        let digest = "md5:098f6bcd4621d373cade4e832627b4f6";
        let result = validate_digest(digest);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, "unsupported_digest_algorithm");
    }

    #[test]
    fn test_validate_digest_invalid_hash_format() {
        let digest = "sha256:invalid_hash";
        let result = validate_digest(digest);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, "invalid_hash_format");
    }
}
