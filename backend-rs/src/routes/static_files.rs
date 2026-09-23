// Static frontend serving + new-frontend proxy — port of the sirv setup and
// frontendProxy middleware from src/index.ts.
//
//   GET/HEAD /login, /login/{*rest}, /beta, /beta/{*rest}, /flags/{*rest},
//            /_nuxt/{*rest}, /__nuxt_devtools__/{*rest}
//       → streamed proxy to http://{FRONTEND_HOST}:{FRONTEND_PORT}{path?query}
//   anything else unmatched
//       → file from ./frontend (safe join, Content-Type by extension,
//         production cache-control), otherwise not_found()
//
// Missing files fall back to {FRONTEND_DIR}/404.html (or plain "Not found").
use std::path::{Path as FsPath, PathBuf};

use axum::body::Body;
use axum::extract::State;
use axum::http::{header, HeaderValue, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;

use crate::config::Config;
use crate::state::AppState;

/// Exact string produced by src/index.ts: `public, maxage=${5*60}, ...`.
const PROD_CACHE_CONTROL: &str =
    "public, maxage=300, s-maxage=300, stale-while-revalidate=300, stale-if-error=300";

pub fn router() -> Router<AppState> {
    Router::new()
        // Proxy the new (Nuxt) frontend — GET/HEAD only.
        .route("/login", get(frontend_proxy))
        .route("/login/{*rest}", get(frontend_proxy))
        .route("/beta", get(frontend_proxy))
        .route("/beta/{*rest}", get(frontend_proxy))
        .route("/flags/{*rest}", get(frontend_proxy))
        .route("/_nuxt/{*rest}", get(frontend_proxy))
        .route("/__nuxt_devtools__/{*rest}", get(frontend_proxy))
    // Static files are served via the router-wide fallback
    // (`serve_fallback`, wired in routes/mod.rs) — a root catch-all route
    // would conflict with parameterized /{season}/... routes.
}

pub async fn serve_path(frontend_dir: &str, config: &Config, raw: &str) -> Response {
    // Safe relative path: drop empty/"." segments, reject traversal.
    let not_found_page = tokio::fs::read_to_string(FsPath::new(frontend_dir).join("404.html"))
        .await
        .ok();
    let not_found = |page: &Option<String>| -> Response {
        match page {
            Some(html) => (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
                html.clone(),
            )
                .into_response(),
            None => (
                StatusCode::NOT_FOUND,
                [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
                "Not found".to_string(),
            )
                .into_response(),
        }
    };
    let mut rel = PathBuf::new();
    for segment in raw.split('/') {
        match segment {
            "" | "." => {}
            ".." => return not_found(&not_found_page),
            s if s.contains('\\') || s.contains('\0') => return not_found(&not_found_page),
            s => rel.push(s),
        }
    }
    if rel.as_os_str().is_empty() {
        rel.push("index.html");
    }

    let mut full = FsPath::new(frontend_dir).join(&rel);
    let bytes = match tokio::fs::read(&full).await {
        Ok(bytes) => bytes,
        Err(_) => {
            // SPA fallback: the admin/moderation panels use history routing,
            // so unknown sub-paths serve their entry HTML instead of a 404.
            let spa_entry = if raw == "/admin" || raw.starts_with("/admin/") {
                Some("admin.html")
            } else if raw == "/moderation" || raw.starts_with("/moderation/") {
                Some("moderation.html")
            } else {
                None
            };
            if let Some(entry) = spa_entry {
                match tokio::fs::read(FsPath::new(frontend_dir).join(entry)).await {
                    Ok(bytes) => {
                        let mut response = Response::new(Body::from(bytes));
                        response.headers_mut().insert(
                            header::CONTENT_TYPE,
                            HeaderValue::from_static("text/html; charset=utf-8"),
                        );
                        if config.is_production {
                            response.headers_mut().insert(
                                header::CACHE_CONTROL,
                                HeaderValue::from_static(PROD_CACHE_CONTROL),
                            );
                        }
                        return response;
                    }
                    Err(_) => return not_found(&not_found_page),
                }
            }
            // Directory index, sirv-style: /dir/ → /dir/index.html.
            full.push("index.html");
            match tokio::fs::read(&full).await {
                Ok(bytes) => bytes,
                Err(_) => return not_found(&not_found_page),
            }
        }
    };

    // Anti-automation collector: inject into HTML pages (except the admin /
    // moderation panels) when the antibot system is active.
    let mut bytes = bytes;
    let rel_name = rel.to_string_lossy();
    let serves_panel = matches!(rel_name.as_ref(), "admin.html" | "moderation.html");
    if !config.is_off() && content_type(&rel).starts_with("text/html") && !serves_panel {
        bytes = inject_fp_script(bytes);
    }

    let mut response = Response::new(Body::from(bytes));
    if let Ok(value) = HeaderValue::from_str(content_type(&rel)) {
        response.headers_mut().insert(header::CONTENT_TYPE, value);
    }
    if config.is_production {
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static(PROD_CACHE_CONTROL),
        );
    }
    response
}

/// Insert the collector script tag before the closing body tag (or append
/// when the document has none).
fn inject_fp_script(bytes: Vec<u8>) -> Vec<u8> {
    const SCRIPT_TAG: &str = r#"<script src="/fp.js" defer></script>"#;
    let mut html = String::from_utf8_lossy(&bytes).into_owned();
    match html.rfind("</body>") {
        Some(pos) => html.insert_str(pos, SCRIPT_TAG),
        None => html.push_str(SCRIPT_TAG),
    }
    html.into_bytes()
}

fn content_type(rel: &FsPath) -> &'static str {
    let ext = rel
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match ext.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "application/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "png" => "image/png",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "json" | "map" => "application/json",
        "wasm" => "application/wasm",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

// ----------------------------------------------------------------- proxy ---

/// Hop-by-hop and length/host headers must not be copied to the proxied
/// response (the stream produces its own framing).
const HOP_BY_HOP: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
    "host",
    "content-length",
];

