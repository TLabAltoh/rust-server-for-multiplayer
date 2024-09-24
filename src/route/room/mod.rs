use serde::{Deserialize, Serialize};

pub mod create;
pub mod delete;
pub mod exit;
pub mod join;
pub mod room;

#[derive(Serialize, Deserialize)]
pub struct RoomInfoJson {
    pub room_id: i32,
    pub room_name: String,
    pub room_capacity: u32,
    pub description: String,
}
