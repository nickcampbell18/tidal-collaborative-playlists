ALTER TABLE playlists ADD COLUMN last_synced_at TEXT;

CREATE TABLE IF NOT EXISTS playlist_tracks (
    playlist_id TEXT NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
    track_id TEXT NOT NULL,
    PRIMARY KEY (playlist_id, track_id)
);
