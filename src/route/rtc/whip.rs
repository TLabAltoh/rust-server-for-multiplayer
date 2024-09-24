use axum::body::Body;
use axum::extract::Path;
use axum::response::Response;
use axum::routing::post;
use axum::Router;
use base64::prelude::*;
use http::StatusCode;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use webrtc::ice_transport::ice_candidate::RTCIceCandidate;

use tracing::debug;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;

use crate::http::{self, BodyUtil};
use crate::result::Result;
use crate::room::Room;
use crate::route::AppState;
use crate::ROOMS;

pub fn route() -> Router<AppState> {
    Router::new().route("/stream/whip/:json_base64/", post(whip))
}

#[derive(Serialize, Deserialize)]
struct JSON {
    room_id: i32,
    room_pass: String,
    user_id: i32,
    user_token: u32,
    stream: String,
    offer: String,
}

async fn whip(Path(params): Path<HashMap<String, String>>) -> Result<Response> {
    debug!("HTTP GET /stream/whip");

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
                    let offer = RTCSessionDescription::offer(json.offer)?;

                    let forwarder = room.forwarder();
                    let forwarder = forwarder.write().await;
                    let id = json.user_id as u32;
                    let (_peer, answer, _session) = forwarder
                        .publish(
                            json.stream.clone(),
                            id,
                            offer,
                            Box::new(move |candidate: Option<RTCIceCandidate>| {
                                if let Some(c) = candidate {
                                    let c = c.to_string();
                                    debug!("peer ice-candidate: {}", c);
                                    // let _ = Box::pin(async {
                                    //     _peer1
                                    //         .add_ice_candidate(RTCIceCandidateInit {
                                    //             candidate: c,
                                    //             ..Default::default()
                                    //         })
                                    //         .await
                                    // });
                                }
                                Box::pin(async {})
                            }),
                        )
                        .await
                        .unwrap();
                    drop(forwarder);

                    let mut client = client;
                    client.add_stream(json.stream.clone()).await?;

                    return Ok(http::create_response(answer.sdp.into(), StatusCode::OK));
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
