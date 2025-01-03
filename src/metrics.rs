use lazy_static::lazy_static;
use prometheus::{Registry, TextEncoder};

lazy_static! {
    pub static ref REGISTRY: Registry =
        Registry::new_custom(Some("rust-server-for-multiplayer".to_string()), None).unwrap();
    pub static ref ENCODER: TextEncoder = TextEncoder::new();
}
