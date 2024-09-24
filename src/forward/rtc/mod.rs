use std::sync::Arc;

use tokio::sync::Mutex;
use tracing::info;

use webrtc::ice_transport::ice_gatherer::OnLocalCandidateHdlrFn;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::peer_connection::RTCPeerConnection;

use internal::PeerForwardInternal;
use media::MediaInfo;
use message::ForwardInfo;

use crate::error::AppError;
use crate::result::Result;

pub mod client;
pub mod internal;
pub mod media;
pub mod message;
pub mod publish;
pub mod rtcp;
pub mod subscribe;
pub mod track;

#[derive(Clone)]
pub struct PeerForward {
    publish_lock: Arc<Mutex<()>>,
    internal: Arc<PeerForwardInternal>,
}

impl PeerForward {
    pub fn new(stream: impl ToString, ice_server: Vec<RTCIceServer>) -> Self {
        PeerForward {
            publish_lock: Arc::new(Mutex::new(())),
            internal: Arc::new(PeerForwardInternal::new(stream, ice_server)),
        }
    }

    pub async fn gen_virtual_publish(
        &self,
        on_ice_candidate: OnLocalCandidateHdlrFn,
    ) -> Result<(Arc<RTCPeerConnection>, RTCSessionDescription, String)> {
        let peer = self.internal.new_virtual_publish_peer().await?;
        let pc = Arc::downgrade(&peer);
        peer.on_ice_candidate(on_ice_candidate);
        peer.on_peer_connection_state_change(Box::new(move |s: RTCPeerConnectionState| {
            if let Some(pc) = pc.upgrade() {
                tokio::spawn(async move {
                    match s {
                        RTCPeerConnectionState::Failed | RTCPeerConnectionState::Disconnected => {
                            let _ = pc.close().await;
                        }
                        RTCPeerConnectionState::Closed => {}
                        _ => {}
                    };
                });
            }
            Box::pin(async {})
        }));
        let dc = peer.create_data_channel("data", None).await?;
        dc.on_open(Box::new(move || Box::pin(async {})));
        let offer = peer.create_offer(None).await?;
        peer.set_local_description(offer).await?;
        let description = peer
            .local_description()
            .await
            .ok_or(anyhow::anyhow!("failed to get local description"))?;
        let session = get_peer_id(&peer);
        Ok((peer, description, session))
    }

    pub async fn set_publish(
        &self,
        id: u32,
        offer: RTCSessionDescription,
        on_ice_candidate: OnLocalCandidateHdlrFn,
    ) -> Result<(Arc<RTCPeerConnection>, RTCSessionDescription, String)> {
        if self.internal.publish_is_some().await {
            return Err(AppError::stream_already_exists(
                "A connection has already been established",
            ));
        }
        let _ = self.publish_lock.lock().await;
        if self.internal.publish_is_some().await {
            return Err(AppError::stream_already_exists(
                "A connection has already been established",
            ));
        }
        let peer = self
            .internal
            .new_publish_peer(MediaInfo::try_from(offer.unmarshal()?)?)
            .await?;
        let internal = Arc::downgrade(&self.internal);
        let pc = Arc::downgrade(&peer);
        peer.on_ice_candidate(on_ice_candidate);
        peer.on_peer_connection_state_change(Box::new(move |s: RTCPeerConnectionState| {
            if let (Some(internal), Some(pc)) = (internal.upgrade(), pc.upgrade()) {
                tokio::spawn(async move {
                    info!(
                        "[{}] [publish] [{}] connection state changed: {}",
                        internal.stream,
                        get_peer_id(&pc),
                        s
                    );
                    match s {
                        RTCPeerConnectionState::Failed | RTCPeerConnectionState::Disconnected => {
                            let _ = pc.close().await;
                        }
                        RTCPeerConnectionState::Closed => {
                            let _ = internal.remove_publish(pc).await;
                        }
                        _ => {}
                    };
                });
            }
            Box::pin(async {})
        }));
        let internal = Arc::downgrade(&self.internal);
        let pc = Arc::downgrade(&peer);
        peer.on_track(Box::new(move |track, _, _| {
            if let (Some(internal), Some(pc)) = (internal.upgrade(), pc.upgrade()) {
                tokio::spawn(async move {
                    let _ = internal.publish_track_up(pc, track).await;
                });
            }
            Box::pin(async {})
        }));
        let internal = Arc::downgrade(&self.internal);
        let pc = Arc::downgrade(&peer);
        peer.on_data_channel(Box::new(move |dc| {
            if let (Some(internal), Some(pc)) = (internal.upgrade(), pc.upgrade()) {
                tokio::spawn(async move {
                    let _ = internal.publish_data_channel(pc, id, dc).await;
                });
            }
            Box::pin(async {})
        }));
        let description = peer_complete(offer, peer.clone()).await?;
        self.internal.set_publish(peer.clone()).await?;
        let session = get_peer_id(&peer);
        Ok((peer, description, session))
    }

