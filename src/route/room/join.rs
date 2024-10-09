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

pub fn route() -> Router<AppState> {
    Router::new().route("/room/join/:json_base64/", post(room_join))
}

#[derive(Serialize, Deserialize)]
struct JSON {
    user_name: String,
    room_id: i32,
    room_pass: String,
    master_key: String,
}

#[derive(Serialize, Deserialize)]
struct RESPONSE {
    user_id: i32,
    user_token: u32,
}

async fn room_join(Path(params): Path<HashMap<String, String>>) -> Result<Response> {
    debug!("HTTP GET /room/join");

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
        if room.check_password(json.room_pass.clone()) {
            let mut user_id = i32::default();
            let mut user_token = u32::default();
            if room
                .join(
                    json.user_name.clone(),
                    json.master_key.clone(),
                    &mut user_id,
                    &mut user_token,
                )
                .await?
            {
                let group_manager = room.group_manager();
                let group_manager = group_manager.write().await;
                group_manager.init_user(user_id.to_string(), None).await;
                drop(group_manager);

                let json = RESPONSE {
                    user_id: user_id.clone(),
                    user_token: user_token.clone(),
                };
                let body = serde_json::to_string(&json).unwrap().to_string();

                return Ok(http::create_response(Body::from(body), StatusCode::OK));
            } else {
                return Ok(http::create_response(
                    Body::from(BodyUtil::REJECTED),
                    StatusCode::NOT_ACCEPTABLE,
                ));
            }
        }

        return Ok(http::create_response(
            Body::from(BodyUtil::INVILED_PASSWORD),
            StatusCode::NOT_ACCEPTABLE,
        ));
    }

    return Ok(http::create_response(
        Body::from(BodyUtil::ROOM_ID_NOTFOUND),
        StatusCode::NOT_ACCEPTABLE,
    ));
}
