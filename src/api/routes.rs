use crate::{api::handlers, AppState};
use axum::{
    routing::{delete, get, patch, post},
    Router,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/definitions", get(handlers::list_definitions))
        .route("/definitions", post(handlers::create_definition))
        .route("/definitions/{id}", get(handlers::get_definition))
        .route("/definitions/{id}", delete(handlers::delete_definition))
        .route("/definitions/{id}", patch(handlers::update_definition))
        .route("/definitions/install", post(handlers::install_definition))
        .route(
            "/definitions/{id}/update",
            post(handlers::upgrade_definition),
        )

    //.route("/definitions/{id}/configuration", get(handlers::get_definition_configuration))
    // .route("/definitions/{id}/configuration", put(handlers::set_definition_configuration))
    // .route("/definitions/{id}/configuration", patch(handlers::update_definition_configuration))
    // .route("/definitions/{id}/configuration", delete(handlers::reset_definition_configuration))
    // .route("/definitions/{id}/configuration/schema", get(handlers::get_definition_configuration_schema))
    //
    // .route("/definitions/{id}/secret", get(handlers::get_definition_secrets))
    // .route("/definitions/{id}/secret", put(handlers::set_definition_secrets))
    // .route("/definitions/{id}/secret", patch(handlers::update_definition_secrets))
    // .route("/definitions/{id}/secret", delete(handlers::reset_definition_secrets))
    // .route("/definitions/{id}/secret/schema", get(handlers::get_definition_secrets_schema))
}
