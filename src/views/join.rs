use maud::{Markup, html};

use super::layout;

pub fn page(playlist_name: &str) -> Markup {
    layout(
        "Join Playlist",
        html! {
            div class="card" {
                h1 { "Join playlist" }
                p { "You've been invited to collaborate. Choose a name for your copy of this playlist:" }
                form method="post" {
                    input
                        type="text"
                        name="name"
                        value=(playlist_name)
                        style="width:100%;padding:0.6rem 0.75rem;background:#0a0a0a;border:1px solid #2a2a2a;border-radius:6px;color:#f0f0f0;font-size:1rem;margin-bottom:1rem;"
                        autofocus;
                    button type="submit" class="btn btn-tidal" style="width:100%;" {
                        "Join playlist"
                    }
                }
            }
        },
    )
}
