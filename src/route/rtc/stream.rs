use axum::body::Body;
use axum::extract::Path;
use axum::response::IntoResponse;
use axum::response::Response;
use axum::routing::post;
use axum::Json;
use axum::Router;
use http::StatusCode;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;

use tracing::debug;

use crate::constant;
use crate::error::AppError;
use crate::forward::rtc::message::Layer;
use crate::http;
use crate::result::Result;
use crate::route::*;

pub fn route() -> Router<AppState> {
    Router::new()
        .route("/stream/create/:base64/", post(create))
        .merge(Router::new().route("/stream/destroy/:base64/", post(destroy)))
        .merge(Router::new().route("/stream/get_layer/:base64/", post(get_layer)))
        .merge(Router::new().route("/stream/select_layer/:base64/", post(select_layer)))
        .merge(Router::new().route("/stream/un_select_layer/:base64/", post(un_select_layer)))
}

#[derive(Serialize, Deserialize)]
struct RequestJson {
    room_id: i32,
    user_id: i32,
    token: u32,
    stream: String,
    shared_key: String,
}

#[derive(Serialize, Deserialize)]
struct SelectLayerJson {
    room_id: i32,
    user_id: i32,
    token: u32,
    stream: String,
    session: String,
    layer: String,
    shared_key: String,
}

async fn create(Path(params): Path<HashMap<String, String>>) -> Result<Response> {
    debug!("HTTP GET /stream/create");

    let request: RequestJson = match parse_base64_into_json(&params) {
        Ok(request) => request,
        Err(err_response) => return Ok(err_response),
    };

    let (room, client) = match auth_user(
        request.room_id.clone(),
        request.shared_key.clone(),
        request.user_id,
        request.token,
    )
    .await
    {
        Ok((room, client)) => (room, client),
        Err(err_response) => return Ok(err_response),
    };

    let forwarder = room.forwarder();
    let forwarder = forwarder.write().await;
    let _ = match forwarder.stream_create(request.stream.clone()).await {
        Ok(_) => {
            let mut client = client;
            client.add_stream(request.stream.clone()).await?;

            Ok(Response::builder()
                .status(StatusCode::NO_CONTENT)
                .body("".to_string())?)
        }
        Err(e) => Err(AppError::StreamAlreadyExists(e.to_string())),
    };

    return Ok(http::create_response(Body::from(""), StatusCode::OK));
}

async fn destroy(Path(params): Path<HashMap<String, String>>) -> Result<Response> {
    debug!("HTTP GET /stream/destroy");

    let request: RequestJson = match parse_base64_into_json(&params) {
        Ok(request) => request,
        Err(err_response) => return Ok(err_response),
    };

    let (room, client) = match auth_user(
        request.room_id.clone(),
        request.shared_key.clone(),
        request.user_id,
        request.token,
    )
    .await
    {
        Ok((room, client)) => (room, client),
        Err(err_response) => return Ok(err_response),
    };

    let forwarder = room.forwarder();
    let forwarder = forwarder.write().await;
    let _ = match forwarder.stream_delete(request.stream.clone()).await {
        Ok(_) => {
            let mut client = client;
            client.remove_stream(request.stream.clone()).await?;

            Ok(Response::builder()
                .status(StatusCode::NO_CONTENT)
                .body("".to_string())?)
        }
        Err(e) => Err(AppError::StreamAlreadyExists(e.to_string())),
    };

    return Ok(http::create_response(Body::from(""), StatusCode::OK));
}

async fn get_layer(Path(params): Path<HashMap<String, String>>) -> Result<Response> {
    debug!("HTTP GET /stream/get_layer");

    let request: RequestJson = match parse_base64_into_json(&params) {
        Ok(request) => request,
        Err(err_response) => return Ok(err_response),
    };

    let (room, _client) = match auth_user(
        request.room_id.clone(),
        request.shared_key.clone(),
        request.user_id,
        request.token,
    )
    .await
    {
        Ok((room, client)) => (room, client),
        Err(err_response) => return Ok(err_response),
    };

    let forwarder = room.forwarder();
    let forwarder = forwarder.write().await;
    let layers: Vec<http::response::Layer> = forwarder
        .layers(request.stream.clone())
        .await?
        .into_iter()
        .map(|layer| layer.into())
        .collect();

    return Ok(Json(layers).into_response());
}

async fn select_layer(Path(params): Path<HashMap<String, String>>) -> Result<Response> {
    debug!("HTTP GET /stream/select_layer");

    let request: SelectLayerJson = match parse_base64_into_json(&params) {
        Ok(request) => request,
        Err(err_response) => return Ok(err_response),
    };

    let (room, _client) = match auth_user(
        request.room_id.clone(),
        request.shared_key.clone(),
        request.user_id,
        request.token,
    )
    .await
    {
        Ok((room, client)) => (room, client),
        Err(err_response) => return Ok(err_response),
    };

    let forwarder = room.forwarder();
    let forwarder = forwarder.write().await;
    forwarder
        .select_layer(
            request.stream.clone(),
            request.session.clone(),
            Some(Layer {
                encoding_id: request.layer.clone(),
            }),
        )
        .await?;

    return Ok(http::create_response(Body::from(""), StatusCode::OK));
}

async fn un_select_layer(Path(params): Path<HashMap<String, String>>) -> Result<Response> {
    debug!("HTTP GET /stream/un_select_layer");

    let request: SelectLayerJson = match parse_base64_into_json(&params) {
        Ok(request) => request,
        Err(err_response) => return Ok(err_response),
    };

    let (room, _client) = match auth_user(
        request.room_id.clone(),
        request.shared_key.clone(),
        request.user_id,
        request.token,
    )
    .await
    {
        Ok((room, client)) => (room, client),
        Err(err_response) => return Ok(err_response),
    };

    let forwarder = room.forwarder();
    let forwarder = forwarder.write().await;
    forwarder
        .select_layer(
            request.stream.clone(),
            request.session.clone(),
            Some(Layer {
                encoding_id: constant::RID_DISABLE.to_string(),
            }),
        )
        .await?;

    return Ok(http::create_response(Body::from(""), StatusCode::OK));
}
