mod api;

use crate::scheduler::SchedulerManager;
use crate::store::Store;
use axum::{
    http::header,
    response::IntoResponse,
    routing::{delete, get, post, put},
    Router,
};
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub store: Arc<Store>,
    pub scheduler: Arc<SchedulerManager>,
}

/// Build the Axum router for the web UI + API.
pub fn router(store: Arc<Store>, scheduler: Arc<SchedulerManager>) -> Router {
    let state = AppState { store, scheduler };

    Router::new()
        // Single-page UI
        .route("/", get(page_index))
        // API — databases CRUD
        .route("/api/databases", get(api::list_databases))
        .route("/api/databases", post(api::add_database))
        .route("/api/databases/{id}", get(api::get_database))
        .route("/api/databases/{id}", put(api::update_database))
        .route("/api/databases/{id}", delete(api::delete_database))
        .route("/api/databases/{id}/test", post(api::test_database))
        .route("/api/test-connection", post(api::test_database))
        // API — per-DB data
        .route(
            "/api/databases/{id}/collectors/{name}",
            get(api::get_collector_data),
        )
        .route("/api/databases/{id}/pipeline", get(api::get_pipeline))
        .route("/api/databases/{id}/shipping", get(api::get_shipping))
        // API — shipper CRUD
        .route("/api/databases/{id}/shippers", post(api::add_shipper))
        .route(
            "/api/databases/{id}/shippers/{shipper_id}",
            put(api::update_shipper),
        )
        .route(
            "/api/databases/{id}/shippers/{shipper_id}",
            delete(api::delete_shipper),
        )
        // API — global
        .route("/api/health", get(api::health))
        // Static assets
        .route("/style.css", get(css))
        .route("/app.js", get(js))
        .route("/logo.svg", get(logo))
        .with_state(state)
}

async fn page_index() -> axum::response::Html<&'static str> {
    axum::response::Html(include_str!("assets/index.html"))
}

async fn css() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/css")],
        include_str!("assets/style.css"),
    )
}

async fn js() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/javascript")],
        include_str!("assets/app.js"),
    )
}

async fn logo() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "image/svg+xml")],
        include_str!("assets/logo.svg"),
    )
}
