use axum_extra::extract::CookieJar;
use crypto_bigint::U256;
use std::collections::HashMap;
use tokio::sync::RwLock;

// Example session store (use Arc<RwLock<...>> for async safety).
lazy_static::lazy_static! {
    pub static ref SESSION_STORE: RwLock<HashMap<U256, UserSession>> = RwLock::new(HashMap::new());
}

use axum::http::StatusCode;
use axum::{async_trait, extract::FromRequestParts, http::request::Parts};

pub type SessionID = U256;

use super::NormalTokens;

#[derive(Clone)]
pub struct UserSession {
    pub id: SessionID,
    pub email: String,
    pub tokens: NormalTokens,
    pub given_name: String,
    pub family_name: String,
}

#[async_trait]
impl<S> FromRequestParts<S> for UserSession
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        // Extract the session cookie and look up the user session in your session store (infallible)
        let jar = CookieJar::from_request_parts(parts, state).await.unwrap();

        let no = |msg: &'static str| (StatusCode::UNAUTHORIZED, msg);
        let session_id = jar.get("session_id").ok_or(no("No session ID"))?;
        let session_id = hex::decode(session_id.value()).map_err(|_| no("Invalid session ID"))?;
        if session_id.len() != U256::BYTES {
            return Err(no("Invalid session ID length"));
        }
        let session_id = U256::from_be_slice(&session_id);
        let session = SESSION_STORE
            .read()
            .await
            .get(&session_id)
            .cloned()
            .ok_or(no("Session not found"))?;
        Ok(session)
    }
}
