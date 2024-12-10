use axum::body::Body;
use axum::extract::Path;
use axum::response::Response;
use axum::routing::post;
use axum::Router;
use http::BodyUtil;
use http::StatusCode;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use tracing::debug;

use crate::http;
use crate::result::Result;
use crate::room::Room;
use crate::route::room::RoomInfoJson;
use crate::route::*;
use crate::ROOMS;

pub fn route() -> Router<AppState> {
    Router::new()
        .route("/room", post(room))
        .merge(Router::new().route("/room/:base64/", post(room_specific)))
}

#[derive(Serialize, Deserialize)]
struct RequestJson {
    id: i32,
    shared_key: String,
}

#[derive(Serialize, Deserialize)]
struct ResponseJson {
    infos: Vec<RoomInfoJson>,
}

async fn room() -> Result<Response> {
    debug!("HTTP GET /room");

    let rooms = ROOMS.lock().await;

    let mut response = ResponseJson { infos: Vec::new() };

    for (_room_id, room) in rooms.iter() {
        if room.is_public() {
            response.infos.push(room.info());
        }
    }

    return Ok(http::create_response(
        Body::from(serde_json::to_string(&response).unwrap()),
        StatusCode::OK,
    ));
}

async fn room_specific(Path(params): Path<HashMap<String, String>>) -> Result<Response> {
    println!("HTTP GET /room");

    let request: RequestJson = match parse_base64_into_json(&params) {
        Ok(request) => request,
        Err(err_response) => return Ok(err_response),
    };

    let mut rooms = ROOMS.lock().await;

    if !rooms.contains_key(&request.id) {
        return Ok(http::create_response(
            Body::from(BodyUtil::ROOM_ID_NOTFOUND),
            StatusCode::NOT_ACCEPTABLE,
        ));
    }

    let room: &mut Room = rooms.get_mut(&request.id).unwrap();
    if !room.auth_shared_key(request.shared_key.clone()) {
        return Ok(http::create_response(
            Body::from(BodyUtil::INVILED_PASSWORD),
            StatusCode::NOT_ACCEPTABLE,
        ));
    }

    let mut response = ResponseJson { infos: Vec::new() };

    response.infos.append(&mut vec![room.info()]);

    return Ok(http::create_response(
        Body::from(serde_json::to_string(&response).unwrap()),
        StatusCode::OK,
    ));
}
