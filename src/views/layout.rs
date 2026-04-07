use maud::{DOCTYPE, Markup, PreEscaped, html};

const COMMON_CSS: &str = "
    *, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }
    body {
        font-family: system-ui, sans-serif;
        background: #0a0a0a;
        color: #f0f0f0;
        min-height: 100vh;
    }
    .btn {
        display: inline-block;
        padding: 0.75rem 1.5rem;
        border-radius: 8px;
        font-size: 1rem;
        font-weight: 600;
        text-decoration: none;
        cursor: pointer;
        border: none;
        transition: opacity 0.15s;
    }
    .btn:hover { opacity: 0.85; }
    .btn-tidal { background: #00FFFF; color: #000; }
    .btn-secondary { background: #2a2a2a; color: #f0f0f0; }
";

const CENTERED_CSS: &str = "
    body {
        display: flex;
        align-items: center;
        justify-content: center;
    }
    .card {
        background: #1a1a1a;
        border: 1px solid #2a2a2a;
        border-radius: 12px;
        padding: 2.5rem;
        max-width: 420px;
        width: 100%;
        text-align: center;
    }
    h1 { font-size: 1.5rem; margin-bottom: 0.5rem; }
    p { color: #888; margin-bottom: 1.5rem; line-height: 1.5; }
    .meta { font-size: 0.85rem; color: #666; margin-top: 1.5rem; }
";

const DASHBOARD_CSS: &str = "
    body { padding: 2rem 1rem; }
    .page { max-width: 700px; margin: 0 auto; }
    h1 { font-size: 1.5rem; margin-bottom: 2rem; }
    h2 { font-size: 1.1rem; color: #888; margin-bottom: 1rem; font-weight: 500; }

    /* Filter / sort controls */
    #controls {
        display: flex;
        align-items: center;
        gap: 0.5rem;
        margin-bottom: 1.5rem;
    }
    #q-input {
        flex: 1;
        background: #1a1a1a;
        border: 1px solid #2a2a2a;
        border-radius: 6px;
        padding: 0.5rem 0.75rem;
        color: #f0f0f0;
        font-size: 0.9rem;
    }
    #q-input:focus { outline: none; border-color: #444; }
    #clear-btn {
        background: none;
        border: none;
        color: #666;
        cursor: pointer;
        font-size: 1rem;
        padding: 0.25rem 0.5rem;
        line-height: 1;
    }
    #clear-btn:hover { color: #f0f0f0; }
    #q-input:placeholder-shown + #clear-btn { display: none; }
    #sort-select {
        background: #1a1a1a;
        border: 1px solid #2a2a2a;
        border-radius: 6px;
        padding: 0.5rem 0.75rem;
        color: #f0f0f0;
        font-size: 0.9rem;
        cursor: pointer;
    }
    #sort-select:focus { outline: none; border-color: #444; }

    /* Playlist list */
    .playlist-list { list-style: none; }
    .playlist-item:not(:last-of-type) {
        border-bottom: 1px solid #1e1e1e;
    }
    .playlist-row {
        display: flex;
        align-items: center;
        padding: 0.85rem 0;
        gap: 1rem;
    }
    .playlist-cover {
        flex-shrink: 0;
        width: 32px;
        height: 32px;
        border-radius: 4px;
        object-fit: cover;
    }
    .playlist-cover--empty {
        background: #1e1e1e;
    }
    .playlist-name {
        flex: 1;
        min-width: 0;
        font-weight: 600;
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
    }
    .playlist-meta {
        font-size: 0.8rem;
        color: #666;
        display: flex;
        align-items: center;
        gap: 0;
    }
    .sync-form {
        display: inline;
        margin: 0 0 0 5px;
    }
    .sync-link {
        background: none;
        border: none;
        color: #00FFFF;
        cursor: pointer;
        font-size: 0.8rem;
        padding: 0;
        text-decoration: none;
        transition: opacity 0.15s;
    }
    .sync-link:hover { opacity: 0.7; }

    /* Toggle switch */
    .toggle-switch {
        position: relative;
        flex-shrink: 0;
        width: 44px;
        height: 26px;
        border-radius: 13px;
        border: none;
        cursor: pointer;
        background: #2a2a2a;
        padding: 0;
        transition: background 0.2s;
    }
    .toggle-switch::before {
        content: '';
        position: absolute;
        top: 3px;
        left: 3px;
        width: 20px;
        height: 20px;
        border-radius: 50%;
        background: #555;
        transition: transform 0.2s, background 0.2s;
    }
    .toggle-switch:hover { background: #333; }
    .toggle-switch.active { background: rgba(0,255,255,0.15); outline: 1px solid #00FFFF; }
    .toggle-switch.active::before { transform: translateX(18px); background: #00FFFF; }

    /* Expanded shared section */
    .playlist-expanded {
        padding: 0 0 1rem;
        display: flex;
        flex-direction: column;
        gap: 0.5rem;
    }
    .invite-url {
        font-family: ui-monospace, monospace;
        font-size: 0.8rem;
        background: #141414;
        border: 1px solid #2a2a2a;
        border-radius: 6px;
        padding: 0.5rem 0.75rem;
        word-break: break-all;
        user-select: all;
        color: #aaa;
    }


    /* Footer */
    footer {
        margin-top: 3rem;
        padding-top: 1.5rem;
        border-top: 1px solid #1e1e1e;
        color: #555;
        font-size: 0.85rem;
    }
    .logout-btn {
        background: none;
        border: none;
        color: #555;
        cursor: pointer;
        font-size: 0.85rem;
        padding: 0;
        text-decoration: underline;
    }
    .logout-btn:hover { color: #888; }
";

fn shell(title: &str, extra_css: &str, content: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (title) " — TIDAL Collaborative Playlists" }
                script src="https://unpkg.com/htmx.org@2.0.4" integrity="sha384-HGfztofotfshcF7+8n44JQL2oJmowVChPTg48S+jvZoztPfvwD79OC/LTtG6dMp+" crossorigin="anonymous" {}
                style { (PreEscaped(COMMON_CSS)) (PreEscaped(extra_css)) }
            }
            body {
                (content)
            }
        }
    }
}

pub fn layout(title: &str, content: Markup) -> Markup {
    shell(title, CENTERED_CSS, content)
}

pub fn dashboard_layout(title: &str, content: Markup) -> Markup {
    shell(title, DASHBOARD_CSS, content)
}
