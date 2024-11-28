// This source is originally based on axum-ws-rooms, but renamed Room to Group.
// See https://github.com/mohammadjavad948/axum-ws-rooms

use std::{
    collections::HashMap,
    error::Error,
    fmt,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc, RwLock,
    },
    vec,
};

use tokio::{
    sync::{broadcast, Mutex},
    task::JoinHandle,
};

use tracing::debug;

pub struct Group {
    pub name: String,
    tx: broadcast::Sender<Vec<u8>>,
    inner_user: Mutex<Vec<u32>>,
    user_count: AtomicU32,
    user_tasks: Mutex<HashMap<u32, UserTask>>,
    user_senders: Arc<RwLock<HashMap<u32, broadcast::Sender<Vec<u8>>>>>,
}

pub struct GroupsManager {
    inner: Mutex<HashMap<String, Group>>,
    users_group: Mutex<HashMap<u32, Vec<String>>>,
}

#[derive(Debug)]
pub enum GroupError {
    /// group does not exists
    GroupNotFound,
    /// can not send message to group
    MessageSendFail,
    /// you have not called init_user
    NotInitiated,
}

impl Error for GroupError {}

impl fmt::Display for GroupError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            GroupError::GroupNotFound => {
                write!(f, "target group not found")
            }
            GroupError::NotInitiated => {
                write!(f, "user is not initiated")
            }
            GroupError::MessageSendFail => {
                write!(f, "failed to send message to the group")
            }
        }
    }
}

pub struct UserTask {
    broadcast_pipe: JoinHandle<()>,
}

