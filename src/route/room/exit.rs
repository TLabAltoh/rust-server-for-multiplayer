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
    Router::new().route("/room/exit/:base64/", post(room_exit))
}

#[derive(Serialize, Deserialize)]
struct RequestJson {
    room_id: i32,
    user_id: i32,
    token: u32,
    shared_key: String,
}

async fn room_exit(Path(params): Path<HashMap<String, String>>) -> Result<Response> {
    debug!("HTTP GET /room/exit");

    let request: RequestJson = match parse_base64_into_json(&params) {
        Ok(request) => request,
        Err(err_response) => return Ok(err_response),
    };

    let mut rooms = ROOMS.lock().await;

    if !rooms.contains_key(&request.room_id) {
        return Ok(http::create_response(
            Body::from(BodyUtil::ROOM_ID_NOTFOUND),
            StatusCode::NOT_ACCEPTABLE,
        ));
    }

    let room: &mut Room = rooms.get_mut(&request.room_id).unwrap();
    if !room.auth_shared_key(request.shared_key.clone()) {
        return Ok(http::create_response(
            Body::from(BodyUtil::INVILED_PASSWORD),
            StatusCode::NOT_ACCEPTABLE,
        ));
    }

    if room
        .user_delete(request.user_id.clone(), request.token.clone(), true)
        .await?
    {
        return Ok(http::create_response(
            Body::from(BodyUtil::SUCCEED),
            StatusCode::OK,
        ));
    } else {
        return Ok(http::create_response(
            Body::from(BodyUtil::REJECTED),
            StatusCode::NOT_ACCEPTABLE,
        ));
    }
}
