use std::sync::Arc;

use tokio::sync::RwLock;

use crate::{error::AppError, Result};

#[derive(Clone)]
pub struct Client {
    _name: String,
    _id: i32,
    token: u32,
    stream_map: Arc<RwLock<Vec<String>>>,
}

impl Client {
    pub async fn new(id: i32, token: u32, name: String) -> Result<Self> {
        Ok(Self {
            _name: name,
            _id: id,
            token: token,
            stream_map: Arc::new(RwLock::new(Vec::new())),
        })
    }

    pub fn check_token(&self, token: u32) -> bool {
        self.token == token
    }
}

impl Client {
    pub async fn add_stream(&mut self, stream: String) -> Result<()> {
        let mut streams = self.stream_map.write().await;
        if !streams.contains(&stream) {
            streams.push(stream.clone());
            return Ok(());
        }

        Err(AppError::stream_already_exists(stream.clone()))
    }

    pub async fn remove_stream(&mut self, stream: String) -> Result<()> {
        let mut streams = self.stream_map.write().await;
        if streams.contains(&stream) {
            let index = streams.binary_search(&stream);
            if index.is_ok() {
                streams.remove(index.unwrap());
                return Ok(());
            }
        }

        Err(AppError::stream_not_found(stream.clone()))
    }

    pub async fn get_streams(&self) -> Vec<String> {
        let streams = self.stream_map.read().await;
        return streams.iter().map(|stream| stream.clone()).collect();
    }
}
