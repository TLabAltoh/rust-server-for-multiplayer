use axum::body::Body;
use axum::extract::{Path, State};
use axum::response::Response;
use axum::routing::post;
use axum::Router;
use http::StatusCode;
use log::debug;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;

use crate::http;
use crate::result::Result;
use crate::room::Room;
use crate::route::*;
use crate::ROOMS;

pub fn route() -> Router<AppState> {
    Router::new().route("/room/create/:json_base64/", post(create_room))
}

#[derive(Serialize, Deserialize)]
struct JSON {
    room_name: String,
    room_capacity: u32,
    room_key: String,
    needs_host: bool,
    is_public: bool,
    master_key: String,
    description: String,
}

async fn create_room(
    State(state): State<AppState>,
    Path(params): Path<HashMap<String, String>>,
) -> Result<Response> {
    debug!("HTTP GET /room/create");

    let json: JSON = match parse_base64_into_json(&params) {
        Ok(json) => json,
        Err(err_response) => return Ok(err_response),
    };

    let mut rooms = ROOMS.lock().await;

    let room_id = utils::unique::generate_unique_i32();

    let room = Room::new(
        room_id,
        json.room_name.to_string(),
        json.needs_host,
        json.is_public,
        json.room_capacity,
        json.room_key,
        json.master_key,
        json.description,
        state.config,
    );

    let body = serde_json::to_string(&room.info()).unwrap().to_string();
    rooms.insert(room_id, room);

    return Ok(http::create_response(Body::from(body), StatusCode::OK));
}
