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
    Router::new().route("/room/create/:base64/", post(create_room))
}

#[derive(Serialize, Deserialize)]
struct RequestJson {
    name: String,
    capacity: u32,
    needs_host: bool,
    is_public: bool,
    shared_key: String,
    master_key: String,
    description: String,
}

async fn create_room(
    State(state): State<AppState>,
    Path(params): Path<HashMap<String, String>>,
) -> Result<Response> {
    debug!("HTTP GET /room/create");

    let request: RequestJson = match parse_base64_into_json(&params) {
        Ok(request) => request,
        Err(err_response) => return Ok(err_response),
    };

    let mut rooms = ROOMS.lock().await;

    let room_id = utils::unique::generate_unique_i32();

    let room = Room::new(
        room_id,
        request.name.to_string(),
        request.needs_host,
        request.is_public,
        request.capacity,
        request.shared_key,
        request.master_key,
        request.description,
        state.config,
    );

    let body = serde_json::to_string(&room.info()).unwrap().to_string();

    rooms.insert(room_id, room);

    return Ok(http::create_response(Body::from(body), StatusCode::OK));
}
