use axum::body::Body;
use axum::extract::Path;
use axum::response::Response;
use axum::routing::post;
use axum::Router;
use base64::prelude::*;
use http::response::StreamInfo;
use http::BodyUtil;
use http::StatusCode;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use tracing::debug;

use crate::http;
use crate::result::Result;
use crate::room::Room;
use crate::AppState;
use crate::ROOMS;

#[derive(Serialize, Deserialize)]
struct JSON {
    room_id: i32,
    room_pass: String,
    user_id: i32,
    user_token: u32,
}

pub fn route() -> Router<AppState> {
    Router::new().route("/stream/infos/:json_base64/", post(infos))
}

async fn infos(Path(params): Path<HashMap<String, String>>) -> Result<Response> {
    debug!("HTTP GET /stream/infos");

    if !params.contains_key("json_base64") {
        return Ok(http::create_response(
            Body::from("Missing JSON".to_string()),
            StatusCode::NOT_ACCEPTABLE,
        ));
    }

    let json_obj = BASE64_STANDARD.decode(params.get("json_base64").unwrap())?;
    let json: JSON = serde_json::from_slice(&json_obj)?;

    let mut rooms = ROOMS.lock().await;

    if !rooms.contains_key(&json.room_id) {
        return Ok(http::create_response(
            Body::from(BodyUtil::ROOM_ID_NOTFOUND),
            StatusCode::NOT_ACCEPTABLE,
        ));
    }

    let room: &mut Room = rooms.get_mut(&json.room_id).unwrap();
    if !room.check_password(json.room_pass.clone()) {
        return Ok(http::create_response(
            Body::from(BodyUtil::INVILED_PASSWORD),
            StatusCode::NOT_ACCEPTABLE,
        ));
    }

    let client_map = room.client_map();
    let clients = client_map.read().await;
    let client = clients.get(&json.user_id).cloned();
    drop(clients);

    if let Some(client) = client {
        if !client.check_token(json.user_token.clone()) {
            return Ok(http::create_response(
                Body::from(BodyUtil::INVILED_TOKEN),
                StatusCode::NOT_ACCEPTABLE,
            ));
        }

        let streams = client.get_streams().await;
        let forwarder = room.forwarder();
        let forwarder = forwarder.write().await;
        let infos: Vec<StreamInfo> = forwarder
            .forward_infos(streams)
            .await
            .into_iter()
            .map(|forward_info| forward_info.into())
            .collect();
        drop(forwarder);
        let json = serde_json::to_string(&infos).unwrap();
        return Ok(http::create_response(json.into(), StatusCode::OK));
    } else {
        return Ok(http::create_response(
            Body::from(BodyUtil::UNKNOWN_ERROR),
            StatusCode::NOT_ACCEPTABLE,
        ));
    }
}
