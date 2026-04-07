use oauth2::{
    AuthUrl, ClientId, ClientSecret, RedirectUrl, TokenUrl,
    basic::BasicClient,
};
use reqwest::Client;
use serde::Deserialize;

use crate::config::Config;
use crate::error::AppError;

pub fn oauth_client(config: &Config) -> anyhow::Result<BasicClient> {
    let client = BasicClient::new(
        ClientId::new(config.tidal_client_id.clone()),
        Some(ClientSecret::new(config.tidal_client_secret.clone())),
        AuthUrl::new("https://login.tidal.com/authorize".to_string())
            .map_err(|e| anyhow::anyhow!(e))?,
        Some(
            TokenUrl::new("https://auth.tidal.com/v1/oauth2/token".to_string())
                .map_err(|e| anyhow::anyhow!(e))?,
        ),
    )
    .set_redirect_uri(
        RedirectUrl::new(config.tidal_redirect_uri.clone()).map_err(|e| anyhow::anyhow!(e))?,
    );

    Ok(client)
}

#[derive(Debug, Deserialize)]
pub struct TidalUser {
    pub id: String,
}

#[derive(Debug, Deserialize)]
struct UserResponse {
    data: UserData,
}

#[derive(Debug, Deserialize)]
struct UserData {
    id: String,
}

#[derive(Debug)]
pub struct Playlist {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub item_count: u32,
}

#[derive(Debug, Deserialize)]
struct PlaylistsResponse {
    data: Vec<PlaylistData>,
    links: Option<PlaylistsLinks>,
}

#[derive(Debug, Deserialize)]
struct PlaylistsLinks {
    meta: Option<PlaylistsMeta>,
}

#[derive(Debug, Deserialize)]
struct PlaylistsMeta {
    #[serde(rename = "nextCursor")]
    next_cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PlaylistData {
    id: String,
    attributes: PlaylistAttributes,
}

#[derive(Debug, Deserialize)]
struct PlaylistAttributes {
    #[serde(rename = "name")]
    title: Option<String>,
    description: Option<String>,
    #[serde(rename = "numberOfItems")]
    item_count: Option<u32>,
}

pub async fn fetch_playlists(http: &Client, access_token: &str) -> Result<Vec<Playlist>, AppError> {
    let mut all = Vec::new();
    let mut cursor: Option<String> = None;

    loop {
        let mut query = vec![
            ("filter[owners.id]".to_string(), "me".to_string()),
            ("sort".to_string(), "-lastModifiedAt".to_string()),
        ];
        if let Some(ref c) = cursor {
            query.push(("page[cursor]".to_string(), c.clone()));
        }

        let resp = http
            .get("https://openapi.tidal.com/v2/playlists")
            .bearer_auth(access_token)
            .header("Accept", "application/vnd.api+json")
            .query(&query)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::TidalApi(format!("{status}: {body}")));
        }

        let parsed: PlaylistsResponse = resp.json().await?;

        all.extend(parsed.data.into_iter().map(|d| Playlist {
            id: d.id,
            title: d.attributes.title.unwrap_or_default(),
            description: d.attributes.description,
            item_count: d.attributes.item_count.unwrap_or(0),
        }));


        cursor = parsed.links.and_then(|l| l.meta).and_then(|m| m.next_cursor);
        if cursor.is_none() {
            break;
        }
    }

    Ok(all)
}

/// Create a new TIDAL playlist and return its ID.
pub async fn create_playlist(http: &Client, access_token: &str, name: &str) -> Result<String, AppError> {
    let body = serde_json::json!({
        "data": {
            "type": "playlists",
            "attributes": {
                "name": name,
                "description": ""
            }
        }
    });

    let resp = http
        .post("https://openapi.tidal.com/v2/playlists")
        .bearer_auth(access_token)
        .header("Content-Type", "application/vnd.api+json")
        .header("Accept", "application/vnd.api+json")
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(AppError::TidalApi(format!("{status}: {body}")));
    }

    #[derive(Deserialize)]
    struct CreateResponse {
        data: CreateData,
    }
    #[derive(Deserialize)]
    struct CreateData {
        id: String,
    }

    let parsed: CreateResponse = resp.json().await?;
    Ok(parsed.data.id)
}

