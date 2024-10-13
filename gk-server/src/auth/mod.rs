use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use oauth2::basic::{
    BasicErrorResponse, BasicRevocationErrorResponse, BasicTokenIntrospectionResponse,
};
use oauth2::{PkceCodeVerifier, StandardRevocableToken, StandardTokenResponse};

use tokio::sync::RwLock;

use std::sync::Arc;

use crypto_bigint::Random;
use oauth2::{
    basic::BasicTokenType, reqwest::async_http_client, AuthUrl, AuthorizationCode, ClientId,
    ClientSecret, CsrfToken, PkceCodeChallenge, RedirectUrl, RevocationUrl, Scope, TokenUrl,
};
use session::{SessionID, UserSession};

pub mod route;
pub mod session;

pub type AuthResult<X> = Result<X, AuthError>;

use crate::config::AuthConfig;
use crate::errors::WebError;

pub type NormalTokens = oauth2::StandardTokenResponse<GoogleTokenFields, BasicTokenType>;

pub struct ExpiringCSRFToken {
    token: CsrfToken,
    pkce_verifier: PkceCodeVerifier,
    expires: chrono::DateTime<chrono::Utc>,
}

#[derive(thiserror::Error, Debug)]
pub enum AuthError {
    #[error("Invalid CSRF token: {0}")]
    CSRF(&'static str),
    #[error("Session error: {0}")]
    Session(&'static str),
    #[error("JWT error: {0}")]
    JWTError(#[from] jsonwebtoken::errors::Error),
    #[error("JWK refresh request error: {0}")]
    JWKError(reqwest::Error),
    #[error("Missing or invalid JWK: {0}")]
    JWKMissing(String),
    #[error("OAuth error: {0}")]
    OauthClientError(Box<dyn std::error::Error + Send + Sync + 'static>),
    #[error("Other OAuth error: {0}")]
    OtherOauthError(&'static str),
}

impl From<AuthError> for WebError {
    fn from(e: AuthError) -> Self {
        match e {
            AuthError::CSRF(msg) => WebError::AuthFailure(msg.into()),
            AuthError::Session(msg) => WebError::AuthFailure(msg.into()),
            AuthError::JWTError(e) => WebError::InternalError(e.into()),
            AuthError::JWKError(e) => WebError::AuthFailure(e.to_string()),
            AuthError::JWKMissing(e) => WebError::AuthFailure(e),
            AuthError::OauthClientError(e) => WebError::AuthFailure(e.to_string()),
            AuthError::OtherOauthError(e) => WebError::AuthFailure(e.into()),
        }
    }
}

#[derive(Clone)]
pub struct OauthClient {
    client: oauth2::Client<
        BasicErrorResponse,
        StandardTokenResponse<GoogleTokenFields, BasicTokenType>,
        BasicTokenType,
        BasicTokenIntrospectionResponse,
        StandardRevocableToken,
        BasicRevocationErrorResponse,
    >,
    // This only works for a single server deployment. We'd need to put this in a database or cache
    // to make it work for multiple servers.
    open_auth_attempts: Arc<RwLock<Vec<ExpiringCSRFToken>>>,
    jwks: Arc<RwLock<Jwks>>,
}

#[derive(Clone, Default, Deserialize)]
struct Jwks {
    keys: Vec<Jwk>,
    #[serde(default)]
    last_refreshed: chrono::DateTime<chrono::Utc>,
}

impl Jwks {
    async fn get_key(&mut self, kid: &str) -> AuthResult<DecodingKey> {
        if self.last_refreshed < chrono::Utc::now() - chrono::Duration::hours(1) {
            // Refresh the keys
            let jwks = reqwest::get("https://www.googleapis.com/oauth2/v3/certs")
                .await
                .map_err(AuthError::JWKError)?
                .json::<Jwks>()
                .await
                .map_err(AuthError::JWKError)?;
            // Be proactive and try to decode all the keys before updating the jwks
            // This way, if there's an error, we don't update the keys
            for key in &jwks.keys {
                key.to_rsa()?;
            }
            *self = jwks;
            self.last_refreshed = chrono::Utc::now();
        }
        let encoded_key = self
            .keys
            .iter()
            .find(|k| k.kid == kid)
            .ok_or_else(|| AuthError::JWKMissing(kid.into()))?;
        // This won't fail because we already tested it before updating the jwks
        encoded_key.to_rsa()
    }

    async fn verify_token(&mut self, id_token: &str) -> AuthResult<IdTokenClaims> {
        let header = jsonwebtoken::decode_header(id_token)?;
        let kid = header.kid.ok_or_else(|| {
            AuthError::JWKMissing("Key ID (kid) missing from id token claims".into())
        })?;
        let jwk = self.get_key(&kid).await?;
        let validation = Validation::new(Algorithm::RS256);
        let claims = jsonwebtoken::decode::<IdTokenClaims>(id_token, &jwk, &validation)?;
        Ok(claims.claims)
    }
}

impl OauthClient {
    pub fn new_from_config(conf: &AuthConfig) -> anyhow::Result<Self> {
        // Create an OAuth2 client by specifying the client ID, client secret, authorization URL and
        // token URL.
        let client = oauth2::Client::new(
            ClientId::new(conf.client_id.clone()),
            Some(ClientSecret::new(conf.client_secret.clone())),
            AuthUrl::new("https://accounts.google.com/o/oauth2/v2/auth".into())?,
            Some(TokenUrl::new(
                "https://www.googleapis.com/oauth2/v3/token".into(),
            )?),
        )
        // Set the URL the user will be redirected to after the authorization process.
        .set_redirect_uri(RedirectUrl::new(conf.redirect_uri.clone())?)
        .set_revocation_uri(RevocationUrl::new(
            "https://oauth2.googleapis.com/revoke".into(),
        )?);

        Ok(Self {
            client,
            open_auth_attempts: Default::default(),
            jwks: Default::default(),
        })
    }

    pub async fn authorize(&self) -> AuthResult<oauth2::url::Url> {
        // Generate a PKCE challenge.
        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

        // Generate the full authorization URL.
        let (auth_url, csrf_token) = self
            .client
            .authorize_url(CsrfToken::new_random)
            // Set the desired scopes. We just want to know your email so we can verify your identity.
            .add_scope(Scope::new("openid".to_string()))
            .add_scope(Scope::new("email".to_string()))
            .add_scope(Scope::new("profile".to_string()))
            // Set the PKCE code challenge.
            .set_pkce_challenge(pkce_challenge)
            .url();

        let mut opens = self.open_auth_attempts.write().await;

        // Remove expired tokens, so we don't even consider them
        opens.retain(|t| t.expires > chrono::Utc::now());
        opens.push(ExpiringCSRFToken {
            token: csrf_token.clone(),
            pkce_verifier,
            expires: chrono::Utc::now() + chrono::Duration::minutes(5),
        });

        Ok(auth_url)
    }

    pub async fn trade_for_session(&self, query: OAuthQuery) -> AuthResult<UserSession> {
        // Exchange the code with a token.
        let token = {
            let mut opens = self.open_auth_attempts.write().await;
            let position = opens
                .iter()
                .position(|t| t.token.secret() == &query.state)
                .ok_or(AuthError::CSRF("Invalid CSRF token."))?;
            if opens[position].expires < chrono::Utc::now() {
                return Err(AuthError::CSRF("CSRF token expired."));
            }
            opens.swap_remove(position)
        };
        let tokens = self
            .client
            .exchange_code(AuthorizationCode::new(query.code))
            .set_pkce_verifier(token.pkce_verifier)
            .request_async(async_http_client)
            .await
            .map_err(|e| AuthError::OauthClientError(e.into()))?;

        let id_token = tokens
            .extra_fields()
            .id_token
            .as_ref()
            .ok_or(AuthError::OtherOauthError("No ID token"))?;

        let claims = self.jwks.write().await.verify_token(id_token).await?;

        let id = SessionID::random(&mut rand::thread_rng());

        Ok(UserSession {
            id,
            email: claims.email,
            tokens,
            given_name: claims.given_name.unwrap_or("Julius".into()),
            family_name: claims.family_name.unwrap_or("Caesar".into()),
        })
    }
}

#[derive(serde::Deserialize)]
pub struct OAuthQuery {
    pub code: String,
    pub state: String,
}

use oauth2::ExtraTokenFields;
use serde::{Deserialize, Serialize};

// Define your custom ExtraTokenFields to capture the id_token
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GoogleTokenFields {
    id_token: Option<String>, // Add id_token field
}

// You need to implement `ExtraTokenFields` for your struct
impl ExtraTokenFields for GoogleTokenFields {}

// Define the expected claims in the ID token
// Some of these fields are included, but we don't use them
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct IdTokenClaims {
    email: String,
    given_name: Option<String>,  // First name
    family_name: Option<String>, // Last name
    aud: String,                 // Audience
    exp: usize,                  // Expiration time
    iat: usize,                  // Issued at time
    iss: String,                 // Issuer
    sub: String,                 // Subject (user ID)
}

// Define a structure to hold the keys from Google's JWKS
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Jwk {
    kty: String,  // Key type
    alg: String,  // Algorithm
    use_: String, // Use
    kid: String,  // Key ID
    n: String,    // Modulus
    e: String,    // Exponent
}
impl Jwk {
    fn to_rsa(&self) -> AuthResult<jsonwebtoken::DecodingKey> {
        jsonwebtoken::DecodingKey::from_rsa_components(&self.n, &self.e).map_err(Into::into)
    }
}
