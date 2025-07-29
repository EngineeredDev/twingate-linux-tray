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
