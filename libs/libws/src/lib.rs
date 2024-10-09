// This source is originally based on axum-ws-rooms, but renamed Room to Group.
// See https://github.com/mohammadjavad948/axum-ws-rooms

use std::{
    collections::HashMap,
    error::Error,
    fmt,
    sync::atomic::{AtomicU32, Ordering},
};

use tokio::{
    sync::{broadcast, Mutex},
    task::JoinHandle,
};

/// each group has a name and id and it contains `broadcast::sender<String>` which can be accessed
/// by `get_sender` method and you can send message to a groupe by calling `send` on group.
/// each group counts how many user it has and there is a method to check if its empty
/// if you want to join a group you can call `join` method and recieve a `broadcast::Sender<String>`
/// which you can subscribe to it and listen for incoming messages.
/// remember to leave the group after the user disconnects.
pub struct Group<T> {
    pub name: String,
    tx: broadcast::Sender<T>,
    inner_user: Mutex<Vec<String>>,
    user_count: AtomicU32,
}

/// this struct is used for managing multiple groups at once
/// ## how it works
/// everything is managed by a mutex
/// there is a hashmap of groups
/// a hashmap of user reciever
/// and a hashmap of user task
/// when a user joins a group a task will be created and all of that groups
/// message will be forwarded to user reciever
/// and then user can listen to its own user reciever and recieve message from all joind groups
pub struct GroupsManager<T> {
    inner: Mutex<HashMap<String, Group<T>>>,
    users_group: Mutex<HashMap<String, Vec<UserTask>>>,
    user_reciever: Mutex<HashMap<String, broadcast::Sender<T>>>,
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

struct UserTask {
    group_name: String,
    task: JoinHandle<()>,
}

impl<T> Group<T>
where
    T: Clone + Send + 'static,
{
    /// creates new group with a given name
    /// capacity is the underlying channel capacity and its default is 100
    pub fn new(name: String, capacity: Option<usize>) -> Group<T> {
        let (tx, _rx) = broadcast::channel(capacity.unwrap_or(100));

        Group {
            name,
            tx,
            inner_user: Mutex::new(vec![]),
            user_count: AtomicU32::new(0),
        }
    }

    /// join the groups with a unique user
    /// if user has joined before, it just returns the sender
    pub async fn join(&self, user: String) -> broadcast::Sender<T> {
        let mut inner = self.inner_user.lock().await;

        if !inner.contains(&user) {
            inner.push(user);

            self.user_count.fetch_add(1, Ordering::SeqCst);
        }

        self.tx.clone()
    }

    /// leave the group with user
    /// if user has left before it wont do anything
    pub async fn leave(&self, user: String) {
        let mut inner = self.inner_user.lock().await;

        if let Some(pos) = inner.iter().position(|x| *x == user) {
            inner.swap_remove(pos);

            self.user_count.fetch_sub(1, Ordering::SeqCst);
        }
    }

    /// this method will join the user and return a reciever
    pub async fn recieve(&self, user: String) -> broadcast::Receiver<T> {
        self.join(user).await.subscribe()
    }

    /// check if user is in the group
    pub async fn contains_user(&self, user: &String) -> bool {
        let inner = self.inner_user.lock().await;

        inner.contains(user)
    }

    /// checks if group is empty
    pub fn is_empty(&self) -> bool {
        self.user_count.load(Ordering::SeqCst) == 0
    }

    /// get sender without joining group
    pub fn get_sender(&self) -> broadcast::Sender<T> {
        self.tx.clone()
    }

    ///send message to group
    pub fn send(&self, data: T) -> Result<usize, broadcast::error::SendError<T>> {
        self.tx.send(data)
    }

    /// this method locks on user and give it to you
    /// pls drop it when you dont need it
    pub async fn users(&self) -> tokio::sync::MutexGuard<Vec<String>> {
        self.inner_user.lock().await
    }

    /// get user count of group
    pub async fn user_count(&self) -> u32 {
        self.user_count.load(Ordering::SeqCst)
    }
}

impl<T> GroupsManager<T>
where
    T: Clone + Send + 'static,
{
    pub fn new() -> Self {
        GroupsManager {
            inner: Mutex::new(HashMap::new()),
            users_group: Mutex::new(HashMap::new()),
            user_reciever: Mutex::new(HashMap::new()),
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
        user: String,
        group: String,
    ) -> Result<broadcast::Sender<T>, GroupError> {
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
    pub async fn send_message_to_group(&self, name: String, data: T) -> Result<usize, GroupError> {
        let groups = self.inner.lock().await;

        groups
            .get(&name)
            .ok_or(GroupError::GroupNotFound)?
            .send(data)
            .map_err(|_| GroupError::MessageSendFail)
    }

    /// call this at first of your code to initialize user notifyer
    pub async fn init_user(&self, user: String, capacity: Option<usize>) {
        let mut user_reciever = self.user_reciever.lock().await;

        match user_reciever.entry(user) {
            std::collections::hash_map::Entry::Occupied(_) => {}
            std::collections::hash_map::Entry::Vacant(v) => {
                let (tx, _rx) = broadcast::channel(capacity.unwrap_or(100));
                v.insert(tx);
            }
        }
    }

    /// call this at end of your code to remove user from all groups
    pub async fn end_user(&self, user: String) {
        let groups = self.inner.lock().await;
        let mut user_group = self.users_group.lock().await;
        let mut user_reciever = self.user_reciever.lock().await;

        match user_group.entry(user.clone()) {
            std::collections::hash_map::Entry::Occupied(o) => {
                let user_groups = o.get();

                for task in user_groups {
                    let group = groups.get(&task.group_name);

                    if let Some(group) = group {
                        group.leave(user.clone()).await;
                    }

                    task.task.abort();
                }

                o.remove();
            }
            std::collections::hash_map::Entry::Vacant(_) => {}
        }

        match user_reciever.entry(user.clone()) {
            std::collections::hash_map::Entry::Occupied(o) => {
                o.remove();
            }
            std::collections::hash_map::Entry::Vacant(_) => {}
        }
    }

    /// join user to group
    pub async fn join_group(
        &self,
        name: String,
        user: String,
    ) -> Result<broadcast::Sender<T>, GroupError> {
        let groups = self.inner.lock().await;
        let mut users = self.users_group.lock().await;
        let user_reciever = self.user_reciever.lock().await;

        let sender = groups
            .get(&name)
            .ok_or(GroupError::GroupNotFound)?
            .join(user.clone())
            .await;

        let user_reciever = user_reciever
            .get(&user)
            .ok_or(GroupError::NotInitiated)?
            .clone();

        let mut task_recv = sender.subscribe();

        let task = tokio::spawn(async move {
            while let Ok(data) = task_recv.recv().await {
                let _ = user_reciever.send(data);
            }
        });

        match users.entry(user.clone()) {
            std::collections::hash_map::Entry::Occupied(mut o) => {
                let groups = o.get_mut();

                let has = groups.iter().any(|x| x.group_name == name);

                if !has {
                    groups.push(UserTask {
                        group_name: name,
                        task,
                    });
                }
            }
            std::collections::hash_map::Entry::Vacant(v) => {
                v.insert(vec![UserTask {
                    group_name: name,
                    task,
                }]);
            }
        };

        Ok(sender)
    }

    pub async fn remove_group(&self, group: String) {
        let mut groups = self.inner.lock().await;
        let mut users = self.users_group.lock().await;

        match groups.entry(group.clone()) {
            std::collections::hash_map::Entry::Vacant(_) => {}
            std::collections::hash_map::Entry::Occupied(el) => {
                for user in el.get().users().await.iter() {
                    if let std::collections::hash_map::Entry::Occupied(mut user_task) =
                        users.entry(user.into())
                    {
                        let vecotr = user_task.get_mut();

                        vecotr.retain(|task| {
                            if task.group_name == group {
                                task.task.abort();
                            }

                            task.group_name != group
                        });
                    }
                }

                el.remove();
            }
        }
    }

    pub async fn leave_group(&self, name: String, user: String) -> Result<(), GroupError> {
        let groups = self.inner.lock().await;
        let mut users = self.users_group.lock().await;

        groups
            .get(&name)
            .ok_or(GroupError::GroupNotFound)?
            .leave(user.clone())
            .await;

        match users.entry(user.clone()) {
            std::collections::hash_map::Entry::Occupied(mut o) => {
                let vecotr = o.get_mut();

                vecotr.retain(|task| {
                    if task.group_name == name {
                        task.task.abort();
                    }

                    task.group_name != name
                });
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
        name: String,
    ) -> Result<broadcast::Receiver<T>, GroupError> {
        let rx = self.user_reciever.lock().await;

        let rx = rx.get(&name).ok_or(GroupError::NotInitiated)?.subscribe();

        Ok(rx)
    }
}

impl<T> Default for GroupsManager<T>
where
    T: Clone + Send + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}
