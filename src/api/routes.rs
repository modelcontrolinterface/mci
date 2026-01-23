use crate::{api::handlers, AppState};
use axum::{
    routing::{delete, get, post, put},
    Router,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/specs", get(handlers::list_specs))
        // .route("/specs", post(handlers::create_spec))
        .route("/specs/{id}", get(handlers::get_spec))
        .route("/specs/{id}", put(handlers::update_spec))
        .route("/specs/{id}", delete(handlers::delete_spec))
}
