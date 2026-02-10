use regex::Regex;
use std::sync::LazyLock;

pub static NAMESPACE_ID_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-zA-Z0-9_.-]+$").unwrap());

pub static TYPE_IDENTIFIER_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-zA-Z0-9_-]+$").unwrap());

pub static SHA256_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-f0-9]{64}$").unwrap());

#[cfg(test)]
mod tests {
    use super::*;

    fn is_valid_namespace_id(id: &str) -> bool {
        NAMESPACE_ID_REGEX.is_match(id)
    }

    fn is_valid_type_identifier(id: &str) -> bool {
        TYPE_IDENTIFIER_REGEX.is_match(id)
    }

    #[test]
    fn test_namespace_id() {
        assert!(is_valid_namespace_id("my-namespace"));
        assert!(is_valid_namespace_id("my_namespace"));
        assert!(is_valid_namespace_id("my.namespace"));
        assert!(is_valid_namespace_id("namespace123"));
        assert!(is_valid_namespace_id("a"));

        assert!(!is_valid_namespace_id("my namespace"));
        assert!(!is_valid_namespace_id("my@namespace"));
        assert!(!is_valid_namespace_id(""));
    }

    #[test]
    fn test_type_identifier() {
        assert!(is_valid_type_identifier("type"));
        assert!(is_valid_type_identifier("MyType"));
        assert!(is_valid_type_identifier("a"));
        assert!(is_valid_type_identifier("type123"));
        assert!(is_valid_type_identifier("my-type"));
        assert!(is_valid_type_identifier("my_type"));

        assert!(!is_valid_type_identifier("my.type"));
        assert!(!is_valid_type_identifier("my type"));
        assert!(!is_valid_type_identifier("my@type"));
        assert!(!is_valid_type_identifier(""));
    }
}
