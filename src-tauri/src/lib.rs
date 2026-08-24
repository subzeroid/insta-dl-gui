pub mod catalog;
pub mod cdn;
pub mod commands;
pub mod config;
pub mod hiker;
pub mod jobs;
pub mod models;
pub mod targets;

use std::sync::Arc;

use serde::Serialize;
use tauri::Manager;
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
use tokio::sync::RwLock;

use crate::catalog::Catalog;
use crate::config::Config;
use crate::hiker::{Balance, HikerClient};
use crate::jobs::JobRegistry;

#[derive(Debug, Clone, Serialize)]
struct ConfigState {
    has_token: bool,
    token_hint: Option<String>,
    dest_dir: String,
    sidecar: bool,
}

impl From<&Config> for ConfigState {
    fn from(c: &Config) -> Self {
        Self {
            has_token: c.token.is_some(),
            token_hint: c.token_hint(),
            dest_dir: c.dest_dir.clone(),
            sidecar: c.sidecar,
        }
    }
}

pub struct AppState {
    pub catalog: Catalog,
    cfg: RwLock<Config>,
    client: RwLock<Option<Arc<HikerClient>>>,
    /// Separate HTTP client for CDN downloads: redirects are followed
    /// manually by `cdn.rs` so every hop gets validated.
    cdn_http: reqwest::Client,
    pub jobs: Arc<JobRegistry>,
    /// Targets currently being downloaded — backend-side dedup so the same
    /// profile/post never runs twice concurrently, whatever the UI does.
    in_flight: Arc<std::sync::Mutex<std::collections::HashSet<String>>>,
}

fn err_string(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[tauri::command]
async fn config_state(state: tauri::State<'_, AppState>) -> Result<ConfigState, String> {
    let cfg = state.cfg.read().await;
    Ok(ConfigState::from(&*cfg))
}

#[tauri::command]
async fn validate_token(
    token: String,
    state: tauri::State<'_, AppState>,
) -> Result<Balance, String> {
    let probe = HikerClient::new(token.trim().to_string());
    let balance = probe.balance().await.map_err(err_string)?;

    {
        let mut cfg = state.cfg.write().await;
        cfg.token = Some(token.trim().to_string());
        cfg.save().map_err(err_string)?;
    }
    *state.client.write().await = Some(Arc::new(probe));
    Ok(balance)
}

#[tauri::command]
async fn get_balance(state: tauri::State<'_, AppState>) -> Result<Balance, String> {
    let client = state
        .client
        .read()
        .await
        .clone()
        .ok_or_else(|| "No HikerAPI token configured".to_string())?;
    client.balance().await.map_err(err_string)
}

#[tauri::command]
async fn save_settings(
    dest_dir: Option<String>,
    sidecar: Option<bool>,
    state: tauri::State<'_, AppState>,
) -> Result<ConfigState, String> {
    let mut cfg = state.cfg.write().await;
    if let Some(d) = dest_dir {
        if !d.trim().is_empty() {
            cfg.dest_dir = d.trim().to_string();
        }
    }
    if let Some(s) = sidecar {
        cfg.sidecar = s;
    }
    cfg.save().map_err(err_string)?;
    Ok(ConfigState::from(&*cfg))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let cfg = Config::load();
    let client = cfg
        .token
        .as_ref()
        .map(|t| Arc::new(HikerClient::new(t.clone())));
    let cdn_http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("cdn http client");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            let catalog_path = match dirs::data_dir() {
                Some(data_dir) => data_dir.join("insta-dl-gui/catalog.sqlite3"),
                None => {
                    let message =
                        "Failed to initialize catalog: no platform data directory is available";
                    eprintln!("{message}");
                    app.dialog()
                        .message(message)
                        .title("Catalog initialization failed")
                        .kind(MessageDialogKind::Error)
                        .blocking_show();
                    return Err(std::io::Error::other(message).into());
                }
            };
            let catalog = match Catalog::open(&catalog_path) {
                Ok(catalog) => catalog,
                Err(error) => {
                    let message = format!(
                        "Failed to initialize catalog at {}: {error}",
                        catalog_path.display()
                    );
                    eprintln!("{message}");
                    app.dialog()
                        .message(&message)
                        .title("Catalog initialization failed")
                        .kind(MessageDialogKind::Error)
                        .blocking_show();
                    return Err(std::io::Error::other(message).into());
                }
            };

            app.manage(AppState {
                catalog,
                cfg: RwLock::new(cfg),
                client: RwLock::new(client),
                cdn_http,
                jobs: Arc::new(JobRegistry::new()),
                in_flight: Arc::new(std::sync::Mutex::new(std::collections::HashSet::new())),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            config_state,
            validate_token,
            get_balance,
            save_settings,
            commands::resolve_input,
            commands::download_post,
            commands::fetch_profile,
            commands::enqueue_profile_download,
            commands::cancel_job,
            commands::search_users,
            commands::fetch_stories,
            commands::download_direct,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
