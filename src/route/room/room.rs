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
        .merge(Router::new().route("/room/:json_base64/", post(room_specific)))
}

#[derive(Serialize, Deserialize)]
struct JSON {
    room_id: i32,
    room_pass: String,
}

#[derive(Serialize, Deserialize)]
struct RESPONSE {
    room_infos: Vec<RoomInfoJson>,
}

async fn room() -> Result<Response> {
    debug!("HTTP GET /room");

    let rooms = ROOMS.lock().await;

    let mut json = RESPONSE {
        room_infos: Vec::new(),
    };

    for (_room_id, room) in rooms.iter() {
        if room.is_public() {
            json.room_infos.append(&mut vec![room.info()]);
        }
    }

    let mut body = String::default();
    body.push_str(&serde_json::to_string(&json).unwrap());

    return Ok(http::create_response(Body::from(body), StatusCode::OK));
}

async fn room_specific(Path(params): Path<HashMap<String, String>>) -> Result<Response> {
    println!("HTTP GET /room");

    let json: JSON = match parse_base64_into_json(&params) {
        Ok(json) => json,
        Err(err_response) => return Ok(err_response),
    };

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

    let mut json = RESPONSE {
        room_infos: Vec::new(),
    };

    json.room_infos.append(&mut vec![room.info()]);

    return Ok(http::create_response(
        Body::from(serde_json::to_string(&json).unwrap()),
        StatusCode::OK,
    ));
}
