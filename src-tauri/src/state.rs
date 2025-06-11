use crate::models::Network;
use std::sync::Mutex;

#[derive(Default)]
pub struct AppState {
    pub network: Option<Network>,
    pub is_service_running: bool,
    pub last_update: Option<std::time::Instant>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            network: None,
            is_service_running: false,
            last_update: None,
        }
    }

    pub fn update_network(&mut self, network: Option<Network>) {
        self.is_service_running = network.is_some();
        self.network = network;
        self.last_update = Some(std::time::Instant::now());
    }

    pub fn get_network(&self) -> Option<&Network> {
        self.network.as_ref()
    }

    pub fn should_refresh(&self, threshold: std::time::Duration) -> bool {
        match self.last_update {
            Some(last) => last.elapsed() > threshold,
            None => true,
        }
    }
}

pub type AppStateType = Mutex<AppState>;
