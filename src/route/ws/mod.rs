use axum::body::Body;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::Path;
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use base64::prelude::*;
use futures_util::{SinkExt, StreamExt};
use http::BodyUtil;
use http::StatusCode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, error};

use crate::http;
use crate::result::Result;
use crate::room::Room;
use crate::route::AppState;
use crate::ROOMS;

pub fn route() -> Router<AppState> {
    Router::new().route("/ws/connect/:json_base64/", get(channel))
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
    debug!("HTTP GET /ws/connect");

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
        return Ok(ws.on_upgrade(|socket: WebSocket| {
            let json = json;
            Box::pin(async move {
                let channel = json.channel;
                let id = json.user_id as u32;

                let mut rooms = ROOMS.lock().await;
                if !rooms.contains_key(&json.room_id) {
                    error!("room does not exist");
                }

                let room: &mut Room = rooms.get_mut(&json.room_id).unwrap();

                let (mut socekt_sender, mut socket_receiver) = socket.split();
                let group_manager = room.group_manager();
                let group_manager = group_manager.write().await;

                drop(rooms);

                group_manager.init_user(id.to_string(), None).await;

                let user_sender = group_manager
                    .join_or_create(id.to_string(), channel.clone())
                    .await
                    .unwrap();

                let user_receiver = group_manager.get_user_receiver(id.to_string()).await;
                let mut user_receiver = match user_receiver {
                    Ok(user_receiver) => user_receiver,
                    Err(error) => {
                        socekt_sender
                            .send(Message::Text(error.to_string()))
                            .await
                            .unwrap();
                        socekt_sender.close().await.unwrap();
                        return;
                    }
                };

                drop(group_manager);

                debug!("[ws] start receive/send loop ...");

                let mut send_task = tokio::spawn(async move {
                    let id = id;
                    while let Ok(message) = user_receiver.recv().await {
                        let mut msg_id: u32 = 0;
                        for i in 0..4 {
                            msg_id += (message[i] as u32) << ((3 - i) * 8);
                        }
                        if msg_id == id {
                            continue; // This message was sent from own
                        }
                        match message[4] {
                            0 => {
                                // binary
                                socekt_sender
                                    .send(Message::Binary(message[5..].to_vec()))
                                    .await
                                    .unwrap();
                            }
                            1 => {
                                // text
                                socekt_sender
                                    .send(Message::Text(
                                        String::from_utf8(message[5..].to_vec()).unwrap(),
                                    ))
                                    .await
                                    .unwrap();
                            }
                            2.. => {}
                        };
                    }
                });

                let mut recv_task = tokio::spawn(async move {
                    let id = id;
                    let mut header = vec![0u8; 5]; // user_id (0 ~ 3) + message_type (4)
                    for i in 0..4 {
                        header[i] = (id >> ((3 - i) * 8)) as u8;
                    }
                    while let Some(Ok(message)) = socket_receiver.next().await {
                        match message {
                            Message::Binary(binary) => {
                                header[4] = 0;
                                user_sender.send([header.clone(), binary].concat()).unwrap();
                            }
                            Message::Text(text) => {
                                header[4] = 1;
                                user_sender
                                    .send([header.clone(), text.into_bytes()].concat())
                                    .unwrap();
                            }
                            Message::Ping(_vec) => {}
                            Message::Pong(_vec) => {}
                            Message::Close(_close_frame) => {}
                        }
                    }
                });

                tokio::select! {
                    _ = (&mut send_task) => recv_task.abort(),
                    _ = (&mut recv_task) => send_task.abort(),
                };

                let mut rooms = ROOMS.lock().await;
                let room: &mut Room = rooms.get_mut(&json.room_id).unwrap();
                let group_manager = room.group_manager();
                let group_manager = group_manager.write().await;
                let _ = group_manager
                    .leave_group(channel.clone(), id.to_string())
                    .await;
                drop(group_manager);

                println!("[ws] connection closed");
            })
        }));
    } else {
        return Ok(http::create_response(
            Body::from(BodyUtil::UNKNOWN_ERROR),
            StatusCode::NOT_ACCEPTABLE,
        ));
    }
}
