mod config;
mod hiker;

use std::sync::Arc;

use serde::Serialize;
use tokio::sync::RwLock;

use crate::config::Config;
use crate::hiker::{Balance, HikerClient};

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

struct AppState {
    cfg: RwLock<Config>,
    client: RwLock<Option<Arc<HikerClient>>>,
}

fn err_string(e: impl std::fmt::Display) -> String {
    e.to_string()
}

async fn require_client(state: &AppState) -> Result<Arc<HikerClient>, String> {
    state
        .client
        .read()
        .await
        .clone()
        .ok_or_else(|| "No HikerAPI token configured".to_string())
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
    let client = require_client(&state).await?;
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
    let client = cfg.token.as_ref().map(|t| Arc::new(HikerClient::new(t.clone())));

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            cfg: RwLock::new(cfg),
            client: RwLock::new(client),
        })
        .invoke_handler(tauri::generate_handler![
            config_state,
            validate_token,
            get_balance,
            save_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
