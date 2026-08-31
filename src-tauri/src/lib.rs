pub mod catalog;
pub mod cdn;
pub mod commands;
pub mod config;
pub mod hiker;
pub mod jobs;
pub mod library_commands;
pub mod library_protocol;
pub mod models;
pub mod network;
pub mod proxy;
pub mod scanner;
pub mod targets;

use std::sync::Arc;

use serde::Serialize;
use tauri::Manager;
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
use tokio::sync::{Mutex, RwLock};

use crate::catalog::Catalog;
use crate::config::Config;
use crate::hiker::{Balance, HikerClient};
use crate::jobs::{JobRegistry, ScanRegistry};
use crate::network::NetworkClients;

#[derive(Debug, Clone, Serialize)]
struct ConfigState {
    has_token: bool,
    token_hint: Option<String>,
    has_proxy: bool,
    proxy_hint: Option<String>,
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
            has_proxy: c.proxy_url.is_some(),
            proxy_hint: c.proxy_hint(),
            dest_dir: c.dest_dir.clone(),
            sidecar: c.sidecar,
            catalog_warning: None,
        }
    }
}

fn change_proxy(
    config: &mut Config,
    proxy_url: Option<String>,
    persist: impl FnOnce(&Config) -> Result<(), String>,
) -> Result<(ConfigState, NetworkClients), String> {
    let mut candidate = config.clone();
    candidate.proxy_url = crate::proxy::normalize_proxy_url(proxy_url.as_deref())?;
    let network = NetworkClients::from_config(&candidate)?;
    persist(&candidate).map_err(|_| "Proxy settings could not be saved".to_owned())?;
    *config = candidate;
    Ok((ConfigState::from(&*config), network))
}

fn commit_validated_token(
    config: &mut Config,
    token: String,
    persist: impl FnOnce(&Config) -> Result<(), String>,
) -> Result<ConfigState, String> {
    let mut candidate = config.clone();
    candidate.token = Some(token);
    persist(&candidate)?;
    *config = candidate;
    Ok(ConfigState::from(&*config))
}

fn network_from_loaded_config(config: &mut Config) -> NetworkClients {
    match NetworkClients::from_config(config) {
        Ok(network) => network,
        Err(error) => {
            eprintln!("{error}");
            config.proxy_url = None;
            NetworkClients::from_config(config).expect("direct network clients")
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
    let mut candidate = config.clone();
    if let Some(destination) = dest_dir {
        if !destination.trim().is_empty() {
            candidate.dest_dir = destination.trim().to_owned();
        }
    }
    if let Some(enabled) = sidecar {
        candidate.sidecar = enabled;
    }
    persist(&candidate)?;
    *config = candidate;

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

async fn save_settings_on_blocking_thread<Persist>(
    config: Arc<RwLock<Config>>,
    catalog: Catalog,
    dest_dir: Option<String>,
    sidecar: Option<bool>,
    persist: Persist,
) -> Result<ConfigState, String>
where
    Persist: FnOnce(&Config) -> Result<(), String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let mut config = config.blocking_write();
        save_settings_with_catalog(&mut config, dest_dir, sidecar, persist, &catalog)
    })
    .await
    .map_err(|_| "Settings could not be saved".to_owned())?
}

pub struct AppState {
    pub catalog: Catalog,
    cfg: Arc<RwLock<Config>>,
    network: RwLock<NetworkClients>,
    settings_update: Mutex<()>,
    pub jobs: Arc<JobRegistry>,
    pub scans: Arc<ScanRegistry>,
    /// Targets currently being downloaded — backend-side dedup so the same
    /// profile/post never runs twice concurrently, whatever the UI does.
    in_flight: Arc<std::sync::Mutex<std::collections::HashSet<String>>>,
}

impl AppState {
    pub(crate) async fn hiker_client(&self) -> Result<Arc<HikerClient>, String> {
        self.network
            .read()
            .await
            .hiker
            .clone()
            .ok_or_else(|| "No HikerAPI token configured".to_owned())
    }

    pub(crate) async fn cdn_client(&self) -> reqwest::Client {
        self.network.read().await.cdn.clone()
    }

    pub(crate) async fn download_clients(
        &self,
    ) -> Result<(Arc<HikerClient>, reqwest::Client), String> {
        let network = self.network.read().await;
        let hiker = network
            .hiker
            .clone()
            .ok_or_else(|| "No HikerAPI token configured".to_owned())?;
        Ok((hiker, network.cdn.clone()))
    }
}

