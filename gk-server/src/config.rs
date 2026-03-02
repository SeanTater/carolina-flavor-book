use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub auth: AuthConfig,
}

impl Config {
    /// Load the configuration from a TOML file.
    pub fn load(path: &str) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        let config = toml::from_str(&text)?;
        Ok(config)
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct ServerConfig {
    pub address: String,
    pub tls: Option<TLSConfig>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct TLSConfig {
    pub cert_path: String,
    pub key_path: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DatabaseConfig {
    pub path: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AuthConfig {
    pub service_principal_secret: String,
    pub session_storage_path: String,
    pub users: Vec<UserCredential>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct UserCredential {
    pub username: String,
    pub password_hash: String,
}