    pub async fn publish_is_ok(&self) -> bool {
        return self.internal.publish_is_ok().await;
    }

    pub async fn add_subscribe(
        &self,
        id: u32,
        offer: RTCSessionDescription,
        on_ice_candidate: OnLocalCandidateHdlrFn,
    ) -> Result<(Arc<RTCPeerConnection>, RTCSessionDescription, String)> {
        if !self.internal.publish_is_ok().await {
            return Err(AppError::throw("publish is not ok"));
        }
        let peer = self
            .internal
            .new_subscription_peer(MediaInfo::try_from(offer.unmarshal()?)?)
            .await?;
        let internal = Arc::downgrade(&self.internal);
        let pc = Arc::downgrade(&peer);
        peer.on_ice_candidate(on_ice_candidate);
        peer.on_peer_connection_state_change(Box::new(move |s: RTCPeerConnectionState| {
            if let (Some(internal), Some(pc)) = (internal.upgrade(), pc.upgrade()) {
                tokio::spawn(async move {
                    info!(
                        "[{}] [subscribe] [{}] connection state changed: {}",
                        internal.stream,
                        get_peer_id(&pc),
                        s
                    );
                    match s {
                        RTCPeerConnectionState::Failed | RTCPeerConnectionState::Disconnected => {
                            let _ = pc.close().await;
                        }

                        RTCPeerConnectionState::Closed => {
                            let _ = internal.remove_subscribe(pc).await;
                        }
                        _ => {}
                    }
                });
            }
            Box::pin(async {})
        }));
        let internal = Arc::downgrade(&self.internal);
        let pc = Arc::downgrade(&peer);
        peer.on_data_channel(Box::new(move |dc| {
            if let (Some(internal), Some(pc)) = (internal.upgrade(), pc.upgrade()) {
                tokio::spawn(async move {
                    let _ = internal.subscribe_data_channel(pc, id, dc).await;
                });
            }
            Box::pin(async {})
        }));
        let (sdp, session) = (
            peer_complete(offer, peer.clone()).await?,
            get_peer_id(&peer),
        );
        Ok((peer, sdp, session))
    }

    // This function has not used currently, but seems worth to keep retain
    // pub async fn remove_peer(&self, session: String) -> Result<bool> {
    //     self.internal.remove_peer(session).await
    // }

    pub async fn close(&self) -> Result<()> {
        self.internal.close().await?;
        Ok(())
    }

    pub async fn info(&self) -> ForwardInfo {
        self.internal.info().await
    }
}

async fn peer_complete(
    offer: RTCSessionDescription,
    peer: Arc<RTCPeerConnection>,
) -> Result<RTCSessionDescription> {
    peer.set_remote_description(offer).await?;
    let answer = peer.create_answer(None).await?;
    // Use Trickle ICE
    peer.set_local_description(answer).await?;

    let description = peer
        .local_description()
        .await
        .ok_or(anyhow::anyhow!("failed to get local description"))?;
    Ok(description)
}

pub(crate) fn get_peer_id(peer: &Arc<RTCPeerConnection>) -> String {
    let digest = md5::compute(peer.get_stats_id());
    format!("{:x}", digest)
}
