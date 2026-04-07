use maud::{Markup, html};

use super::layout;

pub fn page() -> Markup {
    layout(
        "Login",
        html! {
            div class="card" {
                h1 { "TIDAL Collaborative Playlists" }
                p { "Keep shared playlists in sync across multiple TIDAL accounts." }
                a href="/auth/login" class="btn btn-tidal" { "Connect with TIDAL" }
            }
        },
    )
}
