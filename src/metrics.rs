use lazy_static::lazy_static;
use prometheus::{Registry, TextEncoder};

lazy_static! {
    pub static ref REGISTRY: Registry =
        Registry::new_custom(Some("webrtc-chat-sfu".to_string()), None).unwrap();
    pub static ref ENCODER: TextEncoder = TextEncoder::new();
}
