use axum::body::Body;
use axum::extract::Path;
use axum::response::Response;
use axum::routing::post;
use axum::Router;
use base64::prelude::*;
use http::BodyUtil;
use http::StatusCode;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use tracing::debug;

use crate::http;
use crate::result::Result;
use crate::room::Room;
use crate::route::AppState;
use crate::ROOMS;

#[derive(Serialize, Deserialize)]
struct JSON {
    room_id: i32,
    master_key: String,
}

pub fn route() -> Router<AppState> {
    Router::new().route("/room/delete/:json_base64/", post(delete_room))
}

async fn delete_room(Path(params): Path<HashMap<String, String>>) -> Result<Response> {
    debug!("HTTP GET /room/delete");

    // for (key, value) in &params {
    //     debug!("{}: {}", key, value);
    // }

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
        if room.check_master_key(json.master_key.clone()) {
            room.all_user_delete().await?;
            rooms.remove(&json.room_id);

            return Ok(http::create_response(
                Body::from(BodyUtil::SUCCEED),
                StatusCode::OK,
            ));
        }

        return Ok(http::create_response(
            Body::from(BodyUtil::REJECTED),
            StatusCode::NOT_ACCEPTABLE,
        ));
    }

    return Ok(http::create_response(
        Body::from(BodyUtil::INVILED_PASSWORD),
        StatusCode::NOT_ACCEPTABLE,
    ));
}
