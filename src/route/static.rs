use axum::Router;
#[cfg(debug_assertions)]
use tower_http::services::{ServeDir, ServeFile};

#[cfg(not(debug_assertions))]
use {
    axum::response::IntoResponse,
    http::{header, StatusCode, Uri},
    rust_embed::RustEmbed,
};

#[cfg(not(debug_assertions))]
#[derive(RustEmbed)]
#[folder = "assets/webui/"]
struct Assets;

pub fn static_server(router: Router) -> Router {
    #[cfg(debug_assertions)]
    {
        let serve_dir = ServeDir::new("assets/webui")
            .not_found_service(ServeFile::new("assets/webui/debug-tool.html"));

        router.nest_service("/", serve_dir.clone())
    }
    #[cfg(not(debug_assertions))]
    {
        router.fallback(static_handler)
    }
}

#[cfg(not(debug_assertions))]
async fn static_handler(uri: Uri) -> impl IntoResponse {
    let mut path = uri.path().trim_start_matches('/');
    if path.is_empty() {
        path = "debug-tool.html";
    }
    match Assets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            ([(header::CONTENT_TYPE, mime.as_ref())], content.data).into_response()
        }
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}