/// GET/HEAD proxy for the new (Nuxt) frontend — port of the frontendProxy
/// middleware in src/index.ts. Streams the upstream body; 502 on failure.
async fn frontend_proxy(State(state): State<AppState>, uri: Uri) -> Response {
    let query = uri.query().unwrap_or_default();
    let target = if query.is_empty() {
        format!(
            "http://{}:{}{}",
            state.config.frontend_host,
            state.config.frontend_port,
            uri.path()
        )
    } else {
        format!(
            "http://{}:{}{}?{}",
            state.config.frontend_host,
            state.config.frontend_port,
            uri.path(),
            query
        )
    };
    let upstream = match state.http.get(target).send().await {
        Ok(resp) => resp,
        Err(err) => {
            eprintln!("[proxy] upstream error: {err}");
            return (
                StatusCode::BAD_GATEWAY,
                [("content-type", "text/plain; charset=utf-8")],
                "Bad Gateway",
            )
                .into_response();
        }
    };

    let mut builder = Response::builder().status(upstream.status());
    for (name, value) in upstream.headers() {
        let name_str = name.as_str();
        if HOP_BY_HOP.iter().any(|h| h.eq_ignore_ascii_case(name_str)) {
            continue;
        }
        if let (Ok(name), Ok(value)) = (
            axum::http::HeaderName::from_bytes(name_str.as_bytes()),
            HeaderValue::from_bytes(value.as_bytes()),
        ) {
            builder = builder.header(name, value);
        }
    }
    match builder.body(Body::from_stream(upstream.bytes_stream())) {
        Ok(response) => response,
        Err(err) => {
            eprintln!("[proxy] body error: {err}");
            (
                StatusCode::BAD_GATEWAY,
                [("content-type", "text/plain; charset=utf-8")],
                "Bad Gateway",
            )
                .into_response()
        }
    }
}
