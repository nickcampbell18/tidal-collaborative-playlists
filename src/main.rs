mod auth;
mod config;
mod db;
mod error;
mod sync;
mod views;

use std::sync::Arc;

use axum::{
    Form, Router,
    extract::{FromRef, Path, Query, State},
    response::IntoResponse,
    routing::{get, post},
};
use axum_extra::extract::PrivateCookieJar;
use axum_extra::extract::cookie::Key;
use oauth2::basic::BasicClient;
use reqwest::Client;
use serde::Deserialize;
use sqlx::SqlitePool;
use tower_http::trace::TraceLayer;

use config::Config;
use views::dashboard::PlaylistRow;

#[derive(Clone)]
struct AppState {
    config: Arc<Config>,
    db: SqlitePool,
    oauth_client: BasicClient,
    http: Client,
    cookie_key: Key,
}

impl FromRef<AppState> for Key {
    fn from_ref(state: &AppState) -> Self {
        state.cookie_key.clone()
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "tidal_collaborative_playlists=debug,tower_http=debug".into()),
        )
        .init();

    dotenvy::dotenv().ok();

    let config = Config::from_env()?;
    let cookie_key = Key::from(&config.cookie_secret);
    let oauth_client = auth::tidal::oauth_client(&config)?;
    let db = db::connect(&config.database_url).await?;
    let http = Client::builder().use_rustls_tls().build()?;

    let state = AppState {
        config: Arc::new(config),
        db,
        oauth_client,
        http,
        cookie_key,
    };

    // Background sync poller: every 5 minutes, sync all shared playlists.
    {
        let state = state.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(5 * 60)).await;

                let ids = match db::playlists::get_all_ids(&state.db).await {
                    Ok(ids) => ids,
                    Err(e) => {
                        tracing::error!("poller: failed to list playlists: {e}");
                        continue;
                    }
                };

                for playlist_id in ids {
                    if let Err(e) = sync::run(&state, &playlist_id).await {
                        tracing::warn!(playlist_id, "poller: sync failed: {e}");
                    }
                }
            }
        });
    }

    let app = Router::new()
        .route("/", get(index))
        .route("/auth/login", get(auth::login))
        .route("/auth/callback", get(auth::callback))
        .route("/auth/logout", post(auth::logout))
        .route("/playlists/:tidal_id/share", post(share_playlist))
        .route("/playlists/:tidal_id/unshare", post(unshare_playlist))
        .route("/playlists/:tidal_id/leave", post(leave_playlist))
        .route("/playlists/:playlist_id/sync", post(sync_playlist))
        .route("/join/:playlist_id", get(join_page))
        .route("/join/:playlist_id", post(join_playlist))
        .layer(TraceLayer::new_for_http());

    #[cfg(debug_assertions)]
    let app = app.layer(tower_livereload::LiveReloadLayer::new());

    let app = app.with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    tracing::info!("listening on http://localhost:3000");
    axum::serve(listener, app).await?;

    Ok(())
}

#[derive(Deserialize, Default)]
struct FilterParams {
    q: Option<String>,
    sort: Option<String>,
}

async fn index(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Query(params): Query<FilterParams>,
) -> impl IntoResponse {
    match auth::current_user_id(&jar) {
        None => views::home::page().into_response(),
        Some(user_id) => {
            let access_token = match auth::ensure_fresh_token(&state, &user_id).await {
                Ok(t) => t,
                Err(_) => return views::home::page().into_response(),
            };
            let playlists = match auth::tidal::fetch_playlists(&state.http, &access_token).await {
                Ok(p) => p,
                Err(e) => return e.into_response(),
            };
            let shared_map = match db::playlists::get_shared_by_user(&state.db, &user_id).await {
                Ok(m) => m,
                Err(e) => return error::AppError::Anyhow(e).into_response(),
            };

            let mut rows: Vec<PlaylistRow> = playlists
                .into_iter()
                .map(|p| PlaylistRow {
                    shared: shared_map.get(&p.id).cloned(),
                    tidal_id: p.id,
                    title: p.title,
                    item_count: p.item_count,
                    description: p.description.clone(),
                })
                .collect();

            if let Some(q) = params.q.as_deref().filter(|s| !s.is_empty()) {
                let q_lower = q.to_lowercase();
                rows.retain(|r| r.title.to_lowercase().contains(&q_lower));
            }

            match params.sort.as_deref() {
                Some("name") => {
                    rows.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
                }
                Some("count") => rows.sort_by(|a, b| b.item_count.cmp(&a.item_count)),
                _ => {} // "modified" or default: preserve TIDAL API order (last modified desc)
            }

            views::dashboard::page(
                &rows,
                &state.config.base_url,
                params.q.as_deref(),
                params.sort.as_deref(),
            )
            .into_response()
        }
    }
}

