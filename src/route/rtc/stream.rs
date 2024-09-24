use axum::body::Body;
use axum::extract::Path;
use axum::response::Response;
use axum::routing::post;
use axum::Router;
use base64::prelude::*;
use http::StatusCode;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;

use tracing::debug;

use crate::error::AppError;
use crate::http::{self, BodyUtil};
use crate::result::Result;
use crate::room::Room;
use crate::route::AppState;
use crate::ROOMS;

pub fn route() -> Router<AppState> {
    Router::new()
        .route("/stream/create/:json_base64/", post(create))
        .merge(Router::new().route("/stream/destroy/:json_base64/", post(destroy)))
}

#[derive(Serialize, Deserialize)]
struct JSON {
    room_id: i32,
    room_pass: String,
    user_id: i32,
    user_token: u32,
    stream: String,
}

async fn create(Path(params): Path<HashMap<String, String>>) -> Result<Response> {
    debug!("HTTP GET /stream/create");

    if !params.contains_key("json_base64") {
        let mut body = String::default();
        body.push_str("Missing JSON");

        return Ok(http::create_response(
            Body::from(body),
            StatusCode::NOT_ACCEPTABLE,
        ));
    }

    let json_obj = BASE64_STANDARD.decode(params.get("json_base64").unwrap())?;
    let json: JSON = serde_json::from_slice(&json_obj)?;

    let mut rooms = ROOMS.lock().await;

    if rooms.contains_key(&json.room_id) {
        let room: &mut Room = rooms.get_mut(&json.room_id).unwrap();
        if room.check_password(json.room_pass.clone()) {
            let client_map = room.client_map();
            let clients = client_map.read().await;
            let client = clients.get(&json.user_id).cloned();
            drop(clients);

            if let Some(client) = client {
                if client.check_token(json.user_token.clone()) {
                    let forwarder = room.forwarder();
                    let forwarder = forwarder.write().await;
                    let _ = match forwarder.stream_create(json.stream.clone()).await {
                        Ok(_) => {
                            let mut client = client;
                            client.add_stream(json.stream.clone()).await?;

                            Ok(Response::builder()
                                .status(StatusCode::NO_CONTENT)
                                .body("".to_string())?)
                        }
                        Err(e) => Err(AppError::StreamAlreadyExists(e.to_string())),
                    };
                } else {
                    return Ok(http::create_response(
                        Body::from(BodyUtil::INVILED_TOKEN),
                        StatusCode::NOT_ACCEPTABLE,
                    ));
                }
            }
            return Ok(http::create_response(
                Body::from(BodyUtil::UNKNOWN_ERROR),
                StatusCode::NOT_ACCEPTABLE,
            ));
        } else {
            return Ok(http::create_response(
                Body::from(BodyUtil::INVILED_PASSWORD),
                StatusCode::NOT_ACCEPTABLE,
            ));
        }
    } else {
        return Ok(http::create_response(
            Body::from(BodyUtil::ROOM_ID_NOTFOUND),
            StatusCode::NOT_ACCEPTABLE,
        ));
    }
}

async fn destroy(Path(params): Path<HashMap<String, String>>) -> Result<Response> {
    debug!("HTTP GET /stream/destroy");

    if !params.contains_key("json_base64") {
        return Ok(http::create_response(
            Body::from("Missing JSON".to_string()),
            StatusCode::NOT_ACCEPTABLE,
        ));
    }

    let json_obj = BASE64_STANDARD.decode(params.get("json_base64").unwrap())?;
    let json: JSON = serde_json::from_slice(&json_obj)?;

    let mut rooms = ROOMS.lock().await;

    if rooms.contains_key(&json.room_id) {
        let room: &mut Room = rooms.get_mut(&json.room_id).unwrap();
        if room.check_password(json.room_pass.clone()) {
            let client_map = room.client_map();
            let clients = client_map.read().await;
            let client = clients.get(&json.user_id).cloned();
            drop(clients);

            if let Some(client) = client {
                if client.check_token(json.user_token.clone()) {
                    let forwarder = room.forwarder();
                    let forwarder = forwarder.write().await;
                    let _ = match forwarder.stream_delete(json.stream.clone()).await {
                        Ok(_) => {
                            let mut client = client;
                            client.remove_stream(json.stream.clone()).await?;

                            Ok(Response::builder()
                                .status(StatusCode::NO_CONTENT)
                                .body("".to_string())?)
                        }
                        Err(e) => Err(AppError::StreamAlreadyExists(e.to_string())),
                    };
                } else {
                    return Ok(http::create_response(
                        Body::from(BodyUtil::INVILED_TOKEN),
                        StatusCode::NOT_ACCEPTABLE,
                    ));
                }
            }
            return Ok(http::create_response(
                Body::from(BodyUtil::UNKNOWN_ERROR),
                StatusCode::NOT_ACCEPTABLE,
            ));
        } else {
            return Ok(http::create_response(
                Body::from(BodyUtil::INVILED_PASSWORD),
                StatusCode::NOT_ACCEPTABLE,
            ));
        }
    } else {
        return Ok(http::create_response(
            Body::from(BodyUtil::ROOM_ID_NOTFOUND),
            StatusCode::NOT_ACCEPTABLE,
        ));
    }
}