fn err_string(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[tauri::command]
async fn config_state(state: tauri::State<'_, AppState>) -> Result<ConfigState, String> {
    let _update = state.settings_update.lock().await;
    let cfg = state.cfg.read().await;
    Ok(ConfigState::from(&*cfg))
}

#[tauri::command]
async fn validate_token(
    token: String,
    state: tauri::State<'_, AppState>,
) -> Result<Balance, String> {
    let _update = state.settings_update.lock().await;
    let token = token.trim().to_owned();
    let proxy_url = state.cfg.read().await.proxy_url.clone();
    let probe = HikerClient::with_proxy(token.clone(), proxy_url.as_deref())?;
    let balance = probe.balance().await.map_err(err_string)?;

    let config = Arc::clone(&state.cfg);
    tauri::async_runtime::spawn_blocking(move || {
        let mut config = config.blocking_write();
        commit_validated_token(&mut config, token, Config::save)
    })
    .await
    .map_err(|_| "Token could not be saved".to_owned())??;

    state.network.write().await.hiker = Some(Arc::new(probe));
    Ok(balance)
}

#[tauri::command]
async fn get_balance(state: tauri::State<'_, AppState>) -> Result<Balance, String> {
    let client = state.hiker_client().await?;
    client.balance().await.map_err(err_string)
}

#[tauri::command]
async fn set_proxy(
    proxy_url: Option<String>,
    state: tauri::State<'_, AppState>,
) -> Result<ConfigState, String> {
    let _update = state.settings_update.lock().await;
    let config = Arc::clone(&state.cfg);
    let (config_state, network) = tauri::async_runtime::spawn_blocking(move || {
        let mut config = config.blocking_write();
        change_proxy(&mut config, proxy_url, Config::save)
    })
    .await
    .map_err(|_| "Proxy settings could not be saved".to_owned())??;

    *state.network.write().await = network;
    Ok(config_state)
}

#[tauri::command]
async fn save_settings(
    dest_dir: Option<String>,
    sidecar: Option<bool>,
    state: tauri::State<'_, AppState>,
) -> Result<ConfigState, String> {
    let _update = state.settings_update.lock().await;
    save_settings_on_blocking_thread(
        Arc::clone(&state.cfg),
        state.catalog.clone(),
        dest_dir,
        sidecar,
        |config| config.save(),
    )
    .await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut cfg = Config::load();
    let network = network_from_loaded_config(&mut cfg);

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
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
                cfg: Arc::new(RwLock::new(cfg)),
                network: RwLock::new(network),
                settings_update: Mutex::new(()),
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
            set_proxy,
            save_settings,
            commands::resolve_input,
            commands::download_post,
            commands::fetch_profile,
            commands::fetch_reels,
            commands::enqueue_profile_download,
            commands::enqueue_fetched_post_download,
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
            library_commands::request_library_preview_access,
            library_commands::open_library_file,
            library_commands::reveal_library_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::sync::Arc;
    use std::time::Duration;

    use tempfile::tempdir;

    use super::{
        change_proxy, commit_validated_token, network_from_loaded_config,
        save_settings_on_blocking_thread, save_settings_with_catalog, Catalog, Config,
    };
    use tokio::sync::RwLock;

    #[test]
    fn change_proxy_normalizes_builds_and_persists_before_updating_runtime_config() {
        let mut config = Config {
            token: Some("test-token".to_owned()),
            ..Config::default()
        };
        let persisted = RefCell::new(None);

        let (state, network) = change_proxy(
            &mut config,
            Some("  http://alice:secret@127.0.0.1:8080  ".to_owned()),
            |candidate| {
                persisted.replace(Some(candidate.clone()));
                Ok(())
            },
        )
        .unwrap();

        assert_eq!(
            persisted.borrow().as_ref().unwrap().proxy_url.as_deref(),
            Some("http://alice:secret@127.0.0.1:8080/")
        );
        assert_eq!(
            config.proxy_url.as_deref(),
            Some("http://alice:secret@127.0.0.1:8080/")
        );
        assert!(state.has_proxy);
        let hint = state.proxy_hint.unwrap();
        assert!(hint.contains("***@"));
        assert!(!hint.contains("alice"));
        assert!(!hint.contains("secret"));
        assert!(network.hiker.is_some());
    }

    #[test]
    fn change_proxy_clear_returns_direct_clients_and_empty_proxy_state() {
        let mut config = Config {
            token: Some("test-token".to_owned()),
            proxy_url: Some("http://127.0.0.1:8080/".to_owned()),
            ..Config::default()
        };

        let (state, network) = change_proxy(&mut config, None, |_| Ok(())).unwrap();

        assert_eq!(config.proxy_url, None);
        assert!(!state.has_proxy);
        assert_eq!(state.proxy_hint, None);
        assert!(network.hiker.is_some());
    }

    #[test]
    fn change_proxy_persistence_failure_rolls_back_runtime_config() {
        let original_proxy = "http://127.0.0.1:8080/";
        let mut config = Config {
            proxy_url: Some(original_proxy.to_owned()),
            ..Config::default()
        };

        let error = change_proxy(
            &mut config,
            Some("http://127.0.0.1:9090".to_owned()),
            |_| Err("disk contains sensitive details".to_owned()),
        )
        .err()
        .unwrap();

        assert_eq!(error, "Proxy settings could not be saved");
        assert_eq!(config.proxy_url.as_deref(), Some(original_proxy));
    }

    #[test]
    fn change_proxy_invalid_value_skips_persistence_and_runtime_update() {
        let original_proxy = "http://127.0.0.1:8080/";
        let mut config = Config {
            proxy_url: Some(original_proxy.to_owned()),
            ..Config::default()
        };
        let persist_calls = std::cell::Cell::new(0);

        let error = change_proxy(
            &mut config,
            Some("http://alice:secret@127.0.0.1:8080/private".to_owned()),
            |_| {
                persist_calls.set(persist_calls.get() + 1);
                Ok(())
            },
        )
        .err()
        .unwrap();

        assert_eq!(
            error,
            "Enter a valid HTTP, HTTPS, SOCKS5, or SOCKS5H proxy URL"
        );
        assert_eq!(persist_calls.get(), 0);
        assert_eq!(config.proxy_url.as_deref(), Some(original_proxy));
    }

    #[test]
    fn commit_validated_token_persistence_failure_rolls_back_runtime_config() {
        let mut config = Config {
            token: Some("old-token".to_owned()),
            ..Config::default()
        };

        let error = commit_validated_token(&mut config, "new-token".to_owned(), |_| {
            Err("disk is full".to_owned())
        })
        .err()
        .unwrap();

        assert_eq!(error, "disk is full");
        assert_eq!(config.token.as_deref(), Some("old-token"));
    }

    #[test]
    fn commit_validated_token_persists_candidate_before_runtime_update() {
        let mut config = Config::default();
        let persisted = RefCell::new(None);

        let state = commit_validated_token(&mut config, "new-token".to_owned(), |candidate| {
            persisted.replace(candidate.token.clone());
            Ok(())
        })
        .unwrap();

        assert_eq!(persisted.borrow().as_deref(), Some("new-token"));
        assert_eq!(config.token.as_deref(), Some("new-token"));
        assert!(state.has_token);
    }

    #[test]
    fn invalid_loaded_proxy_is_cleared_in_memory_and_uses_direct_clients() {
        let mut config = Config {
            token: Some("test-token".to_owned()),
            proxy_url: Some("http://alice:secret@127.0.0.1/private".to_owned()),
            ..Config::default()
        };

        let network = network_from_loaded_config(&mut config);

        assert_eq!(config.proxy_url, None);
        assert!(network.hiker.is_some());
    }

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

    #[test]
    fn save_settings_persistence_failure_leaves_runtime_config_unchanged() {
        let directory = tempdir().unwrap();
        let catalog = Catalog::open(directory.path().join("catalog.sqlite3")).unwrap();
        let old_destination = directory.path().join("old-downloads");
        let new_destination = directory.path().join("new-downloads");
        let mut config = Config {
            dest_dir: old_destination.to_string_lossy().into_owned(),
            sidecar: true,
            ..Config::default()
        };

        let error = save_settings_with_catalog(
            &mut config,
            Some(new_destination.to_string_lossy().into_owned()),
            Some(false),
            |_| Err("config disk is full".to_owned()),
            &catalog,
        )
        .unwrap_err();

        assert_eq!(error, "config disk is full");
        assert_eq!(config.dest_dir, old_destination.to_string_lossy());
        assert!(config.sidecar);
        assert!(catalog.list_roots().unwrap().is_empty());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn save_settings_runs_blocking_work_off_async_worker() {
        let directory = tempdir().unwrap();
        let catalog = Catalog::open(directory.path().join("catalog.sqlite3")).unwrap();
        let destination = directory.path().join("downloads");
        let config = Arc::new(RwLock::new(Config::default()));
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();

        let save = save_settings_on_blocking_thread(
            Arc::clone(&config),
            catalog,
            Some(destination.to_string_lossy().into_owned()),
            None,
            move |_| {
                started_tx.send(()).unwrap();
                release_rx
                    .recv_timeout(Duration::from_secs(1))
                    .map_err(|_| "async worker did not make progress".to_owned())?;
                Ok(())
            },
        );
        let heartbeat = async {
            started_rx.await.unwrap();
            tokio::task::yield_now().await;
            release_tx.send(()).unwrap();
        };
        let (state, ()) = tokio::join!(save, heartbeat);

        assert_eq!(state.unwrap().dest_dir, destination.to_string_lossy());
    }
}
