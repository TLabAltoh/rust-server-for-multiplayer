use std::borrow::ToOwned;
use std::collections::HashMap;
use std::sync::Arc;
use std::vec;

use crate::forward::rtc::message::ForwardInfo;
use crate::result::Result;
use chrono::Utc;

use tokio::sync::{broadcast, RwLock};
use tracing::{debug, info};
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::MediaEngine;
use webrtc::api::setting_engine::SettingEngine;
use webrtc::api::APIBuilder;
use webrtc::data::data_channel::DataChannel;
use webrtc::data_channel::RTCDataChannel;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::interceptor::registry::Registry;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::rtp_transceiver::rtp_codec::{RTCRtpHeaderExtensionCapability, RTPCodecType};
use webrtc::rtp_transceiver::rtp_sender::RTCRtpSender;
use webrtc::rtp_transceiver::rtp_transceiver_direction::RTCRtpTransceiverDirection;
use webrtc::rtp_transceiver::RTCRtpTransceiverInit;
use webrtc::sdp::extmap::{SDES_MID_URI, SDES_RTP_STREAM_ID_URI};

use webrtc::track::track_remote::TrackRemote;

use crate::error::AppError;

use super::get_peer_id;
use super::media::MediaInfo;
use super::message::{ForwardEvent, ForwardEventType};
use super::publish::PublishRTCPeerConnection;
use super::rtcp::RtcpMessage;
use super::subscribe::SubscribeRTCPeerConnection;
use super::track::PublishTrackRemote;

const MESSAGE_SIZE: usize = 1024 * 16;

#[derive(Clone)]
struct DataChannelForward {
    sender: broadcast::Sender<Vec<u8>>,
    _receiver: Arc<broadcast::Receiver<Vec<u8>>>,
}

type PublishRtcpChannel = (
    broadcast::Sender<(RtcpMessage, u32)>,
    broadcast::Receiver<(RtcpMessage, u32)>,
);

pub(crate) struct PeerForwardInternal {
    pub(crate) stream: String,
    create_time: i64,
    publish_leave_time: RwLock<i64>,
    subscribe_leave_time: RwLock<i64>,
    publish: RwLock<Option<PublishRTCPeerConnection>>,
    publish_tracks: Arc<RwLock<Vec<PublishTrackRemote>>>,
    publish_tracks_change: (broadcast::Sender<()>, broadcast::Receiver<()>),
    publish_rtcp_channel: PublishRtcpChannel,
    subscribe_group: RwLock<Vec<SubscribeRTCPeerConnection>>,
    user_sender_map: Arc<RwLock<HashMap<u32, broadcast::Sender<Vec<u8>>>>>,
    data_channel_forward: DataChannelForward,
    ice_server: Vec<RTCIceServer>,
    event_sender: broadcast::Sender<ForwardEvent>,
}

impl PeerForwardInternal {
    pub(crate) fn new(stream: impl ToString, ice_server: Vec<RTCIceServer>) -> Self {
        let publish_tracks_change = broadcast::channel(128);
        let data_channel_forward_channel = broadcast::channel(128);
        let data_channel_forward = DataChannelForward {
            sender: data_channel_forward_channel.0,
            _receiver: Arc::new(data_channel_forward_channel.1),
        };
        let (event_sender, mut recv) = broadcast::channel(16);
        tokio::spawn(async move { while recv.recv().await.is_ok() {} });
        PeerForwardInternal {
            stream: stream.to_string(),
            create_time: Utc::now().timestamp_millis(),
            publish_leave_time: RwLock::new(0),
            subscribe_leave_time: RwLock::new(Utc::now().timestamp_millis()),
            publish: RwLock::new(None),
            publish_tracks: Arc::new(RwLock::new(Vec::new())),
            publish_tracks_change,
            publish_rtcp_channel: broadcast::channel(48),
            subscribe_group: RwLock::new(Vec::new()),
            user_sender_map: Arc::new(RwLock::new(HashMap::new())),
            data_channel_forward,
            ice_server,
            event_sender,
        }
    }

    pub(crate) async fn info(&self) -> ForwardInfo {
        let mut subscribe_session_infos = vec![];
        let subscribe_group = self.subscribe_group.read().await;
        for subscribe in subscribe_group.iter() {
            subscribe_session_infos.push(subscribe.info().await);
        }
        ForwardInfo {
            id: self.stream.clone(),
            create_time: self.create_time,
            publish_leave_time: *self.publish_leave_time.read().await,
            subscribe_leave_time: *self.subscribe_leave_time.read().await,
            publish_session_info: self
                .publish
                .read()
                .await
                .as_ref()
                .map(|publish| publish.info()),
            subscribe_session_infos,
        }
    }

