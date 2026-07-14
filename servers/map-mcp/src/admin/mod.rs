mod error;
mod handlers;

use std::sync::Arc;

use axum::{
    Router,
    routing::{get, post},
};

use crate::state::MapApplication;

pub fn router(state: Arc<MapApplication>) -> Router {
    Router::new()
        .route(
            "/sources",
            get(handlers::list_sources).post(handlers::create_source),
        )
        .route(
            "/sources/{source_id}",
            get(handlers::get_source).put(handlers::replace_source),
        )
        .route(
            "/sources/{source_id}/disable",
            post(handlers::disable_source),
        )
        .route(
            "/acquisitions",
            get(handlers::list_acquisitions).post(handlers::create_acquisition),
        )
        .route(
            "/acquisitions/{acquisition_id}",
            get(handlers::get_acquisition),
        )
        .route(
            "/acquisitions/{acquisition_id}/cancel",
            post(handlers::cancel_acquisition),
        )
        .route("/releases", get(handlers::list_releases))
        .route("/active-releases", get(handlers::list_active_releases))
        .route("/releases/{release_id}", get(handlers::get_release))
        .route(
            "/releases/{release_id}/activate",
            post(handlers::activate_release),
        )
        .route(
            "/releases/{release_id}/rollback",
            post(handlers::rollback_release),
        )
        .route(
            "/releases/{release_id}/quarantine",
            post(handlers::quarantine_release),
        )
        .route(
            "/mobility-profiles",
            get(handlers::list_mobility_profiles).post(handlers::create_mobility_profile),
        )
        .route(
            "/mobility-profiles/{profile_id}/versions/{version}",
            get(handlers::get_mobility_profile),
        )
        .with_state(state)
}
