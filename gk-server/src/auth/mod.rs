use std::sync::Arc;

pub mod route;
pub mod session;

use crate::config::{AuthConfig, UserCredential};
use session::{Sessions, UserSession, SessionID};

use crypto_bigint::Random;

#[derive(Clone)]
pub struct AuthService {
    users: Vec<UserCredential>,
    pub sessions: Arc<Sessions>,
    pub service_principal_secret: String,
}

impl AuthService {
    pub async fn new_from_config(conf: &AuthConfig) -> anyhow::Result<Self> {
        let sessions = Sessions::from_config(conf).await;
        Ok(Self {
            users: conf.users.clone(),
            sessions,
            service_principal_secret: conf.service_principal_secret.clone(),
        })
    }

    /// Verify username/password and return a new session if valid.
    pub fn verify_password(&self, username: &str, password: &str) -> Option<UserSession> {
        let user = self.users.iter().find(|u| u.username == username)?;
        if bcrypt::verify(password, &user.password_hash).ok()? {
            let id = SessionID::random(&mut rand::thread_rng());
            Some(UserSession {
                id,
                username: username.to_string(),
            })
        } else {
            None
        }
    }
}
