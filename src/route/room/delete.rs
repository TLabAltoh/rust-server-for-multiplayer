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

#[derive(Serialize, Deserialize)]
struct RequestJson {
    id: i32,
    master_key: String,
}

pub fn route() -> Router<AppState> {
    Router::new().route("/room/delete/:base64/", post(delete_room))
}

async fn delete_room(Path(params): Path<HashMap<String, String>>) -> Result<Response> {
    debug!("HTTP GET /room/delete");

    let request: RequestJson = match parse_base64_into_json(&params) {
        Ok(request) => request,
        Err(err_response) => return Ok(err_response),
    };

    let mut rooms = ROOMS.lock().await;

    if !rooms.contains_key(&request.id) {
        return Ok(http::create_response(
            Body::from(BodyUtil::INVILED_PASSWORD),
            StatusCode::NOT_ACCEPTABLE,
        ));
    }

    let room: &mut Room = rooms.get_mut(&request.id).unwrap();
    if !room.auth_master_key(request.master_key.clone()) {
        return Ok(http::create_response(
            Body::from(BodyUtil::REJECTED),
            StatusCode::NOT_ACCEPTABLE,
        ));
    }

    room.all_user_delete().await?;
    rooms.remove(&request.id);

    return Ok(http::create_response(
        Body::from(BodyUtil::SUCCEED),
        StatusCode::OK,
    ));
}