#[derive(Deserialize)]
struct PlaylistForm {
    name: String,
    item_count: u32,
    description: Option<String>,
}

async fn share_playlist(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path(tidal_id): Path<String>,
    Form(body): Form<PlaylistForm>,
) -> impl IntoResponse {
    let Some(user_id) = auth::current_user_id(&jar) else {
        return error::AppError::TidalApi("not authenticated".into()).into_response();
    };

    let access_token = match auth::ensure_fresh_token(&state, &user_id).await {
        Ok(t) => t,
        Err(e) => return e.into_response(),
    };

    let shared = match db::playlists::share(
        &state.db,
        &tidal_id,
        &user_id,
        &body.name,
        body.description.as_deref(),
    )
    .await
    {
        Ok(info) => Some(info),
        Err(e) => return error::AppError::Anyhow(e).into_response(),
    };

    // Snapshot the owner's current tracks as the canonical baseline so that
    // future syncs can correctly detect additions and deletions.
    if let Some(ref info) = shared {
        match auth::tidal::fetch_playlist_track_ids(&state.http, &access_token, &tidal_id).await {
            Ok(ids) => {
                let track_set: std::collections::HashSet<String> = ids.into_iter().collect();
                if let Err(e) =
                    db::playlists::set_canonical_tracks(&state.db, &info.playlist_id, &track_set)
                        .await
                {
                    return error::AppError::Anyhow(e).into_response();
                }
            }
            Err(e) => return e.into_response(),
        }
    }

    let row = PlaylistRow {
        tidal_id,
        title: body.name,
        item_count: body.item_count,
        description: body.description.clone(),
        shared,
    };
    views::dashboard::playlist_row(&row, &state.config.base_url).into_response()
}

async fn unshare_playlist(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path(tidal_id): Path<String>,
    Form(body): Form<PlaylistForm>,
) -> impl IntoResponse {
    let Some(user_id) = auth::current_user_id(&jar) else {
        return error::AppError::TidalApi("not authenticated".into()).into_response();
    };

    let shared = match db::playlists::get_shared_by_user(&state.db, &user_id).await {
        Ok(m) => m,
        Err(e) => return error::AppError::Anyhow(e).into_response(),
    };
    let current = shared.get(&tidal_id).cloned();

    let new_shared = match db::playlists::unshare(&state.db, &tidal_id, &user_id).await {
        Ok(true) => None,
        Ok(false) => current, // multiple members, leave unchanged
        Err(e) => return error::AppError::Anyhow(e).into_response(),
    };

    let row = PlaylistRow {
        tidal_id,
        title: body.name,
        item_count: body.item_count,
        description: body.description.clone(),
        shared: new_shared,
    };
    views::dashboard::playlist_row(&row, &state.config.base_url).into_response()
}

async fn leave_playlist(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path(tidal_id): Path<String>,
    Form(body): Form<PlaylistForm>,
) -> impl IntoResponse {
    let Some(user_id) = auth::current_user_id(&jar) else {
        return error::AppError::TidalApi("not authenticated".into()).into_response();
    };

    let access_token = match auth::ensure_fresh_token(&state, &user_id).await {
        Ok(t) => t,
        Err(e) => return e.into_response(),
    };

    if let Err(e) = db::playlists::leave(&state.db, &tidal_id, &user_id).await {
        return error::AppError::Anyhow(e).into_response();
    }

    // Best-effort: delete the TIDAL playlist copy. Log on failure but don't block the response.
    if let Err(e) = auth::tidal::delete_playlist(&state.http, &access_token, &tidal_id).await {
        tracing::warn!("failed to delete TIDAL playlist {tidal_id} after leave: {e}");
    }

    let row = PlaylistRow {
        tidal_id,
        title: body.name,
        item_count: body.item_count,
        description: body.description.clone(),
        shared: None,
    };
    views::dashboard::playlist_row(&row, &state.config.base_url).into_response()
}

#[derive(Deserialize)]
struct SyncForm {
    tidal_id: String,
    name: String,
    item_count: u32,
    description: Option<String>,
}

