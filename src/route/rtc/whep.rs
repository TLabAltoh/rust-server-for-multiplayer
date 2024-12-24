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
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use webrtc::ice_transport::ice_candidate::{RTCIceCandidate, RTCIceCandidateInit};
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;

use tracing::{debug, error};

use crate::result::Result;
use crate::room::Room;
use crate::route::*;
use crate::ROOMS;

pub fn route() -> Router<AppState> {
    Router::new().route("/stream/whep/:base64/", get(whep))
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

async fn whep(
    Path(params): Path<HashMap<String, String>>,
    ws: WebSocketUpgrade,
) -> Result<Response> {
    debug!("HTTP GET /stream/whep");

    let request: RequestJson = match parse_base64_into_json(&params) {
        Ok(request) => request,
        Err(err_response) => return Ok(err_response),
    };

    let (room, _client) = match auth_user(
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

    let forwarder = room.forwarder();
    let forwarder = forwarder.write().await;
    if !forwarder.is_stream_exists(request.stream.clone()).await? {
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
                request.stream.clone(),
                Box::new(move |candidate: Option<RTCIceCandidate>| {
                    let candidate = candidate.clone();
                    let tx0 = tx0.clone();
                    if let Some(candidate) = candidate {
                        return Box::pin(async move {
                            let c = candidate.to_json().unwrap().candidate;
                            if let Err(_err) = tx0.clone().send((0, c.clone())).await {}
                        });
                    }
                    Box::pin(async {})
                }),
            )
            .await?;
        let id = request.user_id as u32;
        let (peer1, answer, _session) = forwarder
            .publish(
                request.stream.clone(),
                id,
                sdp,
                Box::new(move |candidate: Option<RTCIceCandidate>| {
                    let candidate = candidate.clone();
                    let tx1 = tx1.clone();
                    if let Some(candidate) = candidate {
                        return Box::pin(async move {
                            let c = candidate.to_json().unwrap().candidate;
                            if let Err(_err) = tx1.clone().send((1, c.clone())).await {}
                        });
                    }
                    Box::pin(async {})
                }),
                Box::new(move || Box::pin(async {})),
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

            while !forwarder.publish_is_ok(stream.clone()).await.unwrap() {}

            let (tx0, mut rx0) = mpsc::channel::<(bool, String)>(32);
            let tx1 = tx0.clone();
            let (peer, answer, session) = forwarder
                .subscribe(
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
