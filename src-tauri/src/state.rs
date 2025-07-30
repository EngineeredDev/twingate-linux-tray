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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Network, User, InternetSecurity};
    use std::time::Duration;

    fn create_test_network() -> Network {
        Network {
            admin_url: "https://admin.twingate.com".to_string(),
            full_tunnel_time_limit: 3600,
            internet_security: InternetSecurity {
                mode: 1,
                status: 2,
            },
            resources: vec![],
            user: User {
                avatar_url: "https://example.com/avatar.png".to_string(),
                email: "test@example.com".to_string(),
                first_name: "Test".to_string(),
                id: "user-123".to_string(),
                is_admin: false,
                last_name: "User".to_string(),
            },
        }
    }

    #[test]
    fn test_service_status_default() {
        let status = ServiceStatus::default();
        assert_eq!(status, ServiceStatus::NotRunning);
    }

    #[test]
    fn test_service_status_equality() {
        assert_eq!(ServiceStatus::NotRunning, ServiceStatus::NotRunning);
        assert_eq!(ServiceStatus::Connected, ServiceStatus::Connected);
        assert_eq!(
            ServiceStatus::Authenticating("url".to_string()),
            ServiceStatus::Authenticating("url".to_string())
        );
        
        assert_ne!(ServiceStatus::NotRunning, ServiceStatus::Connected);
        assert_ne!(
            ServiceStatus::Authenticating("url1".to_string()),
            ServiceStatus::Authenticating("url2".to_string())
        );
    }

    #[test]
    fn test_app_state_new() {
        let state = AppState::new();
        assert!(state.network().is_none());
        assert_eq!(state.service_status(), &ServiceStatus::NotRunning);
        assert!(state.auth_url().is_none());
        assert!(state.last_update.is_none());
        assert!(!state.refreshing);
    }

    #[test]
    fn test_app_state_default() {
        let state = AppState::default();
        assert!(state.network().is_none());
        assert_eq!(state.service_status(), &ServiceStatus::NotRunning);
        assert!(state.auth_url().is_none());
    }

    #[test]
    fn test_update_network_with_data() {
        let mut state = AppState::new();
        let network = create_test_network();
        
        state.update_network(Some(network.clone()));
        
        assert!(state.network().is_some());
        assert_eq!(state.network().unwrap().user.email, "test@example.com");
        assert_eq!(state.service_status(), &ServiceStatus::Connected);
        assert!(state.last_update.is_some());
        assert!(!state.refreshing);
    }

    #[test]
    fn test_update_network_with_none() {
        let mut state = AppState::new();
        
        state.update_network(None);
        
        assert!(state.network().is_none());
        assert_eq!(state.service_status(), &ServiceStatus::NotRunning);
        assert!(state.last_update.is_some());
        assert!(!state.refreshing);
    }

    #[test]
    fn test_set_authenticating() {
        let mut state = AppState::new();
        let auth_url = "https://auth.example.com".to_string();
        
        state.set_authenticating(auth_url.clone());
        
        assert!(state.network().is_none());
        assert_eq!(
            state.service_status(),
            &ServiceStatus::Authenticating(auth_url.clone())
        );
        assert_eq!(state.auth_url(), Some(auth_url.as_str()));
        assert!(state.last_update.is_some());
        assert!(!state.refreshing);
    }

    #[test]
    fn test_auth_url_when_not_authenticating() {
        let mut state = AppState::new();
        let network = create_test_network();
        
        state.update_network(Some(network));
        
        assert_eq!(state.service_status(), &ServiceStatus::Connected);
        assert!(state.auth_url().is_none());
    }

    #[test]
    fn test_is_stale_with_no_update() {
        let state = AppState::new();
        let threshold = Duration::from_secs(30);
        
        assert!(state.is_stale(threshold));
    }

    #[test]
    fn test_is_stale_with_recent_update() {
        let mut state = AppState::new();
        state.update_network(None); // This sets last_update to now
        
        let threshold = Duration::from_secs(30);
        assert!(!state.is_stale(threshold));
    }

    #[test]
    fn test_should_refresh_when_not_refreshing_and_stale() {
        let state = AppState::new(); // No last_update, so it's stale
        let threshold = Duration::from_secs(30);
        
        assert!(state.should_refresh(threshold));
    }

    #[test]
    fn test_should_refresh_when_refreshing() {
        let mut state = AppState::new();
        state.refreshing = true;
        let threshold = Duration::from_secs(30);
        
        assert!(!state.should_refresh(threshold));
    }

    #[test]
    fn test_should_refresh_when_not_stale() {
        let mut state = AppState::new();
        state.update_network(None); // Sets last_update to now
        let threshold = Duration::from_secs(30);
        
        assert!(!state.should_refresh(threshold));
    }

    #[test]
    fn test_state_transitions() {
        let mut state = AppState::new();
        
        // Initial state
        assert_eq!(state.service_status(), &ServiceStatus::NotRunning);
        
        // Start authenticating
        state.set_authenticating("https://auth.example.com".to_string());
        assert!(matches!(state.service_status(), ServiceStatus::Authenticating(_)));
        assert!(state.network().is_none());
        
        // Complete authentication with network data
        let network = create_test_network();
        state.update_network(Some(network));
        assert_eq!(state.service_status(), &ServiceStatus::Connected);
        assert!(state.network().is_some());
        
        // Service stops
        state.update_network(None);
        assert_eq!(state.service_status(), &ServiceStatus::NotRunning);
        assert!(state.network().is_none());
    }

    #[test]
    fn test_concurrent_access_safety() {
        // This test verifies the state structure works well with Mutex
        use std::sync::{Arc, Mutex};
        use std::thread;
        
        let state = Arc::new(Mutex::new(AppState::new()));
        let state_clone = Arc::clone(&state);
        
        let handle = thread::spawn(move || {
            let mut guard = state_clone.lock().unwrap();
            guard.set_authenticating("https://test.com".to_string());
        });
        
        handle.join().unwrap();
        
        let guard = state.lock().unwrap();
        assert!(matches!(guard.service_status(), ServiceStatus::Authenticating(_)));
    }

    #[test]
    fn test_debug_format() {
        let state = AppState::new();
        let debug_str = format!("{:?}", state);
        assert!(debug_str.contains("AppState"));
        assert!(debug_str.contains("NotRunning"));
    }

    #[test]
    fn test_service_status_debug_format() {
        let status = ServiceStatus::NotRunning;
        assert_eq!(format!("{:?}", status), "NotRunning");
        
        let status = ServiceStatus::Connected;
        assert_eq!(format!("{:?}", status), "Connected");
        
        let status = ServiceStatus::Authenticating("test".to_string());
        assert_eq!(format!("{:?}", status), "Authenticating(\"test\")");
    }

    #[test]
    fn test_service_status_clone() {
        let status = ServiceStatus::Authenticating("test".to_string());
        let cloned = status.clone();
        assert_eq!(status, cloned);
    }
}

