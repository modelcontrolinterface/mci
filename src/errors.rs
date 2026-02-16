use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use std::fmt;
use validator::ValidationErrors;

#[derive(Debug)]
pub enum AppError {
    NotFound(String),
    Conflict(String),
    BadRequest(String),
    Validation(ValidationErrors),

    UnsupportedScheme(String),
    InvalidSource(String),

    Internal(anyhow::Error),
    Pool(diesel::r2d2::PoolError),
    Database(diesel::result::Error),
    TaskJoin(tokio::task::JoinError),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::Conflict(msg) => write!(f, "Conflict: {}", msg),
            AppError::NotFound(msg) => write!(f, "Not found: {}", msg),
            AppError::BadRequest(msg) => write!(f, "Bad request: {}", msg),
            AppError::Validation(err) => write!(f, "Validation error: {}", err),

            AppError::UnsupportedScheme(scheme) => write!(f, "Unsupported scheme: '{}'", scheme),
            AppError::InvalidSource(msg) => {
                write!(f, "Invalid  source: {}", msg)
            }

            AppError::Internal(err) => write!(f, "Internal error: {}", err),
            AppError::Database(err) => write!(f, "Database error: {}", err),
            AppError::TaskJoin(err) => write!(f, "Task join error: {}", err),
            AppError::Pool(err) => write!(f, "Connection pool error: {}", err),
        }
    }
}

impl std::error::Error for AppError {}

impl From<ValidationErrors> for AppError {
    fn from(err: ValidationErrors) -> Self {
        AppError::Validation(err)
    }
}

impl From<diesel::r2d2::PoolError> for AppError {
    fn from(err: diesel::r2d2::PoolError) -> Self {
        AppError::Pool(err)
    }
}

impl From<diesel::result::Error> for AppError {
    fn from(err: diesel::result::Error) -> Self {
        match err {
            diesel::result::Error::NotFound => AppError::NotFound("Resource not found".to_string()),
            _ => AppError::Database(err),
        }
    }
}

impl From<tokio::task::JoinError> for AppError {
    fn from(err: tokio::task::JoinError) -> Self {
        AppError::TaskJoin(err)
    }
}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        AppError::Internal(err)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_type, message) = match &self {
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "bad_request", msg.clone()),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, "not_found", msg.clone()),
            AppError::Conflict(msg) => (StatusCode::CONFLICT, "conflict", msg.clone()),
            AppError::Validation(errors) => (
                StatusCode::BAD_REQUEST,
                "validation_error",
                format_validation_errors(errors),
            ),

            AppError::InvalidSource(msg) => {
                (StatusCode::BAD_REQUEST, "invalid_source", msg.clone())
            }
            AppError::UnsupportedScheme(scheme) => (
                StatusCode::BAD_REQUEST,
                "unsupported_scheme",
                format!("Unsupported scheme: '{}'", scheme),
            ),

            AppError::Database(err) => {
                tracing::error!("Database error: {:?}", err);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "database_error",
                    "A database error occurred".to_string(),
                )
            }
            AppError::Pool(err) => {
                tracing::error!("Connection pool error: {:?}", err);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "pool_error",
                    "Database connection error".to_string(),
                )
            }
            AppError::TaskJoin(err) => {
                tracing::error!("Task join error: {:?}", err);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "task_error",
                    "Task execution failed".to_string(),
                )
            }
            AppError::Internal(err) => {
                tracing::error!("Internal error: {:?}", err);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "An internal error occurred".to_string(),
                )
            }
        };

        let body = Json(json!({
            "error": {
                "type": error_type,
                "message": message,
            }
        }));

        (status, body).into_response()
    }
}

fn format_validation_errors(errors: &ValidationErrors) -> String {
    let mut messages = Vec::new();

    for (field, field_errors) in errors.field_errors() {
        for error in field_errors {
            let message = error
                .message
                .as_ref()
                .map(|m| m.to_string())
                .unwrap_or_else(|| format!("Invalid value for field '{}'", field));
            messages.push(message);
        }
    }

    messages.join(", ")
}

impl AppError {
    pub fn bad_request(msg: impl Into<String>) -> Self {
        AppError::BadRequest(msg.into())
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        AppError::NotFound(msg.into())
    }

    pub fn conflict(msg: impl Into<String>) -> Self {
        AppError::Conflict(msg.into())
    }