    // This function has not used currently, but seems worth to keep retain
    // pub(crate) async fn remove_peer(&self, id: String) -> Result<bool> {
    //     let publish = self.publish.read().await;
    //     if publish.is_some() && publish.as_ref().unwrap().id == id {
    //         publish.as_ref().unwrap().peer.close().await?;
    //         return Ok(true);
    //     }

    //     let subscribe_group = self.subscribe_group.read().await;
    //     for subscribe in subscribe_group.iter() {
    //         if subscribe.id == id {
    //             subscribe.peer.close().await?;
    //             break;
    //         }
    //     }
    //     Ok(false)
    // }

    pub(crate) async fn close(&self) -> Result<()> {
        let publish = self.publish.read().await;
        let subscribe_group = self.subscribe_group.read().await;
        if publish.is_some() {
            publish.as_ref().unwrap().peer.close().await?;
        }
        for subscribe in subscribe_group.iter() {
            subscribe.peer.close().await?;
        }
        info!("{} close", self.stream);
        Ok(())
    }

    async fn data_channel_forward(
        id: u32,
        dc: Arc<RTCDataChannel>,
        group_sender: broadcast::Sender<Vec<u8>>,
        user_sender: broadcast::Sender<Vec<u8>>,
        user_sender_map: Arc<RwLock<HashMap<u32, broadcast::Sender<Vec<u8>>>>>,
    ) {
        let dc2 = dc.clone();
        dc.on_open(Box::new(move || {
            tokio::spawn(async move {
                let raw = match dc2.detach().await {
                    Ok(raw) => raw,
                    Err(err) => {
                        debug!("detach err: {}", err);
                        return;
                    }
                };
                let r = Arc::clone(&raw);
                tokio::spawn(Self::data_channel_read_loop(
                    id.clone(),
                    r,
                    group_sender.clone(),
                    user_sender_map,
                ));
                tokio::spawn(Self::data_channel_write_loop(
                    id.clone(),
                    raw,
                    user_sender.subscribe(),
                ));
                tokio::spawn(Self::data_channel_broadcast_pipe_loop(
                    id.clone(),
                    user_sender.clone(),
                    group_sender.subscribe(),
                ));
            });

            Box::pin(async {})
        }));
    }

    async fn data_channel_read_loop(
        id: u32,
        d: Arc<DataChannel>,
        group_sender: broadcast::Sender<Vec<u8>>,
        user_sender_map: Arc<RwLock<HashMap<u32, broadcast::Sender<Vec<u8>>>>>,
    ) {
        let mut buffer = vec![0u8; 9 + MESSAGE_SIZE]; // typ (1) + from (0 ~ 3) + to (4 ~ 7)

        buffer[0] = 1; // typ: open
        for i in 0..4 {
            buffer[i + 1] = (id >> (i * 8)) as u8;
        }
        for i in 0..4 {
            buffer[i + 5] = (id >> (i * 8)) as u8;
        }
        if let Err(err) = group_sender.send(buffer[..9].to_vec()) {
            info!("send data channel err: {}", err);
            return;
        }

        buffer[0] = 0; // typ: struct
        loop {
            let n = match d.read(&mut buffer[5..]).await {
                Ok(n) => n,
                Err(err) => {
                    info!("Datachannel closed; Exit the read_loop: {err}");
                    return;
                }
            };
            if n == 0 {
                break;
            }

            let is_broadcast = buffer[1..5] == buffer[5..9];
            if is_broadcast {
                //debug!("[rtc] send broadcast message");
                if let Err(err) = group_sender.send(buffer[..n + 5].to_vec()) {
                    info!("send data channel err: {}", err);
                    return;
                }
            } else {
                //debug!("[rtc] send unicast message");
                let to = u32::from_be_bytes([buffer[8], buffer[7], buffer[6], buffer[5]]);
                let user_sender_map = user_sender_map.read().await;
                if let Some(user_sender) = user_sender_map.get(&to) {
                    if let Err(err) = user_sender.send(buffer[..n + 4].to_vec()) {
                        info!("send data channel err: {}", err);
                        return;
                    };
                }
                drop(user_sender_map);
            }
        }
    }

