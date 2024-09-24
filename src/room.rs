use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

use crate::config::Config;
use crate::forward::rtc::client::Client;
use crate::result::Result;
use crate::route::room::RoomInfoJson;
use crate::rtc::{Forwarder, ForwarderConfig};

use tokio::sync::RwLock;

pub struct Room {
    id: i32,
    name: String,

    needs_host: bool,
    is_public: bool,
    capacity: u32,

    client_map: Arc<RwLock<HashMap<i32, Client>>>,

    password_hash: u32,
    master_key_hash: u32,

    description: String,

    forwarder: Arc<RwLock<Forwarder>>,
}

impl Room {
    pub fn new(
        id: i32,
        name: String,
        needs_host: bool,
        is_public: bool,
        capacity: u32,
        password: String,
        master_key: String,
        description: String,
        config: Config,
    ) -> Self {
        let client_map: Arc<RwLock<HashMap<i32, Client>>> = Default::default();
        let forwarder = Arc::new(RwLock::new(Forwarder::new(ForwarderConfig::from_config(
            config.clone(),
        ))));

        let room: Room = Self {
            id: id,
            name: name,

            needs_host: needs_host,
            is_public: is_public,

            capacity: capacity,

            client_map: client_map,

            password_hash: utils::unique::hash_from_string(password),
            master_key_hash: utils::unique::hash_from_string(master_key),

            description: description,

            forwarder: forwarder,
            //cfg: cfg,
        };

        room
    }

    pub fn id(&self) -> i32 {
        self.id
    }

    pub fn name(&self) -> String {
        String::from_str(self.name.as_str()).unwrap()
    }

    pub fn is_public(&self) -> bool {
        self.is_public
    }

    pub fn capacity(&self) -> u32 {
        self.capacity
    }

    pub fn client_map(&self) -> Arc<RwLock<HashMap<i32, Client>>> {
        self.client_map.clone()
    }

    pub fn forwarder(&self) -> Arc<RwLock<Forwarder>> {
        self.forwarder.clone()
    }

    pub fn description(&self) -> String {
        String::from_str(&self.description.as_str()).unwrap()
    }
}

impl Room {
    pub fn info(&self) -> RoomInfoJson {
        RoomInfoJson {
            room_id: self.id(),
            room_name: self.name(),
            room_capacity: self.capacity(),
            description: self.description(),
        }
    }

    pub fn check_password(&self, pass: String) -> bool {
        let hash = utils::unique::hash_from_string(pass);

        self.password_hash == hash
    }

    pub fn check_master_key(&self, key: String) -> bool {
        let hash = utils::unique::hash_from_string(key);

        self.master_key_hash == hash
    }

    pub async fn all_user_delete(&mut self) -> Result<bool> {
        let client_map = self.client_map();
        let clients = client_map.read().await;
        let user_ids: Vec<i32> = clients.keys().into_iter().map(|user_id| *user_id).collect();
        drop(clients);

        for user_id in user_ids {
            self.user_delete(user_id, 0, false).await?;
        }

        return Ok(true);
    }

    pub async fn user_delete(
        &mut self,
        user_id: i32,
        user_token: u32,
        check_token: bool,
    ) -> Result<bool> {
        let clients = self.client_map.read().await;
        let client = clients.get(&user_id).cloned();
        drop(clients);

        if let Some(mut client) = client {
            if check_token && !client.check_token(user_token.clone()) {
                return Ok(false);
            }
            for stream in client.get_streams().await {
                let forwarder = self.forwarder.write().await;
                forwarder.stream_delete(stream.clone()).await?;
                drop(forwarder);
                client.remove_stream(stream.clone()).await?;
            }

            let mut clients = self.client_map.write().await;
            clients.remove(&user_id);
            drop(clients);

            return Ok(true);
        }

        Ok(false)
    }

    pub async fn join(
        &mut self,
        user_name: String,
        master_key: String,
        user_id: &mut i32,
        user_token: &mut u32,
    ) -> Result<bool> {
        let mut clients = self.client_map.write().await;

        if self.needs_host {
            if master_key != "" {
                if self.check_master_key(master_key) && !clients.contains_key(&0) {
                    *user_id = 0;
                    *user_token = utils::unique::generate_unique_u32();
                    clients.insert(
                        *user_id,
                        Client::new(user_id.clone(), user_token.clone(), user_name.clone()).await?,
                    );
                    return Ok(true);
                }
                return Ok(false);
            } else {
                for i in 1..self.capacity.try_into().unwrap() {
                    if !clients.contains_key(&i) {
                        *user_id = i.try_into().unwrap();
                        *user_token = utils::unique::generate_unique_u32();
                        clients.insert(
                            *user_id,
                            Client::new(user_id.clone(), user_token.clone(), user_name.clone())
                                .await?,
                        );
                        return Ok(true);
                    }
                }
                return Ok(false);
            }
        } else {
            for i in 0..self.capacity.try_into().unwrap() {
                if !clients.contains_key(&i) {
                    *user_id = i.try_into().unwrap();
                    *user_token = utils::unique::generate_unique_u32();
                    clients.insert(
                        *user_id,
                        Client::new(user_id.clone(), user_token.clone(), user_name.clone()).await?,
                    );
                    return Ok(true);
                }
            }
            return Ok(false);
        }
    }
}
