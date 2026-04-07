use std::collections::{HashMap, HashSet};

use sqlx::SqlitePool;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct DbPlaylist {
    pub id: String,
    pub name: String,
    pub owner_user_id: String,
}

pub async fn get_by_id(pool: &SqlitePool, playlist_id: &str) -> anyhow::Result<Option<DbPlaylist>> {
    #[derive(sqlx::FromRow)]
    struct Row {
        id: String,
        name: String,
        owner_user_id: String,
    }

    let row =
        sqlx::query_as::<_, Row>("SELECT id, name, owner_user_id FROM playlists WHERE id = ?")
            .bind(playlist_id)
            .fetch_optional(pool)
            .await?;

    Ok(row.map(|r| DbPlaylist {
        id: r.id,
        name: r.name,
        owner_user_id: r.owner_user_id,
    }))
}

/// Add a member to a shared playlist. The tidal_playlist_id is the member's own TIDAL playlist.
/// No-ops silently if the user is already a member.
pub async fn join(
    pool: &SqlitePool,
    playlist_id: &str,
    user_id: &str,
    tidal_playlist_id: &str,
) -> anyhow::Result<()> {
    let member_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT OR IGNORE INTO playlist_members (id, playlist_id, user_id, tidal_playlist_id, is_owner) VALUES (?, ?, ?, ?, 0)",
    )
    .bind(&member_id)
    .bind(playlist_id)
    .bind(user_id)
    .bind(tidal_playlist_id)
    .execute(pool)
    .await?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct SharedInfo {
    pub playlist_id: String,
    pub member_count: i64,
    pub is_owner: bool,
    pub last_synced_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PlaylistMember {
    pub user_id: String,
    pub tidal_playlist_id: String,
}

/// Returns a map of tidal_playlist_id -> SharedInfo for all playlists this user is a member of
/// (both owned and joined).
pub async fn get_shared_by_user(
    pool: &SqlitePool,
    user_id: &str,
) -> anyhow::Result<HashMap<String, SharedInfo>> {
    #[derive(sqlx::FromRow)]
    struct Row {
        tidal_playlist_id: String,
        playlist_id: String,
        member_count: i64,
        is_owner: i64,
        last_synced_at: Option<String>,
    }

    let rows = sqlx::query_as::<_, Row>(
        r#"
        SELECT pm.tidal_playlist_id, pm.playlist_id,
               (SELECT COUNT(*) FROM playlist_members pm2 WHERE pm2.playlist_id = pm.playlist_id) AS member_count,
               pm.is_owner,
               p.last_synced_at
        FROM playlist_members pm
        JOIN playlists p ON p.id = pm.playlist_id
        WHERE pm.user_id = ?
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    let mut map = HashMap::new();
    for row in rows {
        map.insert(
            row.tidal_playlist_id,
            SharedInfo {
                playlist_id: row.playlist_id,
                member_count: row.member_count,
                is_owner: row.is_owner != 0,
                last_synced_at: row.last_synced_at,
            },
        );
    }
    Ok(map)
}

/// Returns all internal playlist IDs (for background poller).
pub async fn get_all_ids(pool: &SqlitePool) -> anyhow::Result<Vec<String>> {
    let ids = sqlx::query_scalar::<_, String>("SELECT id FROM playlists")
        .fetch_all(pool)
        .await?;
    Ok(ids)
}

/// Share a TIDAL playlist: creates a playlists row and an owner playlist_members row.
pub async fn share(
    pool: &SqlitePool,
    tidal_playlist_id: &str,
    owner_user_id: &str,
    name: &str,
) -> anyhow::Result<SharedInfo> {
    let playlist_id = Uuid::new_v4().to_string();
    let member_id = Uuid::new_v4().to_string();

    sqlx::query("INSERT INTO playlists (id, owner_user_id, name) VALUES (?, ?, ?)")
        .bind(&playlist_id)
        .bind(owner_user_id)
        .bind(name)
        .execute(pool)
        .await?;

    sqlx::query(
        "INSERT INTO playlist_members (id, playlist_id, user_id, tidal_playlist_id, is_owner) VALUES (?, ?, ?, ?, 1)",
    )
    .bind(&member_id)
    .bind(&playlist_id)
    .bind(owner_user_id)
    .bind(tidal_playlist_id)
    .execute(pool)
    .await?;

    Ok(SharedInfo {
        playlist_id,
        member_count: 1,
        is_owner: true,
        last_synced_at: None,
    })
}

/// Leave a shared playlist as a non-owner: removes only this user's membership row.
pub async fn leave(
    pool: &SqlitePool,
    tidal_playlist_id: &str,
    user_id: &str,
) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM playlist_members WHERE user_id = ? AND tidal_playlist_id = ?")
        .bind(user_id)
        .bind(tidal_playlist_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Returns all members of a shared playlist (for sync).
pub async fn get_all_members(
    pool: &SqlitePool,
    playlist_id: &str,
) -> anyhow::Result<Vec<PlaylistMember>> {
    #[derive(sqlx::FromRow)]
    struct Row {
        user_id: String,
        tidal_playlist_id: String,
    }

    let rows = sqlx::query_as::<_, Row>(
        "SELECT user_id, tidal_playlist_id FROM playlist_members WHERE playlist_id = ?",
    )
    .bind(playlist_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| PlaylistMember {
            user_id: r.user_id,
            tidal_playlist_id: r.tidal_playlist_id,
        })
        .collect())
}

/// Fetch the canonical track set for a playlist.
pub async fn get_canonical_tracks(
    pool: &SqlitePool,
    playlist_id: &str,
) -> anyhow::Result<HashSet<String>> {
    let rows = sqlx::query_scalar::<_, String>(
        "SELECT track_id FROM playlist_tracks WHERE playlist_id = ?",
    )
    .bind(playlist_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().collect())
}

/// Replace the canonical track set for a playlist (within a transaction).
pub async fn set_canonical_tracks(
    pool: &SqlitePool,
    playlist_id: &str,
    track_ids: &HashSet<String>,
) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;

    sqlx::query("DELETE FROM playlist_tracks WHERE playlist_id = ?")
        .bind(playlist_id)
        .execute(&mut *tx)
        .await?;

    for track_id in track_ids {
        sqlx::query("INSERT INTO playlist_tracks (playlist_id, track_id) VALUES (?, ?)")
            .bind(playlist_id)
            .bind(track_id)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;
    Ok(())
}

/// Stamp the playlist with the current UTC time as last_synced_at.
pub async fn update_last_synced_at(pool: &SqlitePool, playlist_id: &str) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE playlists SET last_synced_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE id = ?",
    )
    .bind(playlist_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Unshare a playlist. Only succeeds if the current user is the sole member.
/// Returns true if unshared, false if there are other members (no-op).
pub async fn unshare(
    pool: &SqlitePool,
    tidal_playlist_id: &str,
    owner_user_id: &str,
) -> anyhow::Result<bool> {
    #[derive(sqlx::FromRow)]
    struct Row {
        id: String,
        member_count: i64,
    }

    let row = sqlx::query_as::<_, Row>(
        r#"
        SELECT p.id, COUNT(pm2.id) AS member_count
        FROM playlists p
        JOIN playlist_members pm ON pm.playlist_id = p.id AND pm.user_id = ? AND pm.tidal_playlist_id = ?
        JOIN playlist_members pm2 ON pm2.playlist_id = p.id
        GROUP BY p.id
        "#,
    )
    .bind(owner_user_id)
    .bind(tidal_playlist_id)
    .fetch_optional(pool)
    .await?;

    match row {
        None => Ok(false),
        Some(r) if r.member_count > 1 => Ok(false),
        Some(r) => {
            sqlx::query("DELETE FROM playlists WHERE id = ?")
                .bind(&r.id)
                .execute(pool)
                .await?;
            Ok(true)
        }
    }
}
