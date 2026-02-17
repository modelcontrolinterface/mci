use crate::errors::AppError;
use anyhow::Result;
use std::path::{Path, PathBuf};
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Http(String),
    File(PathBuf),
}

impl Source {
    pub fn parse(input: &str) -> Result<Self, AppError> {
        if input.contains("://") {
            return Self::parse_url(input);
        }
        Self::parse_file_path(input)
    }

    fn parse_url(input: &str) -> Result<Self, AppError> {
        let url = Url::parse(input).map_err(|_| AppError::invalid_source(input))?;

        match url.scheme() {
            "http" | "https" => Ok(Self::Http(input.to_string())),
            "file" => Self::parse_file_url(url, input),
            scheme => Err(AppError::unsupported_scheme(scheme)),
        }
    }

    fn parse_file_url(url: Url, input: &str) -> Result<Self, AppError> {
        let path = url.to_file_path().map_err(|()| {
            AppError::invalid_source(format!("Cannot convert file URL to path: {}", input))
        })?;

        Self::validate_and_create_file_source(path)
    }

    fn parse_file_path(input: &str) -> Result<Self, AppError> {
        let path = Path::new(input);

        if path.components().count() == 0 {
            return Err(AppError::invalid_source(input));
        }

        Self::validate_and_create_file_source(path.to_path_buf())
    }

    fn validate_and_create_file_source(path: PathBuf) -> Result<Self, AppError> {
        if !path.exists() {
            return Err(AppError::not_found(format!(
                "File does not exist: {}",
                path.display()
            )));
        }

        if !path.is_file() {
            return Err(AppError::bad_request(format!(
                "Path is not a file: {}",
                path.display()
            )));
        }

        Ok(Self::File(path))
    }

    pub fn as_path(&self) -> Option<&Path> {
        match self {
            Self::File(path) => Some(path),
            Self::Http(_) => None,
        }
    }

    pub fn as_url(&self) -> Option<&str> {
        match self {
            Self::Http(url) => Some(url),
            Self::File(_) => None,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::fs::write;
    use tempfile::TempDir;

    #[test]
    fn test_http_url() {
        let result = Source::parse("http://example.com/example.json");
        assert_eq!(
            result.unwrap(),
            Source::Http("http://example.com/example.json".to_string())
        );
    }

    #[test]
    fn test_https_url() {
        let result = Source::parse("https://example.com/example.json");
        assert_eq!(
            result.unwrap(),
            Source::Http("https://example.com/example.json".to_string())
        );
    }

    #[test]
    fn test_file_url_valid() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("example.json");
        write(&file_path, "test").unwrap();

        let file_url = Url::from_file_path(&file_path).unwrap();
        let result = Source::parse(file_url.as_str());

        assert!(result.is_ok());
        match result.unwrap() {
            Source::File(path) => assert_eq!(path, file_path),
            _ => panic!("Expected File variant"),
        }
    }

    #[test]
    fn test_file_url_not_found() {
        let result = Source::parse("file:///nonexistent/path/example.json");
        assert!(result.is_err());

        match result.unwrap_err() {
            AppError::NotFound(_) => {}
            _ => panic!("Expected NotFound error"),
        }
    }

    #[test]
    fn test_relative_path_valid() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("example.json");
        write(&file_path, "test").unwrap();

        std::env::set_current_dir(&temp_dir).unwrap();

        let result = Source::parse("./example.json");
        assert!(result.is_ok());
    }

    #[test]
    fn test_absolute_path_valid() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("example.json");
        write(&file_path, "test").unwrap();

        let result = Source::parse(file_path.to_str().unwrap());
        assert!(result.is_ok());

        match result.unwrap() {
            Source::File(path) => assert_eq!(path, file_path),
            _ => panic!("Expected File variant"),
        }
    }

    #[test]
    fn test_file_path_not_found() {
        let result = Source::parse("/nonexistent/path/example.json");
        assert!(result.is_err());

        match result.unwrap_err() {
            AppError::NotFound(_) => {}
            _ => panic!("Expected NotFound error"),
        }
    }

    #[test]
    fn test_file_path_is_directory() {
        let temp_dir = TempDir::new().unwrap();

        let result = Source::parse(temp_dir.path().to_str().unwrap());
        assert!(result.is_err());

        match result.unwrap_err() {
            AppError::BadRequest(_) => {}
            _ => panic!("Expected BadRequest error"),
        }
    }

    #[test]
    fn test_unsupported_scheme_ftp() {
        let result = Source::parse("ftp://example.com/example.json");
        assert!(result.is_err());

        match result.unwrap_err() {
            AppError::UnsupportedScheme(scheme) => assert_eq!(scheme, "ftp"),
            _ => panic!("Expected UnsupportedScheme error"),
        }
    }

    #[test]
    fn test_unsupported_scheme_s3() {
        let result = Source::parse("s3://bucket/example.json");
        assert!(result.is_err());

        match result.unwrap_err() {
            AppError::UnsupportedScheme(scheme) => assert_eq!(scheme, "s3"),
            _ => panic!("Expected UnsupportedScheme error"),
        }
    }

    #[test]
    fn test_empty_string_returns_error() {
        let result = Source::parse("");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::InvalidSource(_)));
    }

    #[test]
    fn test_http_url_with_query_params() {
        let result = Source::parse("https://example.com/def.json?version=1.0");
        assert_eq!(
            result.unwrap(),
            Source::Http("https://example.com/def.json?version=1.0".to_string())
        );
    }

    #[test]
    fn test_http_url_with_port() {
        let result = Source::parse("http://localhost:8080/example.json");
        assert_eq!(
            result.unwrap(),
            Source::Http("http://localhost:8080/example.json".to_string())
        );
    }

    #[test]
    fn test_as_path_helper() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("example.json");
        write(&file_path, "test").unwrap();

        let source = Source::parse(file_path.to_str().unwrap()).unwrap();
        assert!(source.as_path().is_some());
        assert_eq!(source.as_path().unwrap(), file_path.as_path());

        let http_source = Source::parse("http://example.com/def.json").unwrap();
        assert!(http_source.as_path().is_none());
    }

    #[test]
    fn test_as_url_helper() {
        let http_source = Source::parse("http://example.com/def.json").unwrap();
        assert!(http_source.as_url().is_some());
        assert_eq!(http_source.as_url().unwrap(), "http://example.com/def.json");

        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("example.json");
        write(&file_path, "test").unwrap();

        let file_source = Source::parse(file_path.to_str().unwrap()).unwrap();
        assert!(file_source.as_url().is_none());
    }
}
