use axum::body::Body;
use axum::extract::{Path, State};
use axum::response::Response;
use axum::routing::post;
use axum::Router;
use base64::prelude::*;
use http::StatusCode;
use log::debug;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;

use crate::http;
use crate::result::Result;
use crate::room::Room;
use crate::route::AppState;
use crate::ROOMS;

pub fn route() -> Router<AppState> {
    Router::new().route("/room/create/:json_base64/", post(create_room))
}

#[derive(Serialize, Deserialize)]
struct JSON {
    room_name: String,
    room_capacity: u32,
    room_pass: String,
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

    if !params.contains_key("json_base64") {
        return Ok(http::create_response(
            Body::from("Missing JSON".to_string()),
            StatusCode::NOT_ACCEPTABLE,
        ));
    }

    let json_obj = BASE64_STANDARD.decode(params.get("json_base64").unwrap())?;
    let json: JSON = serde_json::from_slice(&json_obj)?;

    let mut rooms = ROOMS.lock().await;

    let room_id = utils::unique::generate_unique_i32();

    let room = Room::new(
        room_id,
        json.room_name.to_string(),
        json.needs_host,
        json.is_public,
        json.room_capacity,
        json.room_pass,
        json.master_key,
        json.description,
        state.config,
    );

    let mut body = String::default();
    body.push_str(&serde_json::to_string(&room.info()).unwrap());

    rooms.insert(room_id, room);

    return Ok(http::create_response(Body::from(body), StatusCode::OK));
}
