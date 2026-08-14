use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Html;
use axum::routing::{get, post};
use axum::{Form, Router};
use serde::Deserialize;
use tauri::{AppHandle, Emitter};
use tower_http::cors::{Any, CorsLayer};

use crate::services::epic;
use ugly_core::store::Store;

/// Fixed so the bookmarklet's URL stays valid across restarts. Kept off the common
/// dev-server ports to avoid clashing with anything else the user runs.
pub const PORT: u16 = 43117;

#[derive(Clone)]
struct AppState {
    store: Arc<Store>,
    app: AppHandle,
}

#[derive(Deserialize)]
struct ImportForm {
    data: String,
}

#[derive(Deserialize)]
struct SteamTokenForm {
    token: String,
}

fn result_page(title: &str, message: &str, accent: &str) -> Html<String> {
    Html(format!(
        r#"<!doctype html>
<html><head><meta charset="utf-8"><title>UGLy — {title}</title></head>
<body style="margin:0;min-height:100vh;display:flex;align-items:center;justify-content:center;
             background:#16171d;color:#e8e8ea;font-family:system-ui,'Segoe UI',Roboto,sans-serif;text-align:center">
  <div>
    <h1 style="margin:0 0 .5rem;font-size:1.6rem;color:{accent}">{title}</h1>
    <p style="margin:0 0 1.25rem;opacity:.75">{message}</p>
    <p style="opacity:.5;font-size:.85rem">You can close this tab and switch back to UGLy.</p>
  </div>
</body></html>"#
    ))
}

async fn import(
    State(state): State<AppState>,
    Form(form): Form<ImportForm>,
) -> (StatusCode, Html<String>) {
    match epic::parse_export(&form.data) {
        Ok(games) => {
            let count = games.len();
            let imported_at = crate::services::igdb::now_ms();
            if let Err(err) = state.store.replace_epic_games(&games, imported_at) {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    result_page("Import failed", &err.to_string(), "#ff6b6b"),
                );
            }
            // Tells the running app to refresh without the user doing anything.
            let _ = state.app.emit("epic-imported", count);
            (
                StatusCode::OK,
                result_page(
                    "Imported!",
                    &format!("{count} games are now in your library."),
                    "#a855f7",
                ),
            )
        }
        Err(err) => (
            StatusCode::BAD_REQUEST,
            result_page("Import failed", &err.to_string(), "#ff6b6b"),
        ),
    }
}

/// Receives a `webapi_token` scraped from a logged-in Steam web session. Steam's family
/// endpoints won't accept a Web API key, and the token is short-lived, so it is used
/// immediately and never written to disk.
async fn steam_family(
    State(state): State<AppState>,
    Form(form): Form<SteamTokenForm>,
) -> (StatusCode, Html<String>) {
    let steam_id = match state.store.get_state("steam_id") {
        Ok(Some(id)) if !id.trim().is_empty() => id,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                result_page(
                    "Steam not set up",
                    "Add your SteamID64 in UGLy's Settings first.",
                    "#ff6b6b",
                ),
            )
        }
    };

    match crate::services::steam_family::fetch_family_library(form.token.trim(), &steam_id).await {
        Ok(games) => {
            let count = games.len();
            if let Err(err) = state
                .store
                .replace_family_games(&games, crate::services::igdb::now_ms())
            {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    result_page("Import failed", &err.to_string(), "#ff6b6b"),
                );
            }
            let _ = state.app.emit("family-imported", count);
            (
                StatusCode::OK,
                result_page(
                    "Family library imported!",
                    &format!("{count} shared games added to your library."),
                    "#a855f7",
                ),
            )
        }
        Err(err) => (
            StatusCode::BAD_REQUEST,
            result_page("Import failed", &err.to_string(), "#ff6b6b"),
        ),
    }
}

pub fn spawn(app: AppHandle, store: Arc<Store>) {
    tauri::async_runtime::spawn(async move {
        let state = AppState { store, app };
        let router = Router::new()
            .route("/health", get(|| async { "ok" }))
            .route("/api/epic/import", post(import))
            .route("/api/steam/family", post(steam_family))
            .layer(
                CorsLayer::new()
                    .allow_origin(Any)
                    .allow_methods(Any)
                    .allow_headers(Any),
            )
            .with_state(state);

        // Loopback only — never exposed beyond this machine.
        match tokio::net::TcpListener::bind(("127.0.0.1", PORT)).await {
            Ok(listener) => {
                log::info!("Bookmarklet listener ready on http://127.0.0.1:{PORT}");
                if let Err(err) = axum::serve(listener, router).await {
                    log::error!("Bookmarklet listener stopped: {err}");
                }
            }
            Err(err) => log::error!("Could not bind port {PORT} for the bookmarklet: {err}"),
        }
    });
}
