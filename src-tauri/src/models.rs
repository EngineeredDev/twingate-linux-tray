use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Network {
    #[allow(dead_code)]
    pub admin_url: String,
    #[allow(dead_code)]
    pub full_tunnel_time_limit: u64,
    pub internet_security: InternetSecurity,
    pub resources: Vec<Resource>,
    pub user: User,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InternetSecurity {
    pub mode: i32,
    #[allow(dead_code)]
    pub status: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Resource {
    pub address: String,
    #[allow(dead_code)]
    pub admin_url: String,
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(default)]
    pub aliases: Vec<Alias>,
    pub auth_expires_at: i64,
    #[allow(dead_code)]
    pub auth_flow_id: String,
    #[allow(dead_code)]
    pub auth_state: String,
    pub can_open_in_browser: bool,
    pub client_visibility: i32,
    pub id: String,
    pub name: String,
    #[allow(dead_code)]
    pub open_url: String,
    #[allow(dead_code)]
    #[serde(rename = "type")]
    pub resource_type: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Alias {
    #[allow(dead_code)]
    pub address: String,
    pub open_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct User {
    #[allow(dead_code)]
    pub avatar_url: String,
    pub email: String,
    #[allow(dead_code)]
    pub first_name: String,
    #[allow(dead_code)]
    pub id: String,
    #[allow(dead_code)]
    pub is_admin: bool,
    #[allow(dead_code)]
    pub last_name: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_user_deserialization() {
        let json = r#"{
            "avatar_url": "https://example.com/avatar.png",
            "email": "user@example.com",
            "first_name": "John",
            "id": "user-123",
            "is_admin": true,
            "last_name": "Doe"
        }"#;

        let user: User = serde_json::from_str(json).unwrap();
        assert_eq!(user.email, "user@example.com");
        assert_eq!(user.first_name, "John");
        assert_eq!(user.last_name, "Doe");
        assert_eq!(user.id, "user-123");
        assert!(user.is_admin);
        assert_eq!(user.avatar_url, "https://example.com/avatar.png");
    }

    #[test]
    fn test_alias_deserialization() {
        let json = r#"{
            "address": "app.internal",
            "open_url": "https://app.internal"
        }"#;

        let alias: Alias = serde_json::from_str(json).unwrap();
        assert_eq!(alias.address, "app.internal");
        assert_eq!(alias.open_url, "https://app.internal");
    }

    #[test]
    fn test_internet_security_deserialization() {
        let json = r#"{
            "mode": 1,
            "status": 2
        }"#;

        let security: InternetSecurity = serde_json::from_str(json).unwrap();
        assert_eq!(security.mode, 1);
        assert_eq!(security.status, 2);
    }

    #[test]
    fn test_resource_deserialization_full() {
        let json = r#"{
            "address": "192.168.1.100",
            "admin_url": "https://admin.twingate.com/resource/123",
            "alias": "my-server",
            "aliases": [
                {
                    "address": "server.internal",
                    "open_url": "https://server.internal"
                }
            ],
            "auth_expires_at": 1640995200,
            "auth_flow_id": "flow-123",
            "auth_state": "authenticated",
            "can_open_in_browser": true,
            "client_visibility": 1,
            "id": "resource-123",
            "name": "My Server",
            "open_url": "https://server.internal",
            "type": "tcp"
        }"#;

        let resource: Resource = serde_json::from_str(json).unwrap();
        assert_eq!(resource.address, "192.168.1.100");
        assert_eq!(resource.alias, Some("my-server".to_string()));
        assert_eq!(resource.aliases.len(), 1);
        assert_eq!(resource.aliases[0].open_url, "https://server.internal");
        assert_eq!(resource.auth_expires_at, 1640995200);
        assert!(resource.can_open_in_browser);
        assert_eq!(resource.client_visibility, 1);
        assert_eq!(resource.id, "resource-123");
        assert_eq!(resource.name, "My Server");
        assert_eq!(resource.resource_type, "tcp");
    }

    #[test]
    fn test_resource_deserialization_minimal() {
        let json = r#"{
            "address": "192.168.1.100",
            "admin_url": "https://admin.twingate.com/resource/123",
            "auth_expires_at": 1640995200,
            "auth_flow_id": "flow-123",
            "auth_state": "authenticated",
            "can_open_in_browser": false,
            "client_visibility": 1,
            "id": "resource-123",
            "name": "My Server",
            "open_url": "",
            "type": "tcp"
        }"#;

        let resource: Resource = serde_json::from_str(json).unwrap();
        assert_eq!(resource.address, "192.168.1.100");
        assert_eq!(resource.alias, None);
        assert_eq!(resource.aliases.len(), 0);
        assert!(!resource.can_open_in_browser);
        assert_eq!(resource.id, "resource-123");
        assert_eq!(resource.name, "My Server");
    }

    #[test]
    fn test_network_deserialization() {
        let json = r#"{
            "admin_url": "https://admin.twingate.com",
            "full_tunnel_time_limit": 3600,
            "internet_security": {
                "mode": 1,
                "status": 2
            },
            "resources": [
                {
                    "address": "192.168.1.100",
                    "admin_url": "https://admin.twingate.com/resource/123",
                    "auth_expires_at": 1640995200,
                    "auth_flow_id": "flow-123",
                    "auth_state": "authenticated",
                    "can_open_in_browser": false,
                    "client_visibility": 1,
                    "id": "resource-123",
                    "name": "My Server",
                    "open_url": "",
                    "type": "tcp"
                }
            ],
            "user": {
                "avatar_url": "https://example.com/avatar.png",
                "email": "user@example.com",
                "first_name": "John",
                "id": "user-123",
                "is_admin": true,
                "last_name": "Doe"
            }
        }"#;

        let network: Network = serde_json::from_str(json).unwrap();
        assert_eq!(network.admin_url, "https://admin.twingate.com");
        assert_eq!(network.full_tunnel_time_limit, 3600);
        assert_eq!(network.internet_security.mode, 1);
        assert_eq!(network.resources.len(), 1);
        assert_eq!(network.resources[0].name, "My Server");
        assert_eq!(network.user.email, "user@example.com");
    }

    #[test]
    fn test_invalid_json_deserialization() {
        let invalid_json = r#"{"invalid": "json"}"#;
        
        let result: Result<Network, _> = serde_json::from_str(invalid_json);
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_required_fields() {
        let json_missing_email = r#"{
            "avatar_url": "https://example.com/avatar.png",
            "first_name": "John",
            "id": "user-123",
            "is_admin": true,
            "last_name": "Doe"
        }"#;

        let result: Result<User, _> = serde_json::from_str(json_missing_email);
        assert!(result.is_err());
    }

    #[test]
    fn test_wrong_field_types() {
        let json_wrong_type = r#"{
            "avatar_url": "https://example.com/avatar.png",
            "email": "user@example.com",
            "first_name": "John",
            "id": "user-123",
            "is_admin": "not_a_boolean",
            "last_name": "Doe"
        }"#;

        let result: Result<User, _> = serde_json::from_str(json_wrong_type);
        assert!(result.is_err());
    }
}