    async fn data_channel_write_loop(
        _id: u32,
        d: Arc<DataChannel>,
        mut user_receiver: broadcast::Receiver<Vec<u8>>,
    ) {
        while let Ok(msg) = user_receiver.recv().await {
            if let Err(_err) = d.write(&msg.into()).await {
                // Maybe stream has been closed
                // info!("write data channel err: {}", _err);
                return;
            };
        }
    }

    async fn data_channel_broadcast_pipe_loop(
        id: u32,
        user_sender: broadcast::Sender<Vec<u8>>,
        mut group_receiver: broadcast::Receiver<Vec<u8>>,
    ) {
        while let Ok(msg) = group_receiver.recv().await {
            let from = u32::from_be_bytes([msg[4], msg[3], msg[2], msg[1]]);
            if from == id {
                continue; // This message was sent from own
            }
            if let Err(_err) = user_sender.send(msg) {
                // Maybe stream has been closed
                // info!("write data channel err: {}", _err);
                return;
            };
        }
    }
}

// publish
impl PeerForwardInternal {
    pub(crate) fn notice_network_event(&self, id: u32, open: bool) {
        let mut buffer = vec![0u8; 9]; // typ (1) + from (0 ~ 3) + to (4 ~ 7)

        buffer[0] = if open { 1 } else { 2 };
        for i in 0..4 {
            buffer[i + 1] = (id >> (i * 8)) as u8;
        }
        for i in 0..4 {
            buffer[i + 5] = (id >> (i * 8)) as u8;
        }
        let group_sender = self.data_channel_forward.sender.clone();
        if let Err(err) = group_sender.send(buffer[..9].to_vec()) {
            info!("send data channel err: {}", err);
            return;
        }
    }

    pub(crate) async fn publish_is_some(&self) -> bool {
        let publish = self.publish.read().await;
        publish.is_some()
    }

    pub(crate) async fn publish_is_ok(&self) -> bool {
        let publish = self.publish.read().await;
        publish.is_some()
            && publish.as_ref().unwrap().peer.connection_state()
                == RTCPeerConnectionState::Connected
    }

    pub(crate) async fn set_publish(&self, peer: Arc<RTCPeerConnection>) -> Result<()> {
        {
            let mut publish = self.publish.write().await;
            if publish.is_some() {
                return Err(AppError::stream_already_exists(
                    "A connection has already been established",
                ));
            }
            let publish_peer = PublishRTCPeerConnection::new(
                self.stream.clone(),
                peer.clone(),
                self.publish_rtcp_channel.0.subscribe(),
            )
            .await?;
            info!("[{}] [publish] set {}", self.stream, publish_peer.id);
            *publish = Some(publish_peer);
        }
        {
            let mut publish_leave_time = self.publish_leave_time.write().await;
            *publish_leave_time = 0;
        }
        self.send_event(ForwardEventType::PublishUp, get_peer_id(&peer))
            .await;
        Ok(())
    }

    pub(crate) async fn remove_publish(&self, id: u32, peer: Arc<RTCPeerConnection>) -> Result<()> {
        {
            let mut publish = self.publish.write().await;
            if publish.is_none() {
                return Err(AppError::throw("publish is none"));
            }
            if publish.as_ref().unwrap().id != get_peer_id(&peer) {
                return Err(AppError::throw("publish not myself"));
            }
            *publish = None;
        }
        {
            let mut publish_tracks = self.publish_tracks.write().await;
            publish_tracks.clear();
            let _ = self.publish_tracks_change.0.send(());
        }
        {
            let mut publish_leave_time = self.publish_leave_time.write().await;
            *publish_leave_time = Utc::now().timestamp_millis();
        }
        info!("[{}] [publish] set none", self.stream);

        let mut user_sender_map = self.user_sender_map.write().await;
        user_sender_map.remove(&id);
        drop(user_sender_map);

        self.send_event(ForwardEventType::PublishDown, get_peer_id(&peer))
            .await;
        Ok(())
    }

    pub(crate) async fn new_virtual_publish_peer(&self) -> Result<Arc<RTCPeerConnection>> {
        let mut m = MediaEngine::default();
        m.register_default_codecs()?;
        let mut s = SettingEngine::default();
        s.detach_data_channels();
        let mut registry = Registry::new();
        registry = register_default_interceptors(registry, &mut m)?;
        let mut s = SettingEngine::default();
        s.detach_data_channels();
        let api = APIBuilder::new()
            .with_media_engine(m)
            .with_interceptor_registry(registry)
            .with_setting_engine(s)
            .build();
        let config = RTCConfiguration {
            ice_servers: self.ice_server.clone(),
            ..Default::default()
        };
        let peer = Arc::new(api.new_peer_connection(config).await?);
        Ok(peer)
    }

