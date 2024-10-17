use std::collections::HashMap;

use axum::body::Body;
use axum::response::Response;
use base64::prelude::BASE64_STANDARD;
use base64::Engine;
use http::StatusCode;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::config::Config;
use crate::forward::rtc::client::Client;
use crate::http::BodyUtil;
use crate::room::Room;
use crate::{http, ROOMS};

pub mod room;
pub mod rtc;
pub mod r#static;
pub mod ws;

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
}

pub fn parse_base64_into_json<T>(params: &HashMap<String, String>) -> Result<T, Response>
where
    T: DeserializeOwned + Serialize,
{
    if !params.contains_key("json_base64") {
        let mut body = String::default();
        body.push_str("Missing JSON");

        return Err(http::create_response(
            Body::from(body),
            StatusCode::NOT_ACCEPTABLE,
        ));
    }
    let json_obj = match BASE64_STANDARD.decode(params.get("json_base64").unwrap()) {
        Ok(json_obj) => json_obj,
        Err(err) => {
            return Err(http::create_response(
                Body::from(err.to_string()),
                StatusCode::NOT_ACCEPTABLE,
            ))
        }
    };
    let json: T = match serde_json::from_slice(&json_obj) {
        Ok(json) => json,
        Err(err) => {
            return Err(http::create_response(
                Body::from(err.to_string()),
                StatusCode::NOT_ACCEPTABLE,
            ))
        }
    };
    Ok(json)
}

pub async fn auth_user(
    room_id: i32,
    room_pass: String,
    user_id: i32,
    user_token: u32,
) -> Result<(Room, Client), Response> {
    let mut rooms = ROOMS.lock().await;

    if !rooms.contains_key(&room_id) {
        return Err(http::create_response(
            Body::from(BodyUtil::ROOM_ID_NOTFOUND),
            StatusCode::NOT_ACCEPTABLE,
        ));
    }

    let room: &mut Room = rooms.get_mut(&room_id).unwrap();
    if !room.check_password(room_pass.clone()) {
        return Err(http::create_response(
            Body::from(BodyUtil::INVILED_PASSWORD),
            StatusCode::NOT_ACCEPTABLE,
        ));
    }

    let client_map = room.client_map();
    let clients = client_map.read().await;
    let client = clients.get(&user_id).cloned();
    drop(clients);

    if let Some(client) = client {
        if !client.check_token(user_token.clone()) {
            return Err(http::create_response(
                Body::from(BodyUtil::INVILED_TOKEN),
                StatusCode::NOT_ACCEPTABLE,
            ));
        }

        let room = rooms.get(&room_id).cloned().unwrap();
        return Ok((room, client));
    } else {
        return Err(http::create_response(
            Body::from(BodyUtil::UNKNOWN_ERROR),
            StatusCode::NOT_ACCEPTABLE,
        ));
    }
}