    pub fn internal(err: impl Into<anyhow::Error>) -> Self {
        AppError::Internal(err.into())
    }

    pub fn invalid_source(msg: impl Into<String>) -> Self {
        AppError::InvalidSource(msg.into())
    }

    pub fn unsupported_scheme(scheme: impl Into<String>) -> Self {
        AppError::UnsupportedScheme(scheme.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;
    use validator::Validate;

    #[derive(Debug, Validate)]
    struct TestStruct {
        #[validate(length(min = 3))]
        name: String,
        #[validate(range(min = 18))]
        age: i32,
    }

    #[test]
    fn test_format_validation_errors_with_custom_messages() {
        let test = TestStruct {
            name: "ab".to_string(),
            age: 15,
        };
        let errors = test.validate().unwrap_err();
        let formatted = format_validation_errors(&errors);

        assert!(formatted.contains("name") || formatted.contains("age"));
    }

    #[tokio::test]
    async fn test_app_error_not_found_response() {
        let error = AppError::not_found("User not found");
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let body = response.into_body();
        let bytes = body.collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        assert_eq!(json["error"]["type"], "not_found");
        assert_eq!(json["error"]["message"], "User not found");
    }

    #[tokio::test]
    async fn test_app_error_bad_request_response() {
        let error = AppError::bad_request("Invalid input");
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = response.into_body();
        let bytes = body.collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        assert_eq!(json["error"]["type"], "bad_request");
        assert_eq!(json["error"]["message"], "Invalid input");
    }

    #[tokio::test]
    async fn test_app_error_validation_response() {
        let test = TestStruct {
            name: "ab".to_string(),
            age: 15,
        };
        let validation_errors = test.validate().unwrap_err();
        let error = AppError::from(validation_errors);
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = response.into_body();
        let bytes = body.collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        assert_eq!(json["error"]["type"], "validation_error");
        assert!(!json["error"]["message"].as_str().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_app_error_internal_hides_details() {
        let error = AppError::internal(anyhow::anyhow!("Sensitive database password exposed"));
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

        let body = response.into_body();
        let bytes = body.collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        assert_eq!(json["error"]["type"], "internal_error");
        assert_eq!(json["error"]["message"], "An internal error occurred");
        assert!(!json["error"]["message"]
            .as_str()
            .unwrap()
            .contains("password"));
    }

    #[test]
    fn test_diesel_not_found_converts_to_app_error_not_found() {
        let diesel_error = diesel::result::Error::NotFound;
        let app_error = AppError::from(diesel_error);

        match app_error {
            AppError::NotFound(msg) => {
                assert_eq!(msg, "Resource not found");
            }
            _ => panic!("Expected NotFound variant"),
        }
    }

    #[test]
    fn test_diesel_other_error_converts_to_database_error() {
        let diesel_error = diesel::result::Error::DatabaseError(
            diesel::result::DatabaseErrorKind::UniqueViolation,
            Box::new("test".to_string()),
        );
        let app_error = AppError::from(diesel_error);

        match app_error {
            AppError::Database(_) => {}
            _ => panic!("Expected Database variant"),
        }
    }

    #[tokio::test]
    async fn test_app_error_invalid_source_response() {
        let error = AppError::invalid_source("invalid://source");
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = response.into_body();
        let bytes = body.collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        assert_eq!(json["error"]["type"], "invalid_source");
        assert!(json["error"]["message"]
            .as_str()
            .unwrap()
            .contains("invalid://source"));
    }

    #[tokio::test]
    async fn test_app_error_unsupported_scheme_response() {
        let error = AppError::unsupported_scheme("ftp");
        let response = error.into_response();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = response.into_body();
        let bytes = body.collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        assert_eq!(json["error"]["type"], "unsupported_scheme");
        assert!(json["error"]["message"].as_str().unwrap().contains("ftp"));
    }

    #[test]
    fn test_invalid_source_display() {
        let error = AppError::invalid_source("bad-input");
        let display = format!("{}", error);
        assert!(display.contains("Invalid  source"));
        assert!(display.contains("bad-input"));
    }

    #[test]
    fn test_unsupported_scheme_display() {
        let error = AppError::unsupported_scheme("ftp");
        let display = format!("{}", error);
        assert!(display.contains("Unsupported scheme"));
        assert!(display.contains("ftp"));
    }
}
