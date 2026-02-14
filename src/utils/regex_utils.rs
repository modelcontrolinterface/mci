use regex::Regex;
use std::sync::LazyLock;

pub static NAMESPACE_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-zA-Z0-9_.-]+$").unwrap());
pub static TYPE_IDENTIFIER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-zA-Z0-9_-]+$").unwrap());
pub static SHA256: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-f0-9]{64}$").unwrap());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_namespace_id() {
        assert!(NAMESPACE_ID.is_match("my_namespace"));
        assert!(NAMESPACE_ID.is_match("user.123-prod"));
        assert!(NAMESPACE_ID.is_match("A.B_c-9"));

        assert!(!NAMESPACE_ID.is_match(""));
        assert!(!NAMESPACE_ID.is_match("name space"));
        assert!(!NAMESPACE_ID.is_match("user@domain"));
    }

    #[test]
    fn test_type_identifier() {
        assert!(TYPE_IDENTIFIER.is_match("MyType"));
        assert!(TYPE_IDENTIFIER.is_match("type-123_v1"));

        assert!(!TYPE_IDENTIFIER.is_match("type!"));
        assert!(!TYPE_IDENTIFIER.is_match("type.name"));
    }

    #[test]
    fn test_sha256() {
        assert!(SHA256.is_match("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"));

        assert!(!SHA256.is_match("e3b0c442"));
        assert!(!SHA256.is_match("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855ff"));
        assert!(!SHA256.is_match("g3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"));
        assert!(!SHA256.is_match("E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855"));
    }
}
