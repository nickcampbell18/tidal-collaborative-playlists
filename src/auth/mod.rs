pub mod tidal;

use axum::{
    extract::{Query, State},
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::PrivateCookieJar;
use axum_extra::extract::cookie::{Cookie, SameSite};
use oauth2::{
    AuthorizationCode, CsrfToken, PkceCodeChallenge, RefreshToken, Scope, TokenResponse,
    reqwest::async_http_client,
};
use serde::Deserialize;
use time::{Duration, OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{AppState, db, error::AppError};

const COOKIE_OAUTH_STATE: &str = "oauth_state";
const COOKIE_OAUTH_VERIFIER: &str = "oauth_verifier";
const COOKIE_SESSION: &str = "session_user_id";

#[derive(Deserialize)]
pub struct LoginParams {
    return_to: Option<String>,
}

pub async fn login(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Query(params): Query<LoginParams>,
) -> impl IntoResponse {
    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

    // Embed return_to into the state param: "{csrf}|{return_to}"
    // The cookie stores just the csrf secret for validation.
    let csrf = CsrfToken::new_random();
    let state_value = match params.return_to {
        Some(ref url) if !url.is_empty() => format!("{}|{}", csrf.secret(), url),
        _ => csrf.secret().clone(),
    };

    let (auth_url, _) = state
        .oauth_client
        .authorize_url(move || CsrfToken::new(state_value))
        .add_scope(Scope::new("playlists.read".to_string()))
        .add_scope(Scope::new("playlists.write".to_string()))
        .add_scope(Scope::new("user.read".to_string()))
        .set_pkce_challenge(pkce_challenge)
        .url();

    let jar = jar
        .add(short_lived_cookie(
            COOKIE_OAUTH_STATE,
            csrf.secret().clone(),
        ))
        .add(short_lived_cookie(
            COOKIE_OAUTH_VERIFIER,
            pkce_verifier.secret().clone(),
        ));

    (jar, Redirect::to(auth_url.as_str()))
}

#[derive(Deserialize)]
pub struct CallbackParams {
    code: String,
    state: String,
}

pub async fn callback(
    State(state): State<AppState>,
    Query(params): Query<CallbackParams>,
    jar: PrivateCookieJar,
) -> Result<Response, AppError> {
    let stored_state = jar
        .get(COOKIE_OAUTH_STATE)
        .ok_or(AppError::MissingOAuthCookies)?;
    let stored_verifier = jar
        .get(COOKIE_OAUTH_VERIFIER)
        .ok_or(AppError::MissingOAuthCookies)?;

    // State is "{csrf}|{return_to}" or just "{csrf}" when no return_to was set.
    let (csrf_part, redirect_target) = match params.state.split_once('|') {
        Some((csrf, ret)) => (csrf.to_string(), ret.to_string()),
        None => (params.state.clone(), "/".to_string()),
    };

    if stored_state.value() != csrf_part {
        return Err(AppError::OAuthStateMismatch);
    }

    let pkce_verifier = oauth2::PkceCodeVerifier::new(stored_verifier.value().to_string());

    let token = state
        .oauth_client
        .exchange_code(AuthorizationCode::new(params.code))
        .set_pkce_verifier(pkce_verifier)
        .request_async(async_http_client)
        .await
        .map_err(|e| AppError::TidalApi(e.to_string()))?;

    let access_token = token.access_token().secret();
    let refresh_token = token
        .refresh_token()
        .ok_or_else(|| AppError::TidalApi("no refresh token in response".into()))?
        .secret();

    let expires_at = token.expires_in().map(|d| OffsetDateTime::now_utc() + d);

    let user = tidal::fetch_user(&state.http, access_token).await?;

    db::users::upsert(&state.db, &user.id, access_token, refresh_token, expires_at)
        .await
        .map_err(AppError::Anyhow)?;

    let jar = jar
        .remove(Cookie::from(COOKIE_OAUTH_STATE))
        .remove(Cookie::from(COOKIE_OAUTH_VERIFIER))
        .add({
            let mut c = Cookie::new(COOKIE_SESSION, user.id);
            c.set_path("/");
            c.set_http_only(true);
            c.set_same_site(SameSite::Lax);
            if state.config.base_url.starts_with("https://") {
                c.set_secure(true);
            }
            c
        });

    Ok((jar, Redirect::to(&redirect_target)).into_response())
}

pub async fn logout(jar: PrivateCookieJar) -> impl IntoResponse {
    let jar = jar.remove(Cookie::from(COOKIE_SESSION));
    (jar, Redirect::to("/"))
}

pub fn current_user_id(jar: &PrivateCookieJar) -> Option<String> {
    jar.get(COOKIE_SESSION).map(|c| c.value().to_string())
}

/// Returns a valid access token for the user, refreshing it if it expires within 5 minutes.
/// Returns an error if the user doesn't exist in the DB or the refresh fails.
pub async fn ensure_fresh_token(state: &crate::AppState, user_id: &str) -> Result<String, AppError> {
    let user = db::users::get(&state.db, user_id)
        .await
        .map_err(AppError::Anyhow)?
        .ok_or_else(|| AppError::TidalApi("session expired".into()))?;

    let expires_soon = user.token_expires_at
        .as_deref()
        .map(|s| {
            // If we can't parse the expiry (e.g. old format), assume it might be expired.
            OffsetDateTime::parse(s, &Rfc3339)
                .map(|exp| exp - OffsetDateTime::now_utc() < Duration::minutes(5))
                .unwrap_or(true)
        })
        .unwrap_or(false); // NULL token_expires_at means we have no info — don't refresh speculatively

    if !expires_soon {
        return Ok(user.access_token);
    }

    tracing::info!("refreshing access token for user {user_id}");

    let new_token = state
        .oauth_client
        .exchange_refresh_token(&RefreshToken::new(user.refresh_token.clone()))
        .request_async(async_http_client)
        .await
        .map_err(|e| AppError::TidalApi(format!("token refresh failed: {e}")))?;

    let access_token = new_token.access_token().secret().clone();
    let refresh_token = new_token
        .refresh_token()
        .map(|t| t.secret().clone())
        .unwrap_or(user.refresh_token);
    let expires_at = new_token.expires_in().map(|d| OffsetDateTime::now_utc() + d);

    db::users::upsert(&state.db, user_id, &access_token, &refresh_token, expires_at)
        .await
        .map_err(AppError::Anyhow)?;

    Ok(access_token)
}

fn short_lived_cookie(name: &'static str, value: String) -> Cookie<'static> {
    let mut c = Cookie::new(name, value);
    c.set_path("/");
    c.set_http_only(true);
    c.set_same_site(SameSite::Lax);
    c.set_max_age(Duration::minutes(10));
    c
}
