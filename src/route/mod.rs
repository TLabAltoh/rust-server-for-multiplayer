use crate::config::Config;

pub mod room;
pub mod rtc;
pub mod r#static;
pub mod ws;

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
}
