use sqlx::SqlitePool;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct User {
    #[allow(dead_code)]
    pub id: String,
    pub access_token: String,
    pub refresh_token: String,
    pub token_expires_at: Option<String>,
}

pub async fn upsert(
    pool: &SqlitePool,
    id: &str,
    access_token: &str,
    refresh_token: &str,
    token_expires_at: Option<OffsetDateTime>,
) -> anyhow::Result<()> {
    let now = OffsetDateTime::now_utc().format(&Rfc3339)?;
    let expires = token_expires_at.and_then(|t| t.format(&Rfc3339).ok());

    sqlx::query(
        r#"
        INSERT INTO users (id, access_token, refresh_token, token_expires_at, last_seen_at)
        VALUES (?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            access_token = excluded.access_token,
            refresh_token = excluded.refresh_token,
            token_expires_at = excluded.token_expires_at,
            last_seen_at = excluded.last_seen_at
        "#,
    )
    .bind(id)
    .bind(access_token)
    .bind(refresh_token)
    .bind(expires)
    .bind(now)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn get(pool: &SqlitePool, id: &str) -> anyhow::Result<Option<User>> {
    let user = sqlx::query_as::<_, User>(
        "SELECT id, access_token, refresh_token, token_expires_at FROM users WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(user)
}
