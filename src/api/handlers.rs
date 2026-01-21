use crate::{
    errors::AppError,
    models::{NewSpec, Spec, UpdateSpec},
    services::specs::{self as service, SpecFilter},
    AppState,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use validator::Validate;

pub async fn get_spec(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Spec>, AppError> {
    let mut conn = state.db_pool.get()?;
    let spec = tokio::task::spawn_blocking(move || service::get_spec(&mut conn, &id)).await??;

    Ok(Json(spec))
}

pub async fn list_specs(
    State(state): State<AppState>,
    Query(filter): Query<SpecFilter>,
) -> Result<Json<Vec<Spec>>, AppError> {
    let mut conn = state.db_pool.get()?;
    let specs =
        tokio::task::spawn_blocking(move || service::list_specs(&mut conn, filter)).await??;

    Ok(Json(specs))
}

pub async fn create_spec(
    State(state): State<AppState>,
    Json(new_spec): Json<NewSpec>,
) -> Result<(StatusCode, Json<Spec>), AppError> {
    new_spec.validate()?;

    let mut conn = state.db_pool.get()?;
    let spec =
        tokio::task::spawn_blocking(move || service::create_spec(&mut conn, new_spec)).await??;

    Ok((StatusCode::CREATED, Json(spec)))
}

pub async fn update_spec(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(update): Json<UpdateSpec>,
) -> Result<Json<Spec>, AppError> {
    update.validate()?;

    let mut conn = state.db_pool.get()?;
    let spec =
        tokio::task::spawn_blocking(move || service::update_spec(&mut conn, &id, update)).await??;

    Ok(Json(spec))
}

pub async fn delete_spec(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let mut conn = state.db_pool.get()?;

    tokio::task::spawn_blocking(move || service::delete_spec(&mut conn, &id)).await??;

    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use crate::{app, AppState};
    use crate::{db, s3};
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        Router,
    };
    use serde_json::json;
    use tower::ServiceExt;

    async fn setup_test_app() -> Router {
        let database_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/mci".to_string());
        let pool = db::create_pool(&database_url);
        let mut conn = pool
            .get()
            .expect("Failed to get database connection for migrations");

        tokio::task::spawn_blocking(move || db::run_migrations(&mut conn))
            .await
            .expect("Migration task panicked")
            .expect("Failed to run migrations");

        let app_state = AppState {
            db_pool: pool,
            s3_client: s3::create_s3_client("http://localhost:9000", "test", "test").await,
        };

        crate::app(app_state)
    }

    #[tokio::test]
    async fn test_create_spec_success() {
        let app = setup_test_app().await;
        let new_spec = json!({
            "id": "test-spec",
            "spec_url": "https://example.com/spec",
            "spec_type": "openapi",
            "source_url": "https://example.com",
            "description": "Test"
        });
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/specs")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&new_spec).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn test_create_spec_validation_error() {
        let app = setup_test_app().await;
        let invalid_spec = json!({
            "id": "a",
            "spec_url": "not-a-url",
            "spec_type": "openapi",
            "source_url": "https://example.com",
            "description": "Test"
        });
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/specs")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&invalid_spec).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_get_spec_not_found() {
        let app = setup_test_app().await;
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/specs/nonexistent")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
