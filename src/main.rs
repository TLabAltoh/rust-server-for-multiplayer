#[macro_use]
extern crate lazy_static;

use axum::body::{Body, Bytes};
use axum::extract::Request;
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Router;
use clap::{command, Parser};

use http_body_util::BodyExt;
use room::Room;
use route::r#static::static_server;
use std::collections::HashMap;
use std::future::IntoFuture;
use tokio::sync::Mutex;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tower_http::validate_request::ValidateRequestHeaderLayer;
use tracing::{debug, error, info, info_span, warn};

use crate::auth::ManyValidate;
use crate::config::Config;
use crate::result::Result;
use crate::route::AppState;

mod auth;
mod config;
mod constant;
mod error;
mod forward;
mod http;
mod r#macro;
mod metrics;
mod result;
mod room;
mod route;
mod rtc;
mod support;

pub const HASH_LEN: usize = 8;

lazy_static! {
    static ref ROOMS: Mutex<HashMap<i32, Room>> = Mutex::<HashMap<i32, Room>>::new(HashMap::new());
}

async fn print_request_response(
    req: Request,
    next: Next,
) -> std::result::Result<impl IntoResponse, (StatusCode, String)> {
    let req_headers = req.headers().clone();
    let (parts, body) = req.into_parts();
    let bytes = buffer_and_print("request", req_headers, body).await?;
    let req = Request::from_parts(parts, Body::from(bytes));

    let res = next.run(req).await;
    let res_headers = res.headers().clone();
    let (parts, body) = res.into_parts();
    let bytes = buffer_and_print("response", res_headers, body).await?;
    let res = Response::from_parts(parts, Body::from(bytes));

    Ok(res)
}

async fn buffer_and_print<B>(
    direction: &str,
    headers: HeaderMap,
    body: B,
) -> std::result::Result<Bytes, (StatusCode, String)>
where
    B: axum::body::HttpBody<Data = Bytes>,
    B::Error: std::fmt::Display,
{
    let bytes = match body.collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(err) => {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("failed to read {direction} body: {err}"),
            ));
        }
    };

    if let Ok(body) = std::str::from_utf8(&bytes) {
        debug!("{direction} headers = {headers:?} body = {body:?}");
    }

    Ok(bytes)
}

#[derive(Parser)]
#[command(version)]
struct Args {
    #[arg(short, long)]
    config: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let cfg = Config::parse(args.config);
    utils::set_log(format!("unity_rust_sfu={},webrtc=error", cfg.log.level));

    warn!("set log level : {}", cfg.log.level);
    debug!("config : {:?}", cfg);
    let listener = tokio::net::TcpListener::bind(&cfg.http.listen)
        .await
        .unwrap();
    let addr = listener.local_addr().unwrap();
    info!("Server listening on {}", addr);
    debug!("Debug tool shortcut http://localhost:{}", addr.port());
    let app_state = AppState {
        config: cfg.clone(),
    };
    let auth_layer = ValidateRequestHeaderLayer::custom(ManyValidate::new(vec![cfg.auth]));
    let app = Router::new()
        .merge(
            route::room::room::route()
                .merge(route::room::create::route())
                .merge(route::room::delete::route())
                .merge(route::room::join::route())
                .merge(route::room::exit::route())
                .merge(route::rtc::infos::route())
                .merge(route::rtc::stream::route())
                .merge(route::rtc::whip::route())
                .merge(route::rtc::whep::route())
                .merge(route::ws::route())
                .layer(auth_layer),
        )
        .with_state(app_state.clone())
        .layer(if cfg.http.cors {
            CorsLayer::permissive()
        } else {
            CorsLayer::new()
        })
        .layer(axum::middleware::from_fn(print_request_response))
        .layer(
            TraceLayer::new_for_http().make_span_with(|request: &Request<_>| {
                let span = info_span!(
                    "http_request",
                    uri = ?request.uri(),
                    method = ?request.method(),
                    span_id = tracing::field::Empty,
                );
                span.record("span_id", span.id().unwrap().into_u64());
                span
            }),
        );
    tokio::select! {
        Err(e) = axum::serve(listener, static_server(app)).into_future() => error!("Application error: {e}"),
        msg = signal::wait_for_stop_signal() => debug!("Received signal: {}", msg),
    }
    info!("Server shutdown");

    Ok(())
}
