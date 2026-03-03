use axum::extract::FromRef;
use axum_extra::extract::CookieJar;
use crypto_bigint::{subtle::ConstantTimeEq, U256};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::{extract::FromRequestParts, http::request::Parts};

pub type SessionID = U256;

use crate::config::AuthConfig;

use super::AuthService;

#[derive(Clone, Serialize, Deserialize)]
pub struct UserSession {
    pub id: SessionID,
    pub username: String,
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct Sessions {
    pub sessions: DashMap<SessionID, UserSession>,
}

impl Sessions {
    pub async fn from_config(conf: &AuthConfig) -> Arc<Self> {
        let path = conf.session_storage_path.clone();
        let session_text = tokio::fs::read_to_string(&path).await.unwrap_or_default();
        let sessions: Self = serde_json::from_str(&session_text).unwrap_or_default();

        let sessions_ref = Arc::new(sessions);

        let sessions_ref2 = sessions_ref.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(300)).await;
                let sessions_text = serde_json::to_string(&*sessions_ref2).unwrap();
                tokio::fs::write(&path, sessions_text).await.unwrap();
            }
        });

        sessions_ref
    }
}

impl<S> FromRequestParts<S> for UserSession
where
    AuthService: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_request_parts(parts, state).await.unwrap();

        let no = |msg: &'static str| (StatusCode::UNAUTHORIZED, msg);
        let session_id = jar.get("session_id").ok_or(no("No session ID"))?;
        let session_id = hex::decode(session_id.value()).map_err(|_| no("Invalid session ID"))?;
        if session_id.len() != U256::BYTES {
            return Err(no("Invalid session ID length"));
        }
        let session_id = U256::from_be_slice(&session_id);
        let client = AuthService::from_ref(state);
        let session = client
            .sessions
            .sessions
            .get(&session_id)
            .ok_or(no("Session not found"))?
            .value()
            .clone();
        Ok(session)
    }
}

/// Service principal authentication via Bearer token
pub struct ServicePrincipal;

impl<S> FromRequestParts<S> for ServicePrincipal
where
    AuthService: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let no = |msg: &'static str| (StatusCode::UNAUTHORIZED, msg);

        let auth_header = parts
            .headers
            .get("Authorization")
            .ok_or(no("No Authorization header"))?;

        let auth_str = auth_header
            .to_str()
            .map_err(|_| no("Invalid Authorization header"))?;

        let token = auth_str
            .strip_prefix("Bearer ")
            .ok_or(no("Authorization must be Bearer token"))?;

        let client = AuthService::from_ref(state);
        let expected = &client.service_principal_secret;

        if token.as_bytes().ct_eq(expected.as_bytes()).into() {
            Ok(ServicePrincipal)
        } else {
            Err(no("Invalid service principal secret"))
        }
    }
}

/// Authenticated user - either via session cookie or service principal token
pub enum AuthenticatedUser {
    Session(UserSession),
    ServicePrincipal,
}

impl<S> FromRequestParts<S> for AuthenticatedUser
where
    AuthService: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AuthRejection;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        if let Ok(session) = UserSession::from_request_parts(parts, state).await {
            return Ok(Self::Session(session));
        }

        if let Ok(_principal) = ServicePrincipal::from_request_parts(parts, state).await {
            return Ok(Self::ServicePrincipal);
        }

        Err(AuthRejection)
    }
}

pub struct AuthRejection;

impl IntoResponse for AuthRejection {
    fn into_response(self) -> Response {
        let html = crate::TEMPLATES
            .get_template("login-required.html.jinja")
            .and_then(|t| t.render(minijinja::context! {}));
        match html {
            Ok(body) => (StatusCode::UNAUTHORIZED, Html(body)).into_response(),
            Err(_) => (StatusCode::UNAUTHORIZED, "Authentication required").into_response(),
        }
    }
}
