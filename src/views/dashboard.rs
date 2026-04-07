use maud::{Markup, html};

use crate::db::playlists::SharedInfo;

use super::dashboard_layout;

pub struct PlaylistRow {
    pub tidal_id: String,
    pub title: String,
    pub item_count: u32,
    pub shared: Option<SharedInfo>,
}

pub fn page(rows: &[PlaylistRow], base_url: &str, q: Option<&str>, sort: Option<&str>) -> Markup {
    dashboard_layout(
        "Dashboard",
        html! {
            div class="page" {
                h1 { "Playlists" }
                form id="controls"
                     hx-get="/"
                     hx-select="#playlist-list-wrap"
                     hx-target="#playlist-list-wrap"
                     hx-trigger="input delay:50ms from:#q-input, change from:#sort-select" {
                    input id="q-input" name="q" type="text"
                          placeholder="Filter playlists…"
                          value=(q.unwrap_or(""))
                          autocomplete="off";
                    button id="clear-btn" type="button"
                           onclick="document.getElementById('q-input').value=''; htmx.trigger('#controls', 'submit')" {
                        "×"
                    }
                    select id="sort-select" name="sort" {
                        option value="modified" selected[sort.is_none() || sort == Some("modified")] { "Last modified" }
                        option value="name"     selected[sort == Some("name")]  { "Name" }
                        option value="count"    selected[sort == Some("count")] { "Track count" }
                    }
                }
                div id="playlist-list-wrap" {
                    ul class="playlist-list" {
                        @for row in rows {
                            (playlist_row(row, base_url))
                        }
                    }
                }
                footer {
                    form hx-post="/auth/logout" hx-push-url="true" {
                        button class="logout-btn" { "Logout" }
                    }
                }
            }
        },
    )
}

pub fn playlist_row(row: &PlaylistRow, base_url: &str) -> Markup {
    let elem_id = format!("playlist-{}", row.tidal_id);
    let (action, is_active) = match &row.shared {
        None => (format!("/playlists/{}/share", row.tidal_id), false),
        Some(s) if s.is_owner => (format!("/playlists/{}/unshare", row.tidal_id), true),
        Some(_) => (format!("/playlists/{}/leave", row.tidal_id), true),
    };
    html! {
        li id=(elem_id) class="playlist-item" {
            div class="playlist-row" {
                div class="playlist-name" { (row.title) }
                form hx-post=(action)
                     hx-target={"#" (elem_id)}
                     hx-swap="outerHTML" {
                    input type="hidden" name="name" value=(row.title);
                    input type="hidden" name="item_count" value=(row.item_count);
                    @if is_active {
                        button class="toggle-switch active" type="submit" title="Leave" {}
                    } @else {
                        button class="toggle-switch" type="submit" title="Share" {}
                    }
                }
            }
            @if let Some(shared) = &row.shared {
                div class="playlist-expanded" {
                    div class="playlist-meta" {
                        (row.item_count)
                        @if row.item_count == 1 { " track · " } @else { " tracks · " }
                        (shared.member_count)
                        @if shared.member_count == 1 { " member" } @else { " members" }
                        " · "
                        form class="sync-form" hx-post={"/playlists/" (shared.playlist_id) "/sync"}
                             hx-target={"#" (elem_id)}
                             hx-swap="outerHTML" {
                            input type="hidden" name="tidal_id" value=(row.tidal_id);
                            input type="hidden" name="name" value=(row.title);
                            input type="hidden" name="item_count" value=(row.item_count);
                            button class="sync-link" type="submit" { "Sync now" }
                        }
                    }
                    @if shared.is_owner {
                        div class="invite-row" {
                            div class="invite-url" {
                                (base_url) "/join/" (shared.playlist_id)
                            }
                        }
                    }
                }
            }
        }
    }
}