/// Delete a TIDAL playlist. Returns Ok(()) even if the playlist no longer exists (404).
pub async fn delete_playlist(http: &Client, access_token: &str, tidal_playlist_id: &str) -> Result<(), AppError> {
    let resp = http
        .delete(format!("https://openapi.tidal.com/v2/playlists/{tidal_playlist_id}"))
        .bearer_auth(access_token)
        .header("Accept", "application/vnd.api+json")
        .send()
        .await?;

    match resp.status().as_u16() {
        200..=299 | 404 => Ok(()),
        status => {
            let body = resp.text().await.unwrap_or_default();
            Err(AppError::TidalApi(format!("{status}: {body}")))
        }
    }
}

/// Fetch all track IDs from a TIDAL playlist (paginated, skips non-track items).
pub async fn fetch_playlist_track_ids(
    http: &Client,
    access_token: &str,
    tidal_playlist_id: &str,
) -> Result<Vec<String>, AppError> {
    #[derive(Deserialize)]
    struct ItemsResponse {
        data: Vec<ItemData>,
        links: Option<ItemsLinks>,
    }
    #[derive(Deserialize)]
    struct ItemData {
        id: String,
        #[serde(rename = "type")]
        item_type: String,
    }
    #[derive(Deserialize)]
    struct ItemsLinks {
        meta: Option<ItemsMeta>,
    }
    #[derive(Deserialize)]
    struct ItemsMeta {
        #[serde(rename = "nextCursor")]
        next_cursor: Option<String>,
    }

    let mut all = Vec::new();
    let mut cursor: Option<String> = None;

    loop {
        let mut query: Vec<(&str, String)> = vec![];
        if let Some(ref c) = cursor {
            query.push(("page[cursor]", c.clone()));
        }

        let resp = http
            .get(format!(
                "https://openapi.tidal.com/v2/playlists/{tidal_playlist_id}/relationships/items"
            ))
            .bearer_auth(access_token)
            .header("Accept", "application/vnd.api+json")
            .query(&query)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::TidalApi(format!("{status}: {body}")));
        }

        let parsed: ItemsResponse = resp.json().await?;
        all.extend(
            parsed
                .data
                .into_iter()
                .filter(|d| d.item_type == "tracks")
                .map(|d| d.id),
        );

        cursor = parsed
            .links
            .and_then(|l| l.meta)
            .and_then(|m| m.next_cursor);
        if cursor.is_none() {
            break;
        }
    }

    Ok(all)
}

/// Add tracks to a TIDAL playlist in chunks of 20.
pub async fn add_playlist_tracks(
    http: &Client,
    access_token: &str,
    tidal_playlist_id: &str,
    track_ids: &[String],
) -> Result<(), AppError> {
    for chunk in track_ids.chunks(20) {
        let data: Vec<_> = chunk
            .iter()
            .map(|id| serde_json::json!({"id": id, "type": "tracks"}))
            .collect();
        let body = serde_json::json!({"data": data});

        let resp = http
            .post(format!(
                "https://openapi.tidal.com/v2/playlists/{tidal_playlist_id}/relationships/items"
            ))
            .bearer_auth(access_token)
            .header("Content-Type", "application/vnd.api+json")
            .header("Accept", "application/vnd.api+json")
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::TidalApi(format!("add tracks {status}: {body}")));
        }
    }
    Ok(())
}

/// Remove tracks from a TIDAL playlist in chunks of 20.
pub async fn remove_playlist_tracks(
    http: &Client,
    access_token: &str,
    tidal_playlist_id: &str,
    track_ids: &[String],
) -> Result<(), AppError> {
    for chunk in track_ids.chunks(20) {
        let data: Vec<_> = chunk
            .iter()
            .map(|id| serde_json::json!({"id": id, "type": "tracks"}))
            .collect();
        let body = serde_json::json!({"data": data});

        let resp = http
            .delete(format!(
                "https://openapi.tidal.com/v2/playlists/{tidal_playlist_id}/relationships/items"
            ))
            .bearer_auth(access_token)
            .header("Content-Type", "application/vnd.api+json")
            .header("Accept", "application/vnd.api+json")
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AppError::TidalApi(format!(
                "remove tracks {status}: {body}"
            )));
        }
    }
    Ok(())
}

pub async fn fetch_user(http: &Client, access_token: &str) -> Result<TidalUser, AppError> {
    let resp = http
        .get("https://openapi.tidal.com/v2/users/me")
        .bearer_auth(access_token)
        .header("Accept", "application/vnd.api+json")
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(AppError::TidalApi(format!("{status}: {body}")));
    }

    let parsed: UserResponse = resp.json().await?;

    Ok(TidalUser { id: parsed.data.id })
}
