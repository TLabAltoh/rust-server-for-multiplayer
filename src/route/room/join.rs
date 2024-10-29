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
use crate::route::*;
use crate::ROOMS;

pub fn route() -> Router<AppState> {
    Router::new().route("/room/join/:json_base64/", post(room_join))
}

#[derive(Serialize, Deserialize)]
struct JSON {
    user_name: String,
    room_id: i32,
    room_key: String,
    master_key: String,
}

#[derive(Serialize, Deserialize)]
struct RESPONSE {
    user_id: i32,
    user_token: u32,
}

async fn room_join(Path(params): Path<HashMap<String, String>>) -> Result<Response> {
    debug!("HTTP GET /room/join");

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
    if !room.auth_room_key(json.room_key.clone()) {
        return Ok(http::create_response(
            Body::from(BodyUtil::INVILED_PASSWORD),
            StatusCode::NOT_ACCEPTABLE,
        ));
    }

    let mut user_id = i32::default();
    let mut user_token = u32::default();
    if !room
        .join(
            json.user_name.clone(),
            json.master_key.clone(),
            &mut user_id,
            &mut user_token,
        )
        .await?
    {
        return Ok(http::create_response(
            Body::from(BodyUtil::REJECTED),
            StatusCode::NOT_ACCEPTABLE,
        ));
    }

    let json = RESPONSE {
        user_id: user_id.clone(),
        user_token: user_token.clone(),
    };
    let body = serde_json::to_string(&json).unwrap().to_string();

    return Ok(http::create_response(Body::from(body), StatusCode::OK));
}
