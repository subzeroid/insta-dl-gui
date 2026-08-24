pub mod catalog;
pub mod cdn;
pub mod commands;
pub mod config;
pub mod hiker;
pub mod jobs;
pub mod library_commands;
pub mod library_protocol;
pub mod models;
pub mod scanner;
pub mod targets;

use std::sync::Arc;

use serde::Serialize;
use tauri::Manager;
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
use tokio::sync::RwLock;

use crate::catalog::Catalog;
use crate::config::Config;
use crate::hiker::{Balance, HikerClient};
use crate::jobs::{JobRegistry, ScanRegistry};

#[derive(Debug, Clone, Serialize)]
struct ConfigState {
    has_token: bool,
    token_hint: Option<String>,
    dest_dir: String,
    sidecar: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    catalog_warning: Option<String>,
}

impl From<&Config> for ConfigState {
    fn from(c: &Config) -> Self {
        Self {
            has_token: c.token.is_some(),
            token_hint: c.token_hint(),
            dest_dir: c.dest_dir.clone(),
            sidecar: c.sidecar,
            catalog_warning: None,
        }
    }
}

fn save_settings_with_catalog(
    config: &mut Config,
    dest_dir: Option<String>,
    sidecar: Option<bool>,
    persist: impl FnOnce(&Config) -> Result<(), String>,
    catalog: &Catalog,
) -> Result<ConfigState, String> {
    if let Some(destination) = dest_dir {
        if !destination.trim().is_empty() {
            config.dest_dir = destination.trim().to_owned();
        }
    }
    if let Some(enabled) = sidecar {
        config.sidecar = enabled;
    }
    persist(config)?;

    let mut state = ConfigState::from(&*config);
    if catalog
        .register_root(std::path::Path::new(&config.dest_dir), "Downloads")
        .is_err()
    {
        state.catalog_warning = Some(
            "Settings were saved, but the download folder could not be added to the Library. Open Library and rescan after fixing the folder."
                .to_owned(),
        );
    }
    Ok(state)
}

pub struct AppState {
    pub catalog: Catalog,
    cfg: RwLock<Config>,
    client: RwLock<Option<Arc<HikerClient>>>,
    /// Separate HTTP client for CDN downloads: redirects are followed
    /// manually by `cdn.rs` so every hop gets validated.
    cdn_http: reqwest::Client,
    pub jobs: Arc<JobRegistry>,
    pub scans: Arc<ScanRegistry>,
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
    save_settings_with_catalog(
        &mut cfg,
        dest_dir,
        sidecar,
        |config| config.save(),
        &state.catalog,
    )
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
        .register_asynchronous_uri_scheme_protocol("library", |context, request, responder| {
            let catalog = context.app_handle().state::<AppState>().catalog.clone();
            let webview_label = context.webview_label().to_owned();
            tauri::async_runtime::spawn(async move {
                let response =
                    library_protocol::handle_library_protocol(catalog, &webview_label, request)
                        .await;
                responder.respond(response);
            });
        })
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
                scans: Arc::new(ScanRegistry::new()),
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
            library_commands::ensure_configured_library_root,
            library_commands::list_library_roots,
            library_commands::start_library_scan,
            library_commands::cancel_library_scan,
            library_commands::query_library,
            library_commands::get_library_item,
            library_commands::open_library_file,
            library_commands::reveal_library_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use tempfile::tempdir;

    use super::{save_settings_with_catalog, Catalog, Config};

    #[test]
    fn save_settings_registers_library_root_and_preserves_previous_root() {
        let directory = tempdir().unwrap();
        let catalog = Catalog::open(directory.path().join("catalog.sqlite3")).unwrap();
        let old_destination = directory.path().join("old-downloads");
        let new_destination = directory.path().join("new-downloads");
        catalog.register_root(&old_destination, "Previous").unwrap();
        let mut config = Config {
            dest_dir: old_destination.to_string_lossy().into_owned(),
            ..Config::default()
        };

        let state = save_settings_with_catalog(
            &mut config,
            Some(new_destination.to_string_lossy().into_owned()),
            None,
            |_| Ok(()),
            &catalog,
        )
        .unwrap();

        assert_eq!(state.dest_dir, new_destination.to_string_lossy());
        assert_eq!(state.catalog_warning, None);
        let roots = catalog.list_roots().unwrap();
        let old_destination = old_destination.canonicalize().unwrap();
        let new_destination = new_destination.canonicalize().unwrap();
        assert_eq!(roots.len(), 2);
        assert!(roots.iter().any(|root| root.path == old_destination));
        assert!(roots
            .iter()
            .any(|root| root.path == new_destination && root.label == "Downloads"));
    }

    #[test]
    fn save_settings_registers_library_root_failure_without_rolling_back_config() {
        let directory = tempdir().unwrap();
        let catalog = Catalog::open(directory.path().join("catalog.sqlite3")).unwrap();
        let blocked_destination = directory.path().join("not-a-directory");
        std::fs::write(&blocked_destination, b"file blocks root creation").unwrap();
        let mut config = Config::default();
        let persisted = RefCell::new(None);

        let state = save_settings_with_catalog(
            &mut config,
            Some(blocked_destination.to_string_lossy().into_owned()),
            None,
            |saved| {
                persisted.replace(Some(saved.clone()));
                Ok(())
            },
            &catalog,
        )
        .unwrap();

        assert_eq!(config.dest_dir, blocked_destination.to_string_lossy());
        assert_eq!(
            persisted.borrow().as_ref().unwrap().dest_dir,
            blocked_destination.to_string_lossy()
        );
        assert!(state
            .catalog_warning
            .as_deref()
            .is_some_and(|warning| { warning.contains("Library") && warning.contains("saved") }));
        assert!(catalog.list_roots().unwrap().is_empty());
    }
}
