use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;

use crate::http;

impl From<crate::forward::rtc::message::Layer> for http::response::Layer {
    fn from(value: crate::forward::rtc::message::Layer) -> Self {
        http::response::Layer {
            encoding_id: value.encoding_id,
        }
    }
}

impl From<crate::forward::rtc::message::ForwardInfo> for http::response::StreamInfo {
    fn from(value: crate::forward::rtc::message::ForwardInfo) -> Self {
        http::response::StreamInfo {
            id: value.id,
            create_time: value.create_time,
            publish_leave_time: value.publish_leave_time,
            subscribe_leave_time: value.subscribe_leave_time,
            publish_session_info: value.publish_session_info.map(|session| session.into()),
            subscribe_session_infos: value
                .subscribe_session_infos
                .into_iter()
                .map(|session| session.into())
                .collect(),
        }
    }
}

impl From<crate::forward::rtc::message::SessionInfo> for http::response::SessionInfo {
    fn from(value: crate::forward::rtc::message::SessionInfo) -> Self {
        http::response::SessionInfo {
            id: value.id,
            create_time: value.create_time,
            connect_state: convert_connect_state(value.connect_state),
        }
    }
}

fn convert_connect_state(
    connect_state: RTCPeerConnectionState,
) -> http::response::RTCPeerConnectionState {
    match connect_state {
        RTCPeerConnectionState::Unspecified => http::response::RTCPeerConnectionState::Unspecified,

        RTCPeerConnectionState::New => http::response::RTCPeerConnectionState::New,
        RTCPeerConnectionState::Connecting => http::response::RTCPeerConnectionState::Connecting,

        RTCPeerConnectionState::Connected => http::response::RTCPeerConnectionState::Connected,

        RTCPeerConnectionState::Disconnected => {
            http::response::RTCPeerConnectionState::Disconnected
        }

        RTCPeerConnectionState::Failed => http::response::RTCPeerConnectionState::Failed,

        RTCPeerConnectionState::Closed => http::response::RTCPeerConnectionState::Closed,
    }
}
