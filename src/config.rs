use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub tidal_client_id: String,
    pub tidal_client_secret: String,
    pub tidal_redirect_uri: String,
    pub cookie_secret: Vec<u8>,
    pub base_url: String,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            database_url: required("DATABASE_URL")?,
            tidal_client_id: required("TIDAL_CLIENT_ID")?,
            tidal_client_secret: required("TIDAL_CLIENT_SECRET")?,
            tidal_redirect_uri: required("TIDAL_REDIRECT_URI")?,
            cookie_secret: hex_bytes("COOKIE_SECRET")?,
            base_url: env::var("BASE_URL")
                .unwrap_or_else(|_| "http://localhost:3000".to_string()),
        })
    }
}

fn required(key: &str) -> anyhow::Result<String> {
    env::var(key).map_err(|_| anyhow::anyhow!("missing env var: {key}"))
}

fn hex_bytes(key: &str) -> anyhow::Result<Vec<u8>> {
    let hex = required(key)?;
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|e| anyhow::anyhow!("{e}")))
        .collect::<anyhow::Result<_>>()?;
    anyhow::ensure!(
        bytes.len() >= 64,
        "COOKIE_SECRET must be at least 64 bytes (128 hex chars); got {}. \
         Generate one with: openssl rand -hex 64",
        bytes.len()
    );
    Ok(bytes)
}
