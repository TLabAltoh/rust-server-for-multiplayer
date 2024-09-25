use axum::body::Body;
use axum::extract::ws;
use axum::extract::ws::{WebSocket, WebSocketUpgrade};
use axum::extract::Path;
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use base64::prelude::*;
use futures_util::StreamExt;
use http::BodyUtil;
use http::StatusCode;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use tokio::sync::mpsc;

use tracing::{debug, error};

use crate::http;
use crate::result::Result;
use crate::room::Room;
use crate::route::AppState;
use crate::ROOMS;

pub fn route() -> Router<AppState> {
    Router::new().route("/channel/channel/:json_base64/", get(channel))
}

#[derive(Serialize, Deserialize)]
struct JSON {
    room_id: i32,
    room_pass: String,
    user_id: i32,
    user_token: u32,
    channel: String,
}

async fn channel(
    Path(params): Path<HashMap<String, String>>,
    ws: WebSocketUpgrade,
) -> Result<Response> {
    debug!("HTTP GET /channel/channel");

    if !params.contains_key("json_base64") {
        return Ok(http::create_response(
            "Missing JSON".to_string().into(),
            StatusCode::NOT_ACCEPTABLE,
        ));
    }

    let json = BASE64_STANDARD.decode(params.get("json_base64").unwrap())?;
    let json: JSON = serde_json::from_slice(&json)?;

    let mut rooms = ROOMS.lock().await;

    if !rooms.contains_key(&json.room_id) {
        return Ok(http::create_response(
            Body::from(BodyUtil::ROOM_ID_NOTFOUND),
            StatusCode::NOT_ACCEPTABLE,
        ));
    }

    let room: &mut Room = rooms.get_mut(&json.room_id).unwrap();
    if !room.check_password(json.room_pass.clone()) {
        return Ok(http::create_response(
            Body::from(BodyUtil::INVILED_PASSWORD),
            StatusCode::NOT_ACCEPTABLE,
        ));
    }

    let client_map = room.client_map();
    let clients = client_map.read().await;
    let client = clients.get(&json.user_id).cloned();
    drop(clients);

    if let Some(client) = client {
        if !client.check_token(json.user_token.clone()) {
            return Ok(http::create_response(
                Body::from(BodyUtil::INVILED_TOKEN),
                StatusCode::NOT_ACCEPTABLE,
            ));
        }

        return Ok(ws.on_upgrade(|mut socket: WebSocket| {
            let json = json;
            Box::pin(async move {
                let channel = json.channel;
                let id = json.user_id as u32;

                let mut rooms = ROOMS.lock().await;
                if !rooms.contains_key(&json.room_id) {
                    error!("room does not exist");
                }

                let room: &mut Room = rooms.get_mut(&json.room_id).unwrap();

                let (tx0, mut rx) = mpsc::channel::<String>(32);
                let (mut sender, mut receiver) = socket.split();

                let mut send_task =
                    tokio::spawn(async move { while let Some(candidate) = rx.recv().await {} });

                let mut recv_task = tokio::spawn(async move {
                    while let Some(msg) = receiver.next().await {
                        let msg = if let Ok(msg) = msg {
                            msg
                        } else {
                            return;
                        };

                        match msg {
                            ws::Message::Text(t) => {
                                let message = t.clone();
                                debug!("signaling message received: {}", message.clone());
                            }
                            ws::Message::Binary(_b) => {}
                            ws::Message::Ping(_vec) => {}
                            ws::Message::Pong(_vec) => {}
                            ws::Message::Close(_close_frame) => {}
                        }
                    }
                });
                tokio::select! {
                    _rv_a = (&mut send_task) => {
                        recv_task.abort();
                    },
                    _rv_b = (&mut recv_task) => {
                        send_task.abort();
                    }
                }
            })
        }));
    } else {
        return Ok(http::create_response(
            Body::from(BodyUtil::UNKNOWN_ERROR),
            StatusCode::NOT_ACCEPTABLE,
        ));
    }
}
