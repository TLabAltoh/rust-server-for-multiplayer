use serde::{Deserialize, Serialize};

pub mod create;
pub mod delete;
pub mod exit;
pub mod join;
pub mod room;

#[derive(Serialize, Deserialize)]
pub struct RoomInfoJson {
    pub id: i32,
    pub name: String,
    pub capacity: u32,
    pub description: String,
}
