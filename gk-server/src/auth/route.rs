use axum::{
    extract::State,
    response::{Html, IntoResponse, Redirect},
    Form,
};
use axum_extra::extract::{cookie::Cookie, CookieJar};
use serde::Deserialize;

use crypto_bigint::Encoding;

use super::AuthService;

#[derive(Deserialize)]
pub struct LoginForm {
    username: String,
    password: String,
}

pub async fn login_page() -> Html<&'static str> {
    Html(include_str!("../../templates/login.html.jinja"))
}

pub async fn login_submit(
    State(auth): State<AuthService>,
    mut jar: CookieJar,
    Form(form): Form<LoginForm>,
) -> Result<(CookieJar, Redirect), impl IntoResponse> {
    match auth.verify_password(&form.username, &form.password) {
        Some(session) => {
            auth.sessions.sessions.insert(session.id, session.clone());
            let cookie = Cookie::build(("session_id", hex::encode(session.id.to_be_bytes())))
                .http_only(true)
                .path("/");
            jar = jar.add(cookie);
            Ok((jar, Redirect::to("/")))
        }
        None => Err(Html(
            "<html><body><h1>Login failed</h1><p>Invalid username or password.</p><a href=\"/auth/login\">Try again</a></body></html>",
        )),
    }
}

pub async fn logout(mut jar: CookieJar) -> impl IntoResponse {
    jar = jar.remove("session_id");
    (jar, Redirect::to("/"))
}
