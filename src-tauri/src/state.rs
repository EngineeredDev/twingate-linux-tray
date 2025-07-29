use crate::models::Network;
use std::time::{Duration, Instant};

/// Service connection status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceStatus {
    /// Service is not running
    NotRunning,
    /// Service is connected and authenticated
    Connected,
    /// Service is authenticating with an auth URL
    Authenticating(String),
}

impl Default for ServiceStatus {
    fn default() -> Self {
        Self::NotRunning
    }
}

/// Application state with proper async synchronization
#[derive(Debug, Default)]
pub struct AppState {
    /// Current network data, if available
    network: Option<Network>,
    /// Current service status
    service_status: ServiceStatus,
    /// Timestamp of last successful data update
    last_update: Option<Instant>,
    /// Whether a refresh operation is currently in progress
    refreshing: bool,
}

impl AppState {
    pub fn new() -> Self {
        Self::default()
    }
    
    // Network data access
    pub fn network(&self) -> Option<&Network> {
        self.network.as_ref()
    }
    
    // Service status access
    pub fn service_status(&self) -> &ServiceStatus {
        &self.service_status
    }
    
    pub fn auth_url(&self) -> Option<&str> {
        match &self.service_status {
            ServiceStatus::Authenticating(url) => Some(url),
            _ => None,
        }
    }
    
    
    // State update methods
    pub fn update_network(&mut self, network: Option<Network>) {
        let has_data = network.is_some();
        self.network = network;
        self.service_status = if has_data {
            ServiceStatus::Connected
        } else {
            ServiceStatus::NotRunning
        };
        self.last_update = Some(Instant::now());
        self.refreshing = false;
    }
    
    pub fn set_authenticating(&mut self, auth_url: String) {
        self.service_status = ServiceStatus::Authenticating(auth_url);
        self.network = None;
        self.last_update = Some(Instant::now());
        self.refreshing = false;
    }
    
    
    // Cache management
    pub fn is_stale(&self, threshold: Duration) -> bool {
        match self.last_update {
            Some(last) => last.elapsed() > threshold,
            None => true,
        }
    }
    
    pub fn should_refresh(&self, threshold: Duration) -> bool {
        !self.refreshing && self.is_stale(threshold)
    }
    
}