impl Group {
    pub fn new(name: String, capacity: Option<usize>) -> Group {
        let (tx, _rx) = broadcast::channel(capacity.unwrap_or(100));
        Group {
            name,
            tx,
            inner_user: Mutex::new(vec![]),
            user_count: AtomicU32::new(0),
            user_tasks: Mutex::new(HashMap::new()),
            user_senders: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn join(&self, user: u32, capacity: Option<usize>) -> broadcast::Sender<Vec<u8>> {
        let mut inner = self.inner_user.lock().await;
        if !inner.contains(&user) {
            inner.push(user);
            self.user_count.fetch_add(1, Ordering::SeqCst);
        }

        let mut user_tasks = self.user_tasks.lock().await;
        match user_tasks.entry(user.clone()) {
            std::collections::hash_map::Entry::Occupied(_o) => {}
            std::collections::hash_map::Entry::Vacant(v) => {
                let (user_sender, _receiver) =
                    broadcast::channel::<Vec<u8>>(capacity.unwrap_or(100));
                let mut user_senders = self.user_senders.write().unwrap();
                user_senders.insert(user, user_sender.clone());

                let pipe_sender = user_sender.clone();
                let mut broadcast_pipe = self.tx.subscribe();
                let broadcast_pipe = tokio::spawn(async move {
                    while let Ok(data) = broadcast_pipe.recv().await {
                        let from = u32::from_be_bytes([data[4], data[3], data[2], data[1]]);
                        if from == user {
                            continue; // This message was sent from own
                        }
                        if let Err(_err) = pipe_sender.send(data) {
                            return;
                        }
                    }
                });
                v.insert(UserTask { broadcast_pipe });
            }
        };
        self.tx.clone()
    }

    pub async fn leave(&self, user: u32) {
        let mut inner = self.inner_user.lock().await;
        if let Some(pos) = inner.iter().position(|x| *x == user) {
            inner.swap_remove(pos);
            self.user_count.fetch_sub(1, Ordering::SeqCst);
        }

        let mut user_tasks = self.user_tasks.lock().await;
        if let Some(user_task) = user_tasks.get(&user) {
            user_task.broadcast_pipe.abort();
            user_tasks.remove(&user);
        }

        let mut user_senders = self.user_senders.write().unwrap();
        if let Some(_user_sender) = user_senders.get(&user) {
            user_senders.remove(&user);
        }

        let mut header = vec![0u8; 5]; // typ (1) + from (0 ~ 3) + to (4 ~ 7)
        for i in 0..4 {
            header[i + 1] = (user >> (i * 8)) as u8;
        }

        header[0] = 2; // close
        let mut dummy_buf = vec![0u8; 4];
        for i in 0..4 {
            dummy_buf[i] = (user >> (i * 8)) as u8;
        }

        if let Err(err) = self.tx.send([header.clone(), dummy_buf.clone()].concat()) {
            debug!("[ws] send socket err: {}", err);
        }
    }

    pub async fn contains_user(&self, user: &u32) -> bool {
        let inner = self.inner_user.lock().await;
        inner.contains(user)
    }

    pub fn is_empty(&self) -> bool {
        self.user_count.load(Ordering::SeqCst) == 0
    }

    pub fn get_sender(&self) -> broadcast::Sender<Vec<u8>> {
        self.tx.clone()
    }

    pub fn send(&self, data: Vec<u8>) -> Result<usize, broadcast::error::SendError<Vec<u8>>> {
        self.tx.send(data)
    }

    pub async fn send_to_user(
        &self,
        user: u32,
        data: Vec<u8>,
    ) -> Result<usize, broadcast::error::SendError<Vec<u8>>> {
        let inner = self.user_senders.read().unwrap();
        inner.get(&user).unwrap().send(data)
    }

    pub async fn get_user_sender(&self, user: u32) -> broadcast::Sender<Vec<u8>> {
        let inner = self.user_senders.read().unwrap();
        inner.get(&user).unwrap().clone()
    }

    pub fn get_user_sender_map(&self) -> Arc<RwLock<HashMap<u32, broadcast::Sender<Vec<u8>>>>> {
        self.user_senders.clone()
    }

    pub async fn user_tasks(&self) -> tokio::sync::MutexGuard<HashMap<u32, UserTask>> {
        self.user_tasks.lock().await
    }

    pub async fn users(&self) -> tokio::sync::MutexGuard<Vec<u32>> {
        self.inner_user.lock().await
    }

    pub async fn user_count(&self) -> u32 {
        self.user_count.load(Ordering::SeqCst)
    }
}

impl GroupsManager {
    pub fn new() -> Self {
        GroupsManager {
            inner: Mutex::new(HashMap::new()),
            users_group: Mutex::new(HashMap::new()),
        }
    }

    pub async fn new_group(&self, name: String, capacity: Option<usize>) {
        let mut groups = self.inner.lock().await;
        groups.insert(name.clone(), Group::new(name, capacity));
    }

    pub async fn group_exists(&self, name: &str) -> bool {
        let groups = self.inner.lock().await;
        match groups.get(name) {
            Some(_) => true,
            None => false,
        }
    }

    pub async fn join_or_create(
        &self,
        user: u32,
        group: String,
    ) -> Result<broadcast::Sender<Vec<u8>>, GroupError> {
        match self.group_exists(&group).await {
            true => self.join_group(group, user).await,
            false => {
                self.new_group(group.clone(), None).await;
                self.join_group(group, user).await
            }
        }
    }

    /// send a message to a group
    /// it will fail if there are no users in the group or
    /// if group does not exists
    pub async fn send_message_to_group(
        &self,
        group: String,
        data: Vec<u8>,
    ) -> Result<usize, GroupError> {
        let groups = self.inner.lock().await;
        groups
            .get(&group)
            .ok_or(GroupError::GroupNotFound)?
            .send(data)
            .map_err(|_| GroupError::MessageSendFail)
    }

    pub async fn send_message_to_user(
        &self,
        group: String,
        user: u32,
        data: Vec<u8>,
    ) -> Result<usize, GroupError> {
        let groups = self.inner.lock().await;
        groups
            .get(&group)
            .ok_or(GroupError::GroupNotFound)?
            .send_to_user(user, data)
            .await
            .map_err(|_| GroupError::MessageSendFail)
    }

    pub async fn init_user(&self, user: u32) {
        let mut users_group = self.users_group.lock().await;
        match users_group.entry(user) {
            std::collections::hash_map::Entry::Occupied(_) => {}
            std::collections::hash_map::Entry::Vacant(v) => {
                v.insert(vec![]);
            }
        }
    }

    pub async fn end_user(&self, user: u32) {
        let groups = self.inner.lock().await;
        let mut users_group = self.users_group.lock().await;
        match users_group.entry(user.clone()) {
            std::collections::hash_map::Entry::Occupied(o) => {
                let group_names = o.get();
                for group_name in group_names {
                    let group = groups.get(group_name);
                    if let Some(group) = group {
                        group.leave(user.clone()).await;
                    }
                }
                o.remove();
            }
            std::collections::hash_map::Entry::Vacant(_) => {}
        }
    }

    pub async fn join_group(
        &self,
        name: String,
        user: u32,
    ) -> Result<broadcast::Sender<Vec<u8>>, GroupError> {
        let groups = self.inner.lock().await;
        let mut users_group = self.users_group.lock().await;

        let sender = groups
            .get(&name)
            .ok_or(GroupError::GroupNotFound)?
            .join(user.clone(), None)
            .await;

        match users_group.entry(user.clone()) {
            std::collections::hash_map::Entry::Occupied(mut o) => {
                let groups = o.get_mut();
                let has = groups.iter().any(|x| *x == name);
                if !has {
                    groups.push(name);
                }
            }
            std::collections::hash_map::Entry::Vacant(v) => {
                v.insert(vec![name]);
            }
        };

        Ok(sender)
    }

    pub async fn remove_group(&self, group: String) {
        let mut groups = self.inner.lock().await;
        let mut users_group = self.users_group.lock().await;

        match groups.entry(group.clone()) {
            std::collections::hash_map::Entry::Vacant(_) => {}
            std::collections::hash_map::Entry::Occupied(el) => {
                for (user, user_task) in el.get().user_tasks().await.iter() {
                    user_task.broadcast_pipe.abort();
                    if let std::collections::hash_map::Entry::Occupied(mut users_group) =
                        users_group.entry(user.clone())
                    {
                        let vecotr = users_group.get_mut();
                        vecotr.retain(|group_name| *group_name != group);
                    }
                }
                el.remove();
            }
        }
    }

    pub async fn leave_group(&self, name: String, user: u32) -> Result<(), GroupError> {
        let groups = self.inner.lock().await;
        let mut users_group = self.users_group.lock().await;

        groups
            .get(&name)
            .ok_or(GroupError::GroupNotFound)?
            .leave(user.clone())
            .await;

        match users_group.entry(user.clone()) {
            std::collections::hash_map::Entry::Occupied(mut o) => {
                let vecotr = o.get_mut();
                vecotr.retain(|group_name| *group_name != name);
            }
            std::collections::hash_map::Entry::Vacant(_) => {}
        }

        Ok(())
    }

    pub async fn is_group_empty(&self, name: String) -> Result<bool, GroupError> {
        let groups = self.inner.lock().await;
        Ok(groups
            .get(&name)
            .ok_or(GroupError::GroupNotFound)?
            .is_empty())
    }

    pub async fn groups_count(&self) -> usize {
        let groups = self.inner.lock().await;

        groups.len()
    }

    pub async fn get_user_receiver(
        &self,
        group: String,
        user: u32,
    ) -> Result<broadcast::Receiver<Vec<u8>>, GroupError> {
        let groups = self.inner.lock().await;
        let user_receiver = groups
            .get(&group)
            .ok_or(GroupError::GroupNotFound)?
            .get_user_sender(user)
            .await
            .subscribe();
        Ok(user_receiver)
    }

    pub async fn get_user_sender(
        &self,
        group: String,
        user: u32,
    ) -> Result<broadcast::Sender<Vec<u8>>, GroupError> {
        let groups = self.inner.lock().await;
        let user_sender = groups
            .get(&group)
            .ok_or(GroupError::GroupNotFound)?
            .get_user_sender(user)
            .await;
        Ok(user_sender)
    }

    pub async fn get_user_sender_map(
        &self,
        group: String,
    ) -> Result<Arc<RwLock<HashMap<u32, broadcast::Sender<Vec<u8>>>>>, GroupError> {
        let groups = self.inner.lock().await;
        let user_sender_map = groups
            .get(&group)
            .ok_or(GroupError::GroupNotFound)?
            .get_user_sender_map();
        Ok(user_sender_map)
    }
}

impl Default for GroupsManager {
    fn default() -> Self {
        Self::new()
    }
}
