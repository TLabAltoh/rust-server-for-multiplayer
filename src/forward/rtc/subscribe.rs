use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, info};
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::rtp_transceiver::rtp_codec::RTPCodecType;
use webrtc::rtp_transceiver::rtp_sender::RTCRtpSender;
use webrtc::track::track_local::track_local_static_rtp::TrackLocalStaticRTP;
use webrtc::track::track_local::TrackLocalWriter;

use crate::forward::rtc::message::SessionInfo;
use crate::forward::rtc::rtcp::RtcpMessage;
use crate::forward::rtc::track::ForwardData;

use super::get_peer_id;
use super::track::PublishTrackRemote;

struct SubscribeForwardChannel {
    publish_rtcp_sender: broadcast::Sender<(RtcpMessage, u32)>,
    publish_track_change: broadcast::Receiver<()>,
}

pub(crate) struct SubscribeRTCPeerConnection {
    pub(crate) id: String,
    pub(crate) peer: Arc<RTCPeerConnection>,
    pub(crate) create_time: i64,
}

impl SubscribeRTCPeerConnection {
    pub(crate) async fn new(
        stream: String,
        peer: Arc<RTCPeerConnection>,
        publish_rtcp_sender: broadcast::Sender<(RtcpMessage, u32)>,
        (publish_tracks, publish_track_change): (
            Arc<RwLock<Vec<PublishTrackRemote>>>,
            broadcast::Sender<()>, // use subscribe
        ),
        (video_sender, audio_sender): (Option<Arc<RTCRtpSender>>, Option<Arc<RTCRtpSender>>),
    ) -> Self {
        let id = get_peer_id(&peer);
        let track_binding_publish_rid = Arc::new(RwLock::new(HashMap::new()));
        for (sender, kind) in [
            (video_sender, RTPCodecType::Video),
            (audio_sender, RTPCodecType::Audio),
        ] {
            if sender.is_none() {
                continue;
            }
            let sender = sender.unwrap();
            tokio::spawn(Self::sender_forward_rtcp(
                kind,
                sender.clone(),
                publish_tracks.clone(),
                track_binding_publish_rid.clone(),
                publish_rtcp_sender.clone(),
            ));
            tokio::spawn(Self::sender_forward_rtp(
                stream.clone(),
                id.clone(),
                sender,
                kind,
                track_binding_publish_rid.clone(),
                publish_tracks.clone(),
                SubscribeForwardChannel {
                    publish_rtcp_sender: publish_rtcp_sender.clone(),
                    publish_track_change: publish_track_change.subscribe(),
                },
            ));
        }
        let _ = publish_track_change.send(());
        Self {
            id,
            peer,
            create_time: Utc::now().timestamp_millis(),
        }
    }

    pub(crate) async fn info(&self) -> SessionInfo {
        SessionInfo {
            id: self.id.clone(),
            create_time: self.create_time,
            connect_state: self.peer.connection_state(),
        }
    }

    async fn sender_forward_rtp(
        stream: String,
        id: String,
        sender: Arc<RTCRtpSender>,
        kind: RTPCodecType,
        track_binding_publish_rid: Arc<RwLock<HashMap<String, String>>>,
        publish_tracks: Arc<RwLock<Vec<PublishTrackRemote>>>,
        mut forward_channel: SubscribeForwardChannel,
    ) {
        info!("[{}] [{}] {} up", stream, id, kind);
        // empty broadcast channel
        let (virtual_sender, _) = broadcast::channel::<ForwardData>(1);
        let mut recv = virtual_sender.subscribe();
        let mut track = None;
        let mut sequence_number: u16 = 0;
        loop {
            tokio::select! {
                publish_change = forward_channel.publish_track_change.recv() =>{
                    debug!("{} {} recv publish track_change",stream,id);
                    if publish_change.is_err() {
                        continue;
                    }
                      let mut track_binding_publish_rid = track_binding_publish_rid.write().await;
                        let publish_tracks = publish_tracks.read().await;
                        let current_rid = track_binding_publish_rid.get(&kind.clone().to_string());
                        if publish_tracks.len() == 0 {
                            debug!("{} {} publish track len 0 , probably offline",stream,id);
                            recv = virtual_sender.subscribe();
                            let _ = sender.replace_track(None).await;
                            track = None;
                            if current_rid.is_some() {
                                track_binding_publish_rid.remove(&kind.clone().to_string());
                            };
                            continue;
                        }
                        if track.is_some(){
                            continue;
                        }
                        for publish_track in publish_tracks.iter() {
                              if publish_track.kind != kind {
                                continue;
                            }
                                    let new_track= Arc::new(
                                        TrackLocalStaticRTP::new(publish_track.track.clone().codec().capability,"webrtc".to_string(),format!("{}-{}","webrtc",kind))
                                    );
                                    match sender.replace_track(Some(new_track.clone())).await {
                                     Ok(_) => {
                                        debug!("[{}] [{}] {} track replace ok", stream, id,kind);
                                        recv = publish_track.subscribe();
                                        track = Some(new_track);
                                        let _ = forward_channel.publish_rtcp_sender.send((RtcpMessage::PictureLossIndication, publish_track.track.ssrc()));
                                        track_binding_publish_rid.insert(kind.clone().to_string(), publish_track.rid.clone());
                                    }
                                     Err(e) => {
                                        debug!("[{}] [{}] {} track replace err: {}", stream, id,kind, e);
                                    }};
                                     break;
                       }
                }
                rtp_result = recv.recv() => {
                    match rtp_result {
                        Ok(packet) => {
                            match track {
                                None => {
                                    continue;
                                }
                                Some(ref track) => {
                                    let mut packet = packet.as_ref().clone();
                                    packet.header.sequence_number = sequence_number;
                                    if let Err(err) = track.write_rtp(&packet).await {
                                        debug!("[{}] [{}] {} track write err: {}", stream, id,kind, err);
                                        break;
                                    }
                                    sequence_number = sequence_number.wrapping_add(1);
                                }
                            }
                        }
                        Err(err) => {
                            debug!("[{}] [{}] {} rtp receiver err: {}", stream, id, kind,err);
                        }
                    }
                }
            }
        }
        info!("[{}] [{}] {} down", stream, id, kind);
    }

    async fn sender_forward_rtcp(
        kind: RTPCodecType,
        sender: Arc<RTCRtpSender>,
        publish_tracks: Arc<RwLock<Vec<PublishTrackRemote>>>,
        track_binding_publish_rid: Arc<RwLock<HashMap<String, String>>>,
        publish_rtcp_sender: broadcast::Sender<(RtcpMessage, u32)>,
    ) {
        loop {
            match sender.read_rtcp().await {
                Ok((packets, _)) => {
                    let track_binding_publish_rid = track_binding_publish_rid.read().await;
                    let publish_rid = match track_binding_publish_rid.get(&kind.clone().to_string())
                    {
                        None => {
                            continue;
                        }
                        Some(rid) => rid,
                    };
                    for packet in packets {
                        if let Some(msg) = RtcpMessage::from_rtcp_packet(packet) {
                            let publish_tracks = publish_tracks.read().await;
                            for publish_track in publish_tracks.iter() {
                                if publish_track.kind == kind && &publish_track.rid == publish_rid {
                                    if let Err(_err) =
                                        publish_rtcp_sender.send((msg, publish_track.track.ssrc()))
                                    {
                                        return;
                                    }
                                }
                            }
                        }
                    }
                }
                Err(_err) => {
                    return;
                }
            }
        }
    }
}
