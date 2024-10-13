use axum::{
    extract::State,
    response::{IntoResponse, Redirect},
};
use axum_extra::extract::{
    cookie::{Cookie, SameSite},
    CookieJar,
};

use crate::errors::WebError;

use super::{OAuthQuery, OauthClient};

pub async fn login(State(oauth): State<OauthClient>) -> Result<Redirect, WebError> {
    let auth_url = oauth.authorize().await?;
    Ok(Redirect::to(auth_url.as_str()))
}

pub async fn oauth_callback(
    State(oauth): State<OauthClient>,
    query: axum::extract::Query<OAuthQuery>, // Extract the authorization code and state
    mut jar: CookieJar,                      // Extract the cookie jar
) -> Result<(CookieJar, Redirect), WebError> {
    let session = oauth.trade_for_session(query.0).await?;
    let cookie = Cookie::build(("session_id", session.id.to_string()))
        .http_only(true)
        .path("/")
        .secure(true);
    jar = jar.add(cookie);
    // Redirect to the home page or some post-login page.
    Ok((jar, Redirect::to("/")))
}

pub async fn logout(mut jar: CookieJar) -> impl IntoResponse {
    // Clear the session/cookie to log the user out
    jar = jar.remove("session_id");
    (jar, Redirect::to("/"))
}
