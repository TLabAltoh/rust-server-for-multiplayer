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
    Router::new().route("/room/join/:base64/", post(room_join))
}

#[derive(Serialize, Deserialize)]
struct RequestJson {
    name: String,
    id: i32,
    shared_key: String,
    master_key: String,
}

#[derive(Serialize, Deserialize)]
struct ResponseJson {
    id: i32,
    token: u32,
}

async fn room_join(Path(params): Path<HashMap<String, String>>) -> Result<Response> {
    debug!("HTTP GET /room/join");

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

    let mut user_id = i32::default();
    let mut token = u32::default();
    if !room
        .join(
            request.name.clone(),
            request.master_key.clone(),
            &mut user_id,
            &mut token,
        )
        .await?
    {
        return Ok(http::create_response(
            Body::from(BodyUtil::REJECTED),
            StatusCode::NOT_ACCEPTABLE,
        ));
    }

    let response = ResponseJson {
        id: user_id.clone(),
        token: token.clone(),
    };
    let body = serde_json::to_string(&response).unwrap().to_string();

    return Ok(http::create_response(Body::from(body), StatusCode::OK));
}