    pub(crate) async fn new_publish_peer(
        &self,
        media_info: MediaInfo,
    ) -> Result<Arc<RTCPeerConnection>> {
        if media_info.video_transceiver.0 > 1 || media_info.audio_transceiver.0 > 1 {
            return Err(AppError::throw("sendonly is more than 1"));
        }
        let mut m = MediaEngine::default();
        m.register_default_codecs()?;
        m.register_header_extension(
            RTCRtpHeaderExtensionCapability {
                uri: SDES_MID_URI.to_owned(),
            },
            RTPCodecType::Video,
            Some(RTCRtpTransceiverDirection::Recvonly),
        )?;
        m.register_header_extension(
            RTCRtpHeaderExtensionCapability {
                uri: SDES_RTP_STREAM_ID_URI.to_owned(),
            },
            RTPCodecType::Video,
            Some(RTCRtpTransceiverDirection::Recvonly),
        )?;
        let mut registry = Registry::new();
        registry = register_default_interceptors(registry, &mut m)?;
        let mut s = SettingEngine::default();
        s.detach_data_channels();
        let api = APIBuilder::new()
            .with_media_engine(m)
            .with_interceptor_registry(registry)
            .with_setting_engine(s)
            .build();
        let config = RTCConfiguration {
            ice_servers: self.ice_server.clone(),
            ..Default::default()
        };
        let peer = Arc::new(api.new_peer_connection(config).await?);
        let mut transceiver_kinds = vec![];
        if media_info.video_transceiver.0 > 0 {
            transceiver_kinds.push(RTPCodecType::Video);
        }
        if media_info.audio_transceiver.0 > 0 {
            transceiver_kinds.push(RTPCodecType::Audio);
        }
        for kind in transceiver_kinds {
            let _ = peer
                .add_transceiver_from_kind(
                    kind,
                    Some(RTCRtpTransceiverInit {
                        direction: RTCRtpTransceiverDirection::Recvonly,
                        send_encodings: Vec::new(),
                    }),
                )
                .await?;
        }
        Ok(peer)
    }

    pub async fn publish_is_svc(&self) -> bool {
        let publish = self.publish.read().await;
        if publish.is_none() {
            return false;
        }
        publish.as_ref().unwrap().media_info.video_transceiver.2
    }

    pub async fn publish_svc_rids(&self) -> Result<Vec<String>> {
        let publish_tracks = self.publish_tracks.read().await;
        let rids = publish_tracks
            .iter()
            .filter(|t| t.kind == RTPCodecType::Video)
            .map(|t| t.rid.clone())
            .collect::<Vec<_>>();
        Ok(rids)
    }

    pub async fn select_kind_rid(&self, id: String, kind: RTPCodecType, rid: String) -> Result<()> {
        let subscribe_group = self.subscribe_group.read().await;
        for subscribe in subscribe_group.iter() {
            if subscribe.id == id {
                subscribe.select_kind_rid(kind, rid)?;
                break;
            }
        }
        Ok(())
    }

    pub(crate) async fn publish_track_up(
        &self,
        peer: Arc<RTCPeerConnection>,
        track: Arc<TrackRemote>,
    ) -> Result<()> {
        let publish_track_remote =
            PublishTrackRemote::new(self.stream.clone(), get_peer_id(&peer), track).await;
        let mut publish_tracks = self.publish_tracks.write().await;
        publish_tracks.push(publish_track_remote);
        publish_tracks.sort_by(|a, b| a.rid.cmp(&b.rid));
        let _ = self.publish_tracks_change.0.send(());
        Ok(())
    }

    pub(crate) async fn publish_data_channel(
        &self,
        _peer: Arc<RTCPeerConnection>,
        id: u32,
        dc: Arc<RTCDataChannel>,
    ) -> Result<()> {
        let group_sender = self.data_channel_forward.sender.clone();
        let (user_sender, _user_receiver) = broadcast::channel(128);

        let mut user_sender_map = self.user_sender_map.write().await;
        user_sender_map.insert(id.clone(), user_sender.clone());
        drop(user_sender_map);

        Self::data_channel_forward(
            id,
            dc,
            group_sender,
            user_sender,
            self.user_sender_map.clone(),
        )
        .await;

        Ok(())
    }
}