async fn sync_playlist(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path(playlist_id): Path<String>,
    Form(body): Form<SyncForm>,
) -> impl IntoResponse {
    let Some(user_id) = auth::current_user_id(&jar) else {
        return error::AppError::TidalApi("not authenticated".into()).into_response();
    };

    if let Err(e) = sync::run(&state, &playlist_id).await {
        return e.into_response();
    }

    let shared_map = match db::playlists::get_shared_by_user(&state.db, &user_id).await {
        Ok(m) => m,
        Err(e) => return error::AppError::Anyhow(e).into_response(),
    };
    let shared = shared_map.get(&body.tidal_id).cloned();
    let row = PlaylistRow {
        tidal_id: body.tidal_id,
        title: body.name,
        item_count: body.item_count,
        description: body.description.clone(),
        shared,
    };
    views::dashboard::playlist_row(&row, &state.config.base_url).into_response()
}

async fn join_page(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path(playlist_id): Path<String>,
) -> impl IntoResponse {
    let Some(user_id) = auth::current_user_id(&jar) else {
        let login_url = format!("/auth/login?return_to=/join/{playlist_id}");
        return axum::response::Redirect::to(&login_url).into_response();
    };

    // Ensure the user record exists (they may have been removed somehow).
    if db::users::get(&state.db, &user_id)
        .await
        .ok()
        .flatten()
        .is_none()
    {
        return axum::response::Redirect::to("/auth/login").into_response();
    }

    match db::playlists::get_by_id(&state.db, &playlist_id).await {
        Ok(Some(playlist)) => {
            views::join::page(&playlist.name, playlist.description.as_deref()).into_response()
        }
        Ok(None) => error::AppError::TidalApi("Invite link not found".into()).into_response(),
        Err(e) => error::AppError::Anyhow(e).into_response(),
    }
}

#[derive(serde::Deserialize)]
struct JoinForm {
    name: String,
    description: Option<String>,
}

async fn join_playlist(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Path(playlist_id): Path<String>,
    Form(body): Form<JoinForm>,
) -> impl IntoResponse {
    let Some(user_id) = auth::current_user_id(&jar) else {
        return axum::response::Redirect::to("/auth/login").into_response();
    };

    let access_token = match auth::ensure_fresh_token(&state, &user_id).await {
        Ok(t) => t,
        Err(_) => return axum::response::Redirect::to("/auth/login").into_response(),
    };

    // Verify the playlist exists.
    match db::playlists::get_by_id(&state.db, &playlist_id).await {
        Ok(None) => {
            return error::AppError::TidalApi("Invite link not found".into()).into_response();
        }
        Err(e) => return error::AppError::Anyhow(e).into_response(),
        Ok(Some(_)) => {}
    }

    // Create a new TIDAL playlist in the joiner's account.
    let tidal_playlist_id = match auth::tidal::create_playlist(
        &state.http,
        &access_token,
        &body.name,
        &if let Some(ref existing_desc) = body.description {
            if existing_desc.is_empty() {
                format!(
                    "Managed by Tidal Collaborative Playlists ({})",
                    state.config.base_url
                )
            } else {
                format!(
                    "{}\n\nManaged by Tidal Collaborative Playlists ({})",
                    existing_desc, state.config.base_url
                )
            }
        } else {
            format!(
                "Managed by Tidal Collaborative Playlists ({})",
                state.config.base_url
            )
        },
    )
    .await
    {
        Ok(id) => id,
        Err(e) => return e.into_response(),
    };

    // Pre-populate the new playlist with the canonical tracks so the first sync
    // doesn't interpret an empty playlist as "the member deleted everything".
    let canonical = match db::playlists::get_canonical_tracks(&state.db, &playlist_id).await {
        Ok(tracks) => tracks,
        Err(e) => return error::AppError::Anyhow(e).into_response(),
    };
    if !canonical.is_empty() {
        let ids: Vec<String> = canonical.into_iter().collect();
        if let Err(e) =
            auth::tidal::add_playlist_tracks(&state.http, &access_token, &tidal_playlist_id, &ids)
                .await
        {
            return e.into_response();
        }
    }

    // Record the membership.
    if let Err(e) = db::playlists::join(&state.db, &playlist_id, &user_id, &tidal_playlist_id).await
    {
        return error::AppError::Anyhow(e).into_response();
    }

    axum::response::Redirect::to("/").into_response()
}
