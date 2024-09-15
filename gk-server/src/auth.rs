use async_trait::async_trait;
use rand::random;
use zerocopy::AsBytes;

use anyhow::Result;
use axum::http::request::Parts;
use axum::{extract::FromRequestParts, http::StatusCode};
use sha2::Digest;

pub struct ServicePrincipal;

#[async_trait]
impl<S> FromRequestParts<S> for ServicePrincipal
where
    S: Send + Sync,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        let auth = parts
            .headers
            .get("Authorization")
            .ok_or(StatusCode::UNAUTHORIZED)?;
        let mut hasher = sha2::Sha256::new();
        let now = chrono::Utc::now().timestamp_micros();
        let salt: [u8; 32] = random();
        hasher.update(auth.as_bytes());
        let their_pre_hash = hasher.finalize();
        hasher = sha2::Sha256::new();
        hasher.update(now.as_bytes());
        hasher.update(salt);
        hasher.update(their_pre_hash);
        let their_hash = hasher.finalize();

        let mut hasher = sha2::Sha256::new();
        let secret_hex = dotenvy::var("AUTH_SECRET").unwrap();
        let secret = hex::decode(secret_hex).unwrap();
        hasher.update(now.as_bytes());
        hasher.update(salt);
        hasher.update(secret);
        let our_hash = hasher.finalize();

        if their_hash == our_hash {
            Ok(ServicePrincipal)
        } else {
            Err(StatusCode::UNAUTHORIZED)
        }
    }
}
