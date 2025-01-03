use axum::extract::ws;
use axum::extract::ws::{WebSocket, WebSocketUpgrade};
use axum::extract::Path;
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use tokio::sync::mpsc;
use webrtc::ice_transport::ice_candidate::{RTCIceCandidate, RTCIceCandidateInit};
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;

use tracing::{debug, error};

use crate::result::Result;
use crate::room::Room;
use crate::route::*;
use crate::ROOMS;

pub fn route() -> Router<AppState> {
    Router::new().route("/stream/whip/:base64/", get(whip))
}

#[derive(Serialize, Deserialize)]
struct RequestJson {
    room_id: i32,
    user_id: i32,
    token: u32,
    stream: String,
    offer: String,
    shared_key: String,
}

#[derive(Serialize, Deserialize)]
struct SignalingJson {
    is_candidate: bool,
    sdp: String,
    session: String,
    candidate: String,
}

async fn whip(
    Path(params): Path<HashMap<String, String>>,
    ws: WebSocketUpgrade,
) -> Result<Response> {
    debug!("HTTP GET /stream/whip");

    let request: RequestJson = match parse_base64_into_json(&params) {
        Ok(request) => request,
        Err(err_response) => return Ok(err_response),
    };

    let (_room, client) = match auth_user(
        request.room_id.clone(),
        request.shared_key.clone(),
        request.user_id,
        request.token,
    )
    .await
    {
        Ok((room, client)) => (room, client),
        Err(err_response) => return Ok(err_response),
    };

    return Ok(ws.on_upgrade(|mut socket: WebSocket| {
        let request = request;
        Box::pin(async move {
            let stream = request.stream;
            let id = request.user_id as u32;
            let offer = RTCSessionDescription::offer(request.offer).unwrap();

            let mut rooms = ROOMS.lock().await;
            if !rooms.contains_key(&request.room_id) {
                error!("room does not exist");
            }

            let room: &mut Room = rooms.get_mut(&request.room_id).unwrap();
            let forwarder = room.forwarder();
            let forwarder = forwarder.write().await;

            drop(rooms);

            let (tx0, mut rx0) = mpsc::channel::<(bool, String)>(32);
            let tx1 = tx0.clone();
            let (peer, answer, session) = forwarder
                .publish(
                    stream.clone(),
                    id.clone(),
                    offer.clone(),
                    Box::new(move |candidate: Option<RTCIceCandidate>| {
                        let candidate = candidate.clone();
                        let tx0 = tx0.clone();
                        if let Some(candidate) = candidate {
                            return Box::pin(async move {
                                let c = candidate.to_json().unwrap().candidate;
                                if let Err(_err) = tx0.clone().send((false, c.clone())).await {}
                            });
                        }
                        Box::pin(async {})
                    }),
                    Box::new(move || {
                        let tx1 = tx1.clone();
                        Box::pin(async move {
                            if let Err(_err) = tx1.clone().send((true, "".to_string())).await {}
                        })
                    }),
                )
                .await
                .unwrap();
            drop(forwarder);

            let mut client = client;
            let _ = client.add_stream(stream.clone()).await;

            let answer = SignalingJson {
                is_candidate: false,
                sdp: answer.sdp,
                session: session,
                candidate: String::new(),
            };

            if socket
                .send(ws::Message::Text(serde_json::to_string(&answer).unwrap()))
                .await
                .is_err()
            {
                return;
            };

            let (mut sender, mut receiver) = socket.split();

            let mut send_task = tokio::spawn(async move {
                while let Some((peer_connected, candidate)) = rx0.recv().await {
                    if peer_connected {
                        return;
                    }
                    let signaling = SignalingJson {
                        is_candidate: true,
                        sdp: String::new(),
                        session: String::new(),
                        candidate: candidate,
                    };
                    let msg = ws::Message::Text(serde_json::to_string(&signaling).unwrap());
                    if sender.send(msg).await.is_err() {
                        return;
                    }
                }
            });

            let mut recv_task = tokio::spawn(async move {
                while let Some(msg) = receiver.next().await {
                    let msg = if let Ok(msg) = msg {
                        msg
                    } else {
                        return;
                    };

                    match msg {
                        ws::Message::Text(t) => {
                            if peer.connection_state() == RTCPeerConnectionState::Connected {
                                return;
                            }

                            let message = t.clone();

                            debug!("signaling message received: {}", message.clone());

                            let signaling: SignalingJson =
                                serde_json::from_str(&message.as_str()).unwrap();

                            let _ = peer
                                .add_ice_candidate(RTCIceCandidateInit {
                                    candidate: signaling.candidate.clone(),
                                    ..Default::default()
                                })
                                .await;
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
}
