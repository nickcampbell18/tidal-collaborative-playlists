# TIDAL Collaborative Playlists

A small web application that adds the missing "shared playlists" feature that Tidal needs!

## How it works

For every person who "joins" one of your playlists, they get a complete copy of yours. When they make changes to their copy, it synchronises those changes back to your version of the playlist, keeping everybody up to date.

Changes are synchronised approximately every 5 minutes, but the Tidal API can be quite restrictive with rate limits so it might take longer - there is a "Sync now" button if you need to manually trigger the sync.

## Running with Docker

The easiest way to run this application is using Docker Compose:

1. Clone the repo
2. Get a TIDAL client ID & secret from [https://developer.tidal.com/](https://developer.tidal.com/)
3. Generate a cookie secret: `openssl rand -hex 32`
4. Edit `docker-compose.yml` and update the environment variables:
   - `TIDAL_CLIENT_ID` - your TIDAL API client ID
   - `TIDAL_CLIENT_SECRET` - your TIDAL API client secret
   - `TIDAL_REDIRECT_URI` - the OAuth callback URL (e.g., `http://localhost:3000/auth/callback` or your public domain)
   - `COOKIE_SECRET` - the 64-character hex string you generated
   - `BASE_URL` - your public URL (e.g., `https://yourdomain.com`)
5. Run: `docker-compose up -d`

The application will be available at http://localhost:3000 (or your configured domain).

### Environment Variables

All configuration is done via environment variables:

- `DATABASE_URL` - SQLite database path (default: `sqlite:///data/data.db`)
- `TIDAL_CLIENT_ID` - TIDAL API client ID (required)
- `TIDAL_CLIENT_SECRET` - TIDAL API client secret (required)
- `TIDAL_REDIRECT_URI` - OAuth redirect URI (required)
- `COOKIE_SECRET` - 64-character hex string for cookie encryption (required)
- `BASE_URL` - Base URL for the application (default: `http://localhost:3000`)
- `RUST_LOG` - Logging level (optional, e.g., `tidal_collaborative_playlists=debug`)

## Development setup

  * Clone the repo. You'll need a recent Rust environment.
  * Copy `.env.example` to `.env` and populate it. Get a Tidal client ID & secret [from here](https://developer.tidal.com/), and setup a redirect URL (e.g. to `http://localhost:3000/auth/callback`)
  * Run the app (`cargo run`)
    
The application uses SQLite via sqlx, but it should be possible to swap out to a different database to suit your needs.
