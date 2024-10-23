use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use std::vec;

use webrtc::ice_transport::ice_gatherer::OnLocalCandidateHdlrFn;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::peer_connection::RTCPeerConnection;

use crate::config::Config;
use crate::error::AppError;
use crate::forward::rtc::message::{ForwardInfo, Layer};
use crate::forward::rtc::{OnPeerConnectionEvtHdlrFn, PeerForward};
use crate::result::Result;

use chrono::{DateTime, Utc};

use tokio::sync::RwLock;
use tracing::{debug, info};

pub mod convert;

pub struct Forwarder {
    stream_map: Arc<RwLock<HashMap<String, PeerForward>>>,
    config: ForwarderConfig,
}

pub struct ForwarderConfig {
    pub ice_servers: Vec<RTCIceServer>,
    pub reforward_close_sub: bool,
    pub publish_leave_timeout: u64,
}

impl ForwarderConfig {
    pub fn from_config(cfg: Config) -> Self {
        let ice_servers: Vec<RTCIceServer> = cfg
            .ice_servers
            .clone()
            .into_iter()
            .map(|i| i.into())
            .collect();
        Self {
            ice_servers,
            reforward_close_sub: cfg.stream_info.reforward_close_sub,
            publish_leave_timeout: cfg.stream_info.publish_leave_timeout.0,
        }
    }
}

impl Forwarder {
    pub fn new(cfg: ForwarderConfig) -> Self {
        let stream_map: Arc<RwLock<HashMap<String, PeerForward>>> = Default::default();
        tokio::spawn(Self::publish_check_tick(
            stream_map.clone(),
            cfg.publish_leave_timeout,
        ));

        let live: Forwarder = Self {
            stream_map: stream_map,
            config: cfg,
        };

        live
    }

    async fn publish_check_tick(
        stream_map: Arc<RwLock<HashMap<String, PeerForward>>>,
        publish_leave_timeout: u64,
    ) {
        let publish_leave_timeout_i64: i64 = publish_leave_timeout.try_into().unwrap();
        loop {
            tokio::time::sleep(Duration::from_millis(1000)).await;
            let stream_map_read = stream_map.read().await;
            let mut remove_streams = vec![];
            for (stream, forward) in stream_map_read.iter() {
                let forward_info = forward.info().await;
                if forward_info.publish_leave_time > 0
                    && Utc::now().timestamp_millis() - forward_info.publish_leave_time
                        > publish_leave_timeout_i64
                {
                    remove_streams.push(stream.clone());
                }
            }
            if remove_streams.is_empty() {
                continue;
            }
            drop(stream_map_read);
            let mut stream_map = stream_map.write().await;
            for stream in remove_streams.iter() {
                if let Some(forward) = stream_map.get(stream) {
                    let forward_info = forward.info().await;
                    if forward_info.publish_leave_time > 0
                        && Utc::now().timestamp_millis() - forward_info.publish_leave_time
                            > publish_leave_timeout_i64
                    {
                        let _ = forward.close().await;
                        stream_map.remove(stream);
                        let publish_leave_time =
                            DateTime::from_timestamp_millis(forward_info.publish_leave_time)
                                .unwrap()
                                .format("%Y-%m-%d %H:%M:%S")
                                .to_string();
                        info!(
                            "stream : {}, publish leave timeout, publish leave time : {}",
                            stream, publish_leave_time
                        );
                    }
                }
            }
        }
    }

    pub async fn stream_create(&self, stream: String) -> std::result::Result<(), anyhow::Error> {
        let mut stream_map = self.stream_map.write().await;
        let forward = stream_map.get(&stream).cloned();
        if forward.is_some() {
            return Err(anyhow::anyhow!("stream already exists"));
        }
        debug!("create stream: {}", stream.clone());
        let forward = self.do_stream_create(stream.clone()).await;
        stream_map.insert(stream.clone(), forward);
        drop(stream_map);
        Ok(())
    }

    async fn do_stream_create(&self, stream: String) -> PeerForward {
        let forward = PeerForward::new(stream.clone(), self.config.ice_servers.clone());
        forward
    }

    pub async fn stream_delete(&self, stream: String) -> std::result::Result<(), anyhow::Error> {
        let mut stream_map = self.stream_map.write().await;
        let forward = stream_map.get(&stream).cloned();
        let _ = match forward {
            Some(forward) => forward.close().await,
            None => return Err(anyhow::anyhow!("stream not exists")),
        };
        stream_map.remove(&stream);
        drop(stream_map);

        info!("remove stream : {}", stream);
        Ok(())
    }

