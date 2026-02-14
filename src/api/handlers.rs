use crate::{
    errors::AppError,
    models::{Definition, UpdateDefinition},
    services::definitions_services::{self, DefinitionFilter, DefinitionPayload},
    AppState,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use validator::Validate;

#[derive(Debug, Deserialize, Validate)]
pub struct InstallDefinitionRequest {
    #[validate(url)]
    pub source: String,
}

pub async fn list_definitions(
    State(state): State<AppState>,
    Query(filter): Query<DefinitionFilter>,
) -> Result<Json<Vec<Definition>>, AppError> {
    let mut conn = state.db_pool.get()?;

    let definitions = tokio::task::spawn_blocking(move || {
        definitions_services::list_definitions(&mut conn, &filter)
    })
    .await??;

    Ok(Json(definitions))
}

pub async fn create_definition(
    State(state): State<AppState>,
    Json(payload): Json<DefinitionPayload>,
) -> Result<(StatusCode, Json<Definition>), AppError> {
    let db_pool = state.db_pool.clone();
    let http_client = state.http_client.clone();
    let s3_client = state.s3_client.clone();

    let definition = definitions_services::create_definition(
        &mut db_pool.get()?,
        &http_client,
        &s3_client,
        &payload,
    )
    .await?;

    Ok((StatusCode::CREATED, Json(definition)))
}

pub async fn get_definition(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Definition>, AppError> {
    let mut conn = state.db_pool.get()?;

    let definition =
        tokio::task::spawn_blocking(move || definitions_services::get_definition(&mut conn, &id))
            .await??;

    Ok(Json(definition))
}

pub async fn delete_definition(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let mut conn = state.db_pool.get()?;
    let id_for_thread = id.clone();

    let rows_deleted = tokio::task::spawn_blocking(move || {
        definitions_services::delete_definition(&mut conn, &id_for_thread)
    })
    .await??;

    if rows_deleted == 0 {
        return Err(AppError::not_found(format!(
            "Definition with id '{}' not found",
            id
        )));
    }

    Ok(StatusCode::NO_CONTENT)
}

pub async fn update_definition(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(update): Json<UpdateDefinition>,
) -> Result<Json<Definition>, AppError> {
    update.validate()?;

    let mut conn = state.db_pool.get()?;

    let definition = tokio::task::spawn_blocking(move || {
        definitions_services::update_definition(&mut conn, &id, &update)
    })
    .await??;

    Ok(Json(definition))
}

pub async fn install_definition(
    State(state): State<AppState>,
    Json(request): Json<InstallDefinitionRequest>,
) -> Result<(StatusCode, Json<Definition>), AppError> {
    request.validate()?;

    let db_pool = state.db_pool.clone();
    let http_client = state.http_client.clone();
    let s3_client = state.s3_client.clone();

    let definition = definitions_services::create_definition_from_registry(
        &mut db_pool.get()?,
        &http_client,
        &s3_client,
        &request.source,
    )
    .await?;

    Ok((StatusCode::CREATED, Json(definition)))
}

pub async fn upgrade_definition(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Definition>, AppError> {
    let db_pool = state.db_pool.clone();
    let http_client = state.http_client.clone();
    let s3_client = state.s3_client.clone();

    let definition = definitions_services::update_definition_from_source(
        &mut db_pool.get()?,
        &http_client,
        &s3_client,
        &id,
    )
    .await?;

    Ok(Json(definition))
}
