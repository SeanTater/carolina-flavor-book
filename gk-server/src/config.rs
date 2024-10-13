use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub auth: AuthConfig,
}

impl Config {
    /// Load the configuration from a YAML file.
    pub fn load(yml_path: &str) -> anyhow::Result<Self> {
        let yml = std::fs::read_to_string(yml_path)?;
        let config = serde_yaml::from_str(&yml)?;
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
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    pub session_storage_path: String,
    pub audiences: Vec<String>,
}
