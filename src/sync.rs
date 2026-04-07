use std::collections::HashSet;

use crate::{AppState, auth, db, error::AppError};

struct MemberState {
    user_id: String,
    tidal_playlist_id: String,
    access_token: String,
    current_tracks: HashSet<String>,
}

/// Run a full sync cycle for one shared playlist.
///
/// Algorithm:
/// 1. Fetch live tracks from all members' TIDAL playlists.
/// 2. Diff each member against the canonical snapshot.
/// 3. Merge diffs; deletion wins conflicts.
/// 4. Commit the new canonical snapshot to the DB.
/// 5. Write back per-member diffs (add missing, remove extras).
/// 6. Stamp last_synced_at.
pub async fn run(state: &AppState, playlist_id: &str) -> Result<(), AppError> {
    let members = db::playlists::get_all_members(&state.db, playlist_id)
        .await
        .map_err(AppError::Anyhow)?;

    if members.is_empty() {
        return Ok(());
    }

    // Step 1: fetch live tracks for all members and cache tokens.
    let mut member_states: Vec<MemberState> = Vec::new();
    for member in &members {
        let access_token = auth::ensure_fresh_token(state, &member.user_id).await?;
        let tracks: HashSet<String> =
            auth::tidal::fetch_playlist_track_ids(&state.http, &access_token, &member.tidal_playlist_id)
                .await?
                .into_iter()
                .collect();
        member_states.push(MemberState {
            user_id: member.user_id.clone(),
            tidal_playlist_id: member.tidal_playlist_id.clone(),
            access_token,
            current_tracks: tracks,
        });
    }

    // Step 2: load canonical and compute merged diff.
    let canonical = db::playlists::get_canonical_tracks(&state.db, playlist_id)
        .await
        .map_err(AppError::Anyhow)?;

    let mut all_additions: HashSet<String> = HashSet::new();
    let mut all_deletions: HashSet<String> = HashSet::new();

    for ms in &member_states {
        for id in ms.current_tracks.difference(&canonical) {
            all_additions.insert(id.clone());
        }
        for id in canonical.difference(&ms.current_tracks) {
            all_deletions.insert(id.clone());
        }
    }

    // Step 3: deletion wins conflicts.
    for id in &all_deletions {
        if all_additions.remove(id) {
            tracing::info!(
                playlist_id,
                track_id = id,
                "sync conflict: added by one member, deleted by another — deletion wins"
            );
        }
    }

    let mut new_canonical = canonical.clone();
    for id in &all_additions {
        new_canonical.insert(id.clone());
    }
    for id in &all_deletions {
        new_canonical.remove(id);
    }

    // Step 4: commit canonical before writing back (safe to retry on failure).
    db::playlists::set_canonical_tracks(&state.db, playlist_id, &new_canonical)
        .await
        .map_err(AppError::Anyhow)?;

    // Step 5: write per-member diffs.
    for ms in &member_states {
        let to_add: Vec<String> = new_canonical
            .difference(&ms.current_tracks)
            .cloned()
            .collect();
        let to_remove: Vec<String> = ms
            .current_tracks
            .difference(&new_canonical)
            .cloned()
            .collect();

        if !to_add.is_empty() {
            auth::tidal::add_playlist_tracks(
                &state.http,
                &ms.access_token,
                &ms.tidal_playlist_id,
                &to_add,
            )
            .await?;
        }
        if !to_remove.is_empty() {
            auth::tidal::remove_playlist_tracks(
                &state.http,
                &ms.access_token,
                &ms.tidal_playlist_id,
                &to_remove,
            )
            .await?;
        }
    }

    // Step 6: stamp last_synced_at.
    db::playlists::update_last_synced_at(&state.db, playlist_id)
        .await
        .map_err(AppError::Anyhow)?;

    tracing::info!(playlist_id, "sync complete");
    Ok(())
}
