pub mod admin;
pub mod alliance_routes;
pub mod auth_routes;
pub mod discord_routes;
pub mod fp;
pub mod leaderboard_routes;
pub mod me;
pub mod misc;
pub mod moderator;
pub mod notification_routes;
pub mod pixel_routes;
pub mod report;
pub mod static_files;

use axum::Router;

use crate::state::AppState;

pub fn api_router(state: AppState) -> Router {
    let inner: Router<AppState> = misc::router()
        .merge(auth_routes::router())
        .merge(me::router())
        .merge(pixel_routes::router())
        .merge(leaderboard_routes::router())
        .merge(alliance_routes::router())
        .merge(admin::router())
        .merge(moderator::router())
        .merge(notification_routes::router())
        .merge(report::router())
        .merge(discord_routes::router())
        .merge(fp::router());
    // JS backend strips the /api prefix for every request, so each route is
    // reachable both with and without it.
    Router::new()
        .merge(inner.clone())
        .nest("/api", inner)
        .with_state(state)
}

/// Full router: API + static frontend + 404 fallback.
/// Static files are served from the router *fallback* (anything no API/proxy
/// route claimed) — a root catch-all route would conflict with the
/// parameterized /{season}/... routes in matchit.
pub fn build_router(state: AppState) -> Router {
    let config = state.config.clone();
    let frontend_dir = state.frontend_dir.clone();
    Router::new()
        .merge(api_router(state.clone()))
        .merge(static_files::router().with_state(state.clone()))
        .fallback(move |uri: axum::http::Uri| {
            let config = config.clone();
            let frontend_dir = frontend_dir.clone();
            async move { static_files::serve_path(&frontend_dir, &config, uri.path()).await }
        })
        .layer(axum::middleware::from_fn(cache_control_middleware))
        .layer(axum::extract::DefaultBodyLimit::max(
            state.config.body_limit_bytes,
        ))
}

async fn cache_control_middleware(
    request: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let mut response = next.run(request).await;
    // Handlers may set their own policy (e.g. the customize page's no-store).
    if !response.headers().contains_key("cache-control") {
        response
            .headers_mut()
            .insert("cache-control", "private, must-revalidate".parse().unwrap());
    }
    response
}
