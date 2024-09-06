use std::{
    collections::VecDeque,
    sync::{Arc, RwLock},
};

use anyhow::Result;
use oauth2::{
    basic::{BasicClient, BasicTokenType},
    reqwest::async_http_client,
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, EmptyExtraTokenFields,
    PkceCodeChallenge, RedirectUrl, RevocationUrl, Scope, TokenUrl,
};

use crate::errors::WebError;

pub type NormalTokens = oauth2::StandardTokenResponse<EmptyExtraTokenFields, BasicTokenType>;

#[derive(Clone)]
pub struct OauthClient {
    client: BasicClient,
    // This only works for a single server deployment. We'd need to put this in a database or cache
    // to make it work for multiple servers.
    open_auth_attempts: Arc<RwLock<VecDeque<CsrfToken>>>,
}

impl OauthClient {
    pub fn new_from_env() -> Result<Self> {
        dotenvy::dotenv()?;

        // Create an OAuth2 client by specifying the client ID, client secret, authorization URL and
        // token URL.
        let client = BasicClient::new(
            ClientId::new(dotenvy::var("OAUTH2_CLIENT_ID")?),
            Some(ClientSecret::new(dotenvy::var("OAUTH2_CLIENT_SECRET")?)),
            AuthUrl::new("https://accounts.google.com/o/oauth2/v2/auth".into())?,
            Some(TokenUrl::new(
                "https://www.googleapis.com/oauth2/v3/token".into(),
            )?),
        )
        // Set the URL the user will be redirected to after the authorization process.
        .set_redirect_uri(RedirectUrl::new(
            "https://gallagher.kitchen/login/return".into(),
        )?)
        .set_revocation_uri(RevocationUrl::new(
            "https://oauth2.googleapis.com/revoke".into(),
        )?);

        Ok(Self {
            client,
            open_auth_attempts: Arc::new(RwLock::new(VecDeque::with_capacity(100))),
        })
    }

    pub fn authorize(&self) -> Result<oauth2::url::Url> {
        // Generate a PKCE challenge.
        let (pkce_challenge, _pkce_verifier) = PkceCodeChallenge::new_random_sha256();

        // Generate the full authorization URL.
        let (auth_url, csrf_token) = self
            .client
            .authorize_url(CsrfToken::new_random)
            // Set the desired scopes. We just want to know your email so we can verify your identity.
            .add_scope(Scope::new(
                "https://www.googleapis.com/auth/userinfo.email".to_string(),
            ))
            .add_scope(Scope::new(
                "https://www.googleapis.com/auth/userinfo.profile".to_string(),
            ))
            // Set the PKCE code challenge.
            .set_pkce_challenge(pkce_challenge)
            .url();

        let mut opens = self.open_auth_attempts.write().unwrap();
        if opens.len() == 100 {
            // We don't want to keep too many open attempts.
            opens.pop_back();
        }
        opens.push_front(csrf_token.clone());

        Ok(auth_url)
    }

    pub async fn trade_for_tokens(&self, query: OAuthQuery) -> Result<NormalTokens, WebError> {
        // Exchange the code with a token.
        {
            let mut opens = self.open_auth_attempts.write().unwrap();
            if let Some(position) = opens.iter().position(|t| t.secret() == &query.state) {
                // We're good. But remove it now. Shift instead of swap_remove to keep the order.
                opens.remove(position);
            } else {
                return Err(WebError::AuthFailure("Invalid CSRF token.".into()));
            }
        }
        let token = self
            .client
            .exchange_code(AuthorizationCode::new(query.code))
            .request_async(async_http_client)
            .await
            .map_err(|e| WebError::AuthFailure(e.to_string()))?;
        Ok(token)
    }
}

#[derive(serde::Deserialize)]
pub struct OAuthQuery {
    pub code: String,
    pub state: String,
}