    pub async fn is_stream_exists(&self, stream: String) -> Result<bool> {
        let stream_map = self.stream_map.read().await;
        return Ok(stream_map.contains_key(&stream));
    }

    pub async fn layers(&self, stream: String) -> Result<Vec<Layer>> {
        let stream_map = self.stream_map.read().await;
        let forward = stream_map.get(&stream).cloned();
        drop(stream_map);
        if let Some(forward) = forward {
            forward.layers().await
        } else {
            Err(AppError::stream_not_found("stream not exists"))
        }
    }

    pub async fn select_layer(
        &self,
        stream: String,
        session: String,
        layer: Option<Layer>,
    ) -> Result<()> {
        let stream_map = self.stream_map.read().await;
        let forward = stream_map.get(&stream).cloned();
        drop(stream_map);
        if let Some(forward) = forward {
            forward.select_layer(session, layer).await
        } else {
            Err(AppError::stream_not_found("stream not exists"))
        }
    }

    pub async fn virtual_publish(
        &self,
        stream: String,
        on_ice_candidate: OnLocalCandidateHdlrFn,
    ) -> Result<(Arc<RTCPeerConnection>, RTCSessionDescription, String)> {
        let stream_map = self.stream_map.read().await;
        let forward = stream_map.get(&stream).cloned();
        drop(stream_map);
        if let Some(forward) = forward {
            forward.gen_virtual_publish(on_ice_candidate).await
        } else {
            let forward = PeerForward::new(stream.clone(), self.config.ice_servers.clone());
            let (peer, sdp, session) = forward.gen_virtual_publish(on_ice_candidate).await?;
            let mut stream_map = self.stream_map.write().await;
            if stream_map.contains_key(&stream) {
                let _ = forward.close().await;
                return Err(AppError::stream_already_exists("stream already exists"));
            }
            info!("add stream : {}", stream);
            stream_map.insert(stream.clone(), forward);
            Ok((peer, sdp, session))
        }
    }

    pub async fn publish(
        &self,
        stream: String,
        id: u32,
        offer: RTCSessionDescription,
        on_ice_candidate: OnLocalCandidateHdlrFn,
        on_peer_connected: OnPeerConnectionEvtHdlrFn,
    ) -> Result<(Arc<RTCPeerConnection>, RTCSessionDescription, String)> {
        let stream_map = self.stream_map.read().await;
        let forward = stream_map.get(&stream).cloned();
        drop(stream_map);
        if let Some(forward) = forward {
            forward
                .set_publish(id, offer, on_ice_candidate, on_peer_connected)
                .await
        } else {
            let forward = PeerForward::new(stream.clone(), self.config.ice_servers.clone());
            let (peer, sdp, session) = forward
                .set_publish(id, offer, on_ice_candidate, on_peer_connected)
                .await?;
            let mut stream_map = self.stream_map.write().await;
            if stream_map.contains_key(&stream) {
                let _ = forward.close().await;
                return Err(AppError::stream_already_exists("stream already exists"));
            }
            info!("add stream : {}", stream);
            stream_map.insert(stream.clone(), forward);
            Ok((peer, sdp, session))
        }
    }

    pub async fn publish_is_ok(&self, stream: String) -> Result<bool> {
        let stream_map = self.stream_map.read().await;
        let forward = stream_map.get(&stream).cloned();
        drop(stream_map);
        if let Some(forward) = forward {
            Ok(forward.publish_is_ok().await)
        } else {
            Err(AppError::stream_not_found("stream not exists"))
        }
    }

    pub async fn subscribe(
        &self,
        stream: String,
        id: u32,
        offer: RTCSessionDescription,
        on_ice_candidate: OnLocalCandidateHdlrFn,
        on_peer_connected: OnPeerConnectionEvtHdlrFn,
    ) -> Result<(Arc<RTCPeerConnection>, RTCSessionDescription, String)> {
        let stream_map = self.stream_map.read().await;
        let forward = stream_map.get(&stream).cloned();
        drop(stream_map);
        if let Some(forward) = forward {
            let (peer, sdp, session) = forward
                .add_subscribe(id, offer, on_ice_candidate, on_peer_connected)
                .await?;
            Ok((peer, sdp, session))
        } else {
            Err(AppError::stream_not_found("stream not exists"))
        }
    }

    pub async fn forward_infos(&self, streams: Vec<String>) -> Vec<ForwardInfo> {
        let mut streams = streams.clone();
        streams.retain(|stream| !stream.trim().is_empty());
        let mut resp = vec![];
        let stream_map = self.stream_map.read().await;
        for (stream, forward) in stream_map.iter() {
            if streams.is_empty() || streams.contains(stream) {
                resp.push(forward.info().await);
            }
        }
        resp
    }
}