// subscribe
impl PeerForwardInternal {
    pub(crate) async fn new_subscription_peer(
        &self,
        media_info: MediaInfo,
    ) -> Result<Arc<RTCPeerConnection>> {
        if !self.publish_is_some().await {
            return Err(AppError::throw("publish is none"));
        }
        if media_info.video_transceiver.1 > 1 || media_info.audio_transceiver.1 > 1 {
            return Err(AppError::throw("recvonly is more than 1"));
        }
        let mut m = MediaEngine::default();
        m.register_default_codecs()?;
        let mut registry = Registry::new();
        registry = register_default_interceptors(registry, &mut m)?;
        let mut s = SettingEngine::default();
        s.detach_data_channels();
        let api = APIBuilder::new()
            .with_media_engine(m)
            .with_interceptor_registry(registry)
            .with_setting_engine(s)
            .build();
        let config = RTCConfiguration {
            ice_servers: self.ice_server.clone(),
            ..Default::default()
        };
        let peer = Arc::new(api.new_peer_connection(config).await?);
        {
            let s = SubscribeRTCPeerConnection::new(
                self.stream.clone(),
                peer.clone(),
                self.publish_rtcp_channel.0.clone(),
                (
                    self.publish_tracks.clone(),
                    self.publish_tracks_change.0.clone(),
                ),
                (
                    Self::new_sender(&peer, RTPCodecType::Video, media_info.video_transceiver.1)
                        .await?,
                    Self::new_sender(&peer, RTPCodecType::Audio, media_info.audio_transceiver.1)
                        .await?,
                ),
            )
            .await;
            self.subscribe_group.write().await.push(s);
            *self.subscribe_leave_time.write().await = 0;
        }
        self.send_event(ForwardEventType::SubscribeUp, get_peer_id(&peer))
            .await;

        Ok(peer)
    }

    async fn new_sender(
        peer: &Arc<RTCPeerConnection>,
        kind: RTPCodecType,
        recv_sender: u8,
    ) -> Result<Option<Arc<RTCRtpSender>>> {
        Ok(if recv_sender > 0 {
            Some(
                peer.add_transceiver_from_kind(
                    kind,
                    Some(RTCRtpTransceiverInit {
                        direction: RTCRtpTransceiverDirection::Sendonly,
                        send_encodings: Vec::new(),
                    }),
                )
                .await?
                .sender()
                .await,
            )
        } else {
            None
        })
    }

    pub async fn remove_subscribe(&self, id: u32, peer: Arc<RTCPeerConnection>) -> Result<()> {
        let mut flag = false;
        let session = get_peer_id(&peer);
        {
            let mut subscribe_peers = self.subscribe_group.write().await;
            for i in 0..subscribe_peers.len() {
                let subscribe = &mut subscribe_peers[i];
                if subscribe.id == session {
                    flag = true;
                    subscribe_peers.remove(i);
                    break;
                }
            }
            if subscribe_peers.is_empty() {
                *self.subscribe_leave_time.write().await = Utc::now().timestamp_millis();
            }
        }
        if flag {
            self.send_event(ForwardEventType::SubscribeDown, get_peer_id(&peer))
                .await;

            let mut user_sender_map = self.user_sender_map.write().await;
            user_sender_map.remove(&id);
            drop(user_sender_map);

            Ok(())
        } else {
            Err(AppError::throw("not found session"))
        }
    }

    pub(crate) async fn subscribe_data_channel(
        &self,
        _peer: Arc<RTCPeerConnection>,
        id: u32,
        dc: Arc<RTCDataChannel>,
    ) -> Result<()> {
        let group_sender = self.data_channel_forward.sender.clone();
        let (user_sender, _user_receiver) = broadcast::channel(32);

        let mut user_sender_map = self.user_sender_map.write().await;
        user_sender_map.insert(id.clone(), user_sender.clone());
        drop(user_sender_map);

        Self::data_channel_forward(
            id,
            dc,
            group_sender,
            user_sender,
            self.user_sender_map.clone(),
        )
        .await;

        Ok(())
    }

    // This function has not used currently, but seems worth to keep retain
    // pub(crate) async fn get_publish_peer(&self) -> Option<Arc<RTCPeerConnection>> {
    //     let publish = self.publish.read().await;
    //     publish.as_ref().map(|p| p.peer.clone())
    // }

    async fn send_event(&self, r#type: ForwardEventType, session: String) {
        let _ = self.event_sender.send(ForwardEvent {
            r#type,
            session,
            stream_info: self.info().await,
        });
    }
}
