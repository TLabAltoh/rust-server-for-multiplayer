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
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::RwLock;
use webrtc::ice_transport::ice_candidate::RTCIceCandidate;
use webrtc::ice_transport::ice_candidate::RTCIceCandidateInit;

use tracing::{debug, error};

use crate::http;
use crate::result::Result;
use crate::room::Room;
use crate::route::AppState;
use crate::ROOMS;

pub fn route() -> Router<AppState> {
    Router::new().route("/stream/vhost/:json_base64/", post(vhost))
}

#[derive(Serialize, Deserialize)]
struct JSON {
    room_id: i32,
    room_pass: String,
    user_id: i32,
    user_token: u32,
    stream: String,
}

async fn vhost(Path(params): Path<HashMap<String, String>>) -> Result<Response> {
    debug!("HTTP GET /stream/vhost");

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
            let client_map = room.client_map();
            let clients = client_map.read().await;
            let client = clients.get(&json.user_id).cloned();
            drop(clients);

            if let Some(client) = client {
                if client.check_token(json.user_token.clone()) {
                    let forwarder = room.forwarder();
                    let forwarder = forwarder.write().await;
                    if !forwarder.is_stream_exists(json.stream.clone()).await? {
                        let (tx0, mut rx) = mpsc::channel::<(u8, String)>(32);
                        let tx1 = tx0.clone();
                        let caches0: Arc<RwLock<Vec<String>>> = Default::default();
                        let caches1: Arc<RwLock<Vec<String>>> = Default::default();
                        let caches0c = Arc::clone(&caches0);
                        let caches1c = Arc::clone(&caches1);
                        let handle = tokio::spawn(async move {
                            let mut c1 = caches1c.write().await;
                            let mut c0 = caches0c.write().await;
                            while let Some(message) = rx.recv().await {
                                match message.0 {
                                    0 => {
                                        c1.push(message.1);
                                    }
                                    1 => {
                                        c0.push(message.1);
                                    }
                                    _ => todo!(),
                                };

                                if c0.len() > 0 && c1.len() > 0 {
                                    break;
                                }
                            }
                        });
                        let (peer0, sdp, _session) = forwarder
                            .virtual_publish(
                                json.stream.clone(),
                                Box::new(move |candidate: Option<RTCIceCandidate>| {
                                    let candidate = candidate.clone();
                                    let tx0 = tx0.clone();
                                    if let Some(candidate) = candidate {
                                        return Box::pin(async move {
                                            let c = candidate.to_json().unwrap().candidate;
                                            if let Err(_err) =
                                                tx0.clone().send((0, c.clone())).await
                                            {
                                                // Maybe channel is closed ...
                                                // error!("{}", _err);
                                            }
                                        });
                                    }
                                    Box::pin(async {})
                                }),
                            )
                            .await?;
                        let id = json.user_id as u32;
                        let (peer1, answer, _session) = forwarder
                            .publish(
                                json.stream.clone(),
                                id,
                                sdp,
                                Box::new(move |candidate: Option<RTCIceCandidate>| {
                                    let candidate = candidate.clone();
                                    let tx1 = tx1.clone();
                                    if let Some(candidate) = candidate {
                                        return Box::pin(async move {
                                            let c = candidate.to_json().unwrap().candidate;
                                            if let Err(_err) =
                                                tx1.clone().send((1, c.clone())).await
                                            {
                                                // Maybe channel is closed ...
                                                // error!("{}", _err);
                                            }
                                        });
                                    }
                                    Box::pin(async {})
                                }),
                            )
                            .await?;
                        peer0.set_remote_description(answer).await?;

                        while !handle.is_finished() {}

                        let caches0 = caches0.read().await;
                        let caches0 = caches0.iter();
                        for candidate in caches0 {
                            debug!("[vhost] peer0 add ice-candidate: {}", candidate);
                            if let Err(err) = peer0
                                .add_ice_candidate(RTCIceCandidateInit {
                                    candidate: candidate.to_string(),
                                    ..Default::default()
                                })
                                .await
                            {
                                error!("{}", err);
                            }
                        }

                        let caches1 = caches1.read().await;
                        let caches1 = caches1.iter();
                        for candidate in caches1 {
                            debug!("[vhost] peer1 add ice-candidate: {}", candidate);
                            if let Err(err) = peer1
                                .add_ice_candidate(RTCIceCandidateInit {
                                    candidate: candidate.to_string(),
                                    ..Default::default()
                                })
                                .await
                            {
                                error!("{}", err);
                            }
                        }
                    }
                    return Ok(http::create_response(
                        "Success to make virtual host".into(),
                        StatusCode::OK,
                    ));
                } else {
                    return Ok(http::create_response(
                        Body::from(BodyUtil::INVILED_TOKEN),
                        StatusCode::NOT_ACCEPTABLE,
                    ));
                }
            }
            return Ok(http::create_response(
                Body::from(BodyUtil::UNKNOWN_ERROR),
                StatusCode::NOT_ACCEPTABLE,
            ));
        } else {
            return Ok(http::create_response(
                Body::from(BodyUtil::INVILED_PASSWORD),
                StatusCode::NOT_ACCEPTABLE,
            ));
        }
    } else {
        return Ok(http::create_response(
            Body::from(BodyUtil::ROOM_ID_NOTFOUND),
            StatusCode::NOT_ACCEPTABLE,
        ));
    }
}
