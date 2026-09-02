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
mod remote_media_protocol;
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

const TOKEN_VALIDATION_ATTEMPTS: usize = 3;
const NETWORK_CHANGED_DURING_TOKEN_VALIDATION: &str =
    "Network settings changed while validating the token; try again";
const TOKEN_REQUEST_SUPERSEDED: &str = "A newer token replacement request superseded this one";

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
    match crate::proxy::normalize_proxy_url(config.proxy_url.as_deref()) {
        Ok(proxy_url) => config.proxy_url = proxy_url,
        Err(error) => {
            eprintln!("{error}");
            config.proxy_url = None;
        }
    }

    match NetworkClients::from_config(config) {
        Ok(network) => network,
        Err(error) => {
            eprintln!("{error}");
            config.proxy_url = None;
            NetworkClients::from_config(config).expect("direct network clients")
        }
    }
}

async fn validate_with_consistent_proxy<
    Snapshot,
    SnapshotFuture,
    Validate,
    ValidateFuture,
    Commit,
    CommitFuture,
    Candidate,
    Output,
>(
    mut snapshot_proxy: Snapshot,
    mut validate: Validate,
    mut commit_if_current: Commit,
) -> Result<Output, String>
where
    Snapshot: FnMut() -> SnapshotFuture,
    SnapshotFuture: std::future::Future<Output = Option<String>>,
    Validate: FnMut(Option<String>) -> ValidateFuture,
    ValidateFuture: std::future::Future<Output = Result<Candidate, String>>,
    Commit: FnMut(Option<String>, Candidate) -> CommitFuture,
    CommitFuture: std::future::Future<Output = Result<Option<Output>, String>>,
{
    for _ in 0..TOKEN_VALIDATION_ATTEMPTS {
        let proxy_url = snapshot_proxy().await;
        let candidate = validate(proxy_url.clone()).await?;
        if let Some(output) = commit_if_current(proxy_url, candidate).await? {
            return Ok(output);
        }
    }

    Err(NETWORK_CHANGED_DURING_TOKEN_VALIDATION.to_owned())
}

async fn begin_token_request(settings_update: &Mutex<u64>) -> u64 {
    let mut generation = settings_update.lock().await;
    *generation = generation.wrapping_add(1);
    *generation
}

fn token_commit_is_current(
    current_generation: u64,
    request_generation: u64,
    current_proxy: &Option<String>,
    validated_proxy: &Option<String>,
) -> Result<bool, String> {
    if current_generation != request_generation {
        return Err(TOKEN_REQUEST_SUPERSEDED.to_owned());
    }
    Ok(current_proxy == validated_proxy)
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

fn spawn_proxy_update<Persist>(
    settings_update: Arc<Mutex<u64>>,
    config: Arc<RwLock<Config>>,
    network: Arc<RwLock<NetworkClients>>,
    proxy_url: Option<String>,
    persist: Persist,
) -> tauri::async_runtime::JoinHandle<Result<ConfigState, String>>
where
    Persist: FnOnce(&Config) -> Result<(), String> + Send + 'static,
{
    tauri::async_runtime::spawn(async move {
        let _update = settings_update.lock().await;
        let (config_state, candidate_network) = tauri::async_runtime::spawn_blocking(move || {
            let mut config = config.blocking_write();
            change_proxy(&mut config, proxy_url, persist)
        })
        .await
        .map_err(|_| "Proxy settings could not be saved".to_owned())??;

        *network.write().await = candidate_network;
        Ok(config_state)
    })
}

#[allow(clippy::too_many_arguments)]
fn spawn_token_commit<Persist>(
    settings_update: Arc<Mutex<u64>>,
    request_generation: u64,
    validated_proxy: Option<String>,
    config: Arc<RwLock<Config>>,
    network: Arc<RwLock<NetworkClients>>,
    token: String,
    probe: HikerClient,
    balance: Balance,
    persist: Persist,
) -> tauri::async_runtime::JoinHandle<Result<Option<Balance>, String>>
where
    Persist: FnOnce(&Config) -> Result<(), String> + Send + 'static,
{
    tauri::async_runtime::spawn(async move {
        let current_generation = settings_update.lock().await;
        let current_proxy = config.read().await.proxy_url.clone();
        if !token_commit_is_current(
            *current_generation,
            request_generation,
            &current_proxy,
            &validated_proxy,
        )? {
            return Ok(None);
        }

        tauri::async_runtime::spawn_blocking(move || {
            let mut config = config.blocking_write();
            commit_validated_token(&mut config, token, persist)
        })
        .await
        .map_err(|_| "Token could not be saved".to_owned())??;

        network.write().await.hiker = Some(Arc::new(probe));
        Ok(Some(balance))
    })
}

fn spawn_save_settings_update<Persist>(
    settings_update: Arc<Mutex<u64>>,
    config: Arc<RwLock<Config>>,
    catalog: Catalog,
    dest_dir: Option<String>,
    sidecar: Option<bool>,
    persist: Persist,
) -> tauri::async_runtime::JoinHandle<Result<ConfigState, String>>
where
    Persist: FnOnce(&Config) -> Result<(), String> + Send + 'static,
{
    tauri::async_runtime::spawn(async move {
        let _update = settings_update.lock().await;
        save_settings_on_blocking_thread(config, catalog, dest_dir, sidecar, persist).await
    })
}

pub struct AppState {
    pub catalog: Catalog,
    cfg: Arc<RwLock<Config>>,
    network: Arc<RwLock<NetworkClients>>,
    settings_update: Arc<Mutex<u64>>,
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
    let token = token.trim().to_owned();
    let app_state = state.inner();
    let request_generation = begin_token_request(&app_state.settings_update).await;
    validate_with_consistent_proxy(
        || {
            let config = Arc::clone(&app_state.cfg);
            async move { config.read().await.proxy_url.clone() }
        },
        |proxy_url| {
            let token = token.clone();
            async move {
                let probe = HikerClient::with_proxy(token, proxy_url.as_deref())?;
                let balance = probe.balance().await.map_err(err_string)?;
                Ok((balance, probe))
            }
        },
        |proxy_url, (balance, probe)| {
            let token = token.clone();
            let settings_update = Arc::clone(&app_state.settings_update);
            let config = Arc::clone(&app_state.cfg);
            let network = Arc::clone(&app_state.network);
            async move {
                spawn_token_commit(
                    settings_update,
                    request_generation,
                    proxy_url,
                    config,
                    network,
                    token,
                    probe,
                    balance,
                    Config::save,
                )
                .await
                .map_err(|_| "Token could not be saved".to_owned())?
            }
        },
    )
    .await
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
    spawn_proxy_update(
        Arc::clone(&state.settings_update),
        Arc::clone(&state.cfg),
        Arc::clone(&state.network),
        proxy_url,
        Config::save,
    )
    .await
    .map_err(|_| "Proxy settings could not be saved".to_owned())?
}

#[tauri::command]
async fn save_settings(
    dest_dir: Option<String>,
    sidecar: Option<bool>,
    state: tauri::State<'_, AppState>,
) -> Result<ConfigState, String> {
    spawn_save_settings_update(
        Arc::clone(&state.settings_update),
        Arc::clone(&state.cfg),
        state.catalog.clone(),
        dest_dir,
        sidecar,
        Config::save,
    )
    .await
    .map_err(|_| "Settings could not be saved".to_owned())?
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
        .register_asynchronous_uri_scheme_protocol("remote-media", |context, request, responder| {
            let app_handle = context.app_handle().clone();
            let webview_label = context.webview_label().to_owned();
            tauri::async_runtime::spawn(async move {
                // Snapshot the current client per protocol request. Settings updates affect
                // subsequent requests, while this request keeps its existing proxy routing.
                let client = app_handle.state::<AppState>().cdn_client().await;
                let response = remote_media_protocol::handle_remote_media_protocol(
                    client,
                    &webview_label,
                    request,
                )
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
                network: Arc::new(RwLock::new(network)),
                settings_update: Arc::new(Mutex::new(0)),
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
            commands::fetch_profile_summary,
            commands::fetch_reels,
            commands::fetch_relationships,
            commands::check_download_statuses,
            commands::enqueue_profile_download,
            commands::enqueue_fetched_post_download,
            commands::cancel_job,
            commands::search_users,
            commands::search_relationships,
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
        begin_token_request, change_proxy, commit_validated_token, network_from_loaded_config,
        save_settings_on_blocking_thread, save_settings_with_catalog, spawn_proxy_update,
        spawn_token_commit, token_commit_is_current, validate_with_consistent_proxy, Balance,
        Catalog, Config, HikerClient,
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

    #[tokio::test]
    async fn token_validation_does_not_hold_settings_gate_while_pending() {
        let gate = Arc::new(tokio::sync::Mutex::new(()));
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let started_tx = Arc::new(std::sync::Mutex::new(Some(started_tx)));
        let release_rx = Arc::new(tokio::sync::Mutex::new(Some(release_rx)));
        let validation_gate = Arc::clone(&gate);

        let transaction = tokio::spawn(validate_with_consistent_proxy(
            || std::future::ready(None),
            move |_| {
                let started_tx = Arc::clone(&started_tx);
                let release_rx = Arc::clone(&release_rx);
                async move {
                    started_tx.lock().unwrap().take().unwrap().send(()).unwrap();
                    release_rx.lock().await.take().unwrap().await.unwrap();
                    Ok(())
                }
            },
            move |_, candidate| {
                let validation_gate = Arc::clone(&validation_gate);
                async move {
                    let _update = validation_gate.lock().await;
                    Ok(Some(candidate))
                }
            },
        ));

        started_rx.await.unwrap();
        let update = tokio::time::timeout(Duration::from_secs(1), gate.lock())
            .await
            .expect("settings gate should be available during validation");
        drop(update);
        release_tx.send(()).unwrap();

        transaction.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn token_validation_retries_when_proxy_changes_before_commit() {
        let old_proxy = Some("http://127.0.0.1:8080/".to_owned());
        let new_proxy = Some("http://127.0.0.1:9090/".to_owned());
        let current_proxy = Arc::new(std::sync::Mutex::new(old_proxy.clone()));
        let validations = Arc::new(std::sync::Mutex::new(Vec::new()));
        let commits = Arc::new(std::sync::Mutex::new(Vec::new()));

        let snapshot_proxy = Arc::clone(&current_proxy);
        let validation_proxy = Arc::clone(&current_proxy);
        let recorded_validations = Arc::clone(&validations);
        let replacement_proxy = new_proxy.clone();
        let commit_proxy = Arc::clone(&current_proxy);
        let recorded_commits = Arc::clone(&commits);
        let result = validate_with_consistent_proxy(
            move || {
                let proxy = snapshot_proxy.lock().unwrap().clone();
                std::future::ready(proxy)
            },
            move |proxy| {
                recorded_validations.lock().unwrap().push(proxy.clone());
                if proxy == old_proxy {
                    *validation_proxy.lock().unwrap() = replacement_proxy.clone();
                }
                std::future::ready(Ok(proxy))
            },
            move |snapshot, candidate| {
                let current = commit_proxy.lock().unwrap().clone();
                let committed = if current == snapshot {
                    recorded_commits.lock().unwrap().push(candidate.clone());
                    Some(candidate)
                } else {
                    None
                };
                std::future::ready(Ok(committed))
            },
        )
        .await
        .unwrap();

        assert_eq!(result, new_proxy);
        assert_eq!(
            *validations.lock().unwrap(),
            vec![
                Some("http://127.0.0.1:8080/".to_owned()),
                Some("http://127.0.0.1:9090/".to_owned())
            ]
        );
        assert_eq!(*commits.lock().unwrap(), vec![new_proxy]);
    }

    #[tokio::test]
    async fn token_validation_with_unchanged_proxy_commits_once() {
        let proxy = Some("http://127.0.0.1:8080/".to_owned());
        let commits = Arc::new(std::sync::Mutex::new(0));
        let recorded_commits = Arc::clone(&commits);

        let result = validate_with_consistent_proxy(
            || std::future::ready(proxy.clone()),
            |snapshot| std::future::ready(Ok(snapshot)),
            move |snapshot, candidate| {
                *recorded_commits.lock().unwrap() += 1;
                std::future::ready(Ok((snapshot == candidate).then_some(candidate)))
            },
        )
        .await
        .unwrap();

        assert_eq!(result, proxy);
        assert_eq!(*commits.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn token_validation_proxy_churn_exhaustion_keeps_runtime_unchanged() {
        #[derive(Clone)]
        struct Runtime {
            proxy: Option<String>,
            token: String,
            client: String,
        }

        let runtime = Arc::new(std::sync::Mutex::new(Runtime {
            proxy: Some("http://127.0.0.1:8000/".to_owned()),
            token: "old-token".to_owned(),
            client: "old-client".to_owned(),
        }));
        let snapshot_runtime = Arc::clone(&runtime);
        let validation_runtime = Arc::clone(&runtime);
        let commit_runtime = Arc::clone(&runtime);
        let attempts = Arc::new(std::sync::Mutex::new(0_u16));
        let validation_attempts = Arc::clone(&attempts);

        let error = validate_with_consistent_proxy(
            move || {
                let proxy = snapshot_runtime.lock().unwrap().proxy.clone();
                std::future::ready(proxy)
            },
            move |_| {
                let mut attempt = validation_attempts.lock().unwrap();
                *attempt += 1;
                validation_runtime.lock().unwrap().proxy =
                    Some(format!("http://127.0.0.1:{}/", 8000 + *attempt));
                std::future::ready(Ok("new-client".to_owned()))
            },
            move |snapshot, candidate| {
                let mut runtime = commit_runtime.lock().unwrap();
                let committed = if runtime.proxy == snapshot {
                    runtime.token = "new-token".to_owned();
                    runtime.client = candidate;
                    Some(())
                } else {
                    None
                };
                std::future::ready(Ok(committed))
            },
        )
        .await
        .err()
        .unwrap();

        assert_eq!(
            error,
            "Network settings changed while validating the token; try again"
        );
        let runtime = runtime.lock().unwrap();
        assert_eq!(runtime.token, "old-token");
        assert_eq!(runtime.client, "old-client");
        assert_eq!(*attempts.lock().unwrap(), 3);
    }

    #[tokio::test]
    async fn newer_token_request_commits_first_and_supersedes_slow_request() {
        let settings_update = Arc::new(tokio::sync::Mutex::new(0_u64));
        let runtime_token = Arc::new(std::sync::Mutex::new("old-token".to_owned()));
        let a_validations = Arc::new(std::sync::Mutex::new(0_u16));
        let (a_started_tx, a_started_rx) = tokio::sync::oneshot::channel();
        let (release_a_tx, release_a_rx) = tokio::sync::oneshot::channel();
        let a_started_tx = Arc::new(std::sync::Mutex::new(Some(a_started_tx)));
        let release_a_rx = Arc::new(tokio::sync::Mutex::new(Some(release_a_rx)));

        let a_generation = begin_token_request(&settings_update).await;
        let a_gate = Arc::clone(&settings_update);
        let a_runtime = Arc::clone(&runtime_token);
        let a_validation_count = Arc::clone(&a_validations);
        let slow_a = tokio::spawn(validate_with_consistent_proxy(
            || std::future::ready(None),
            move |_| {
                let a_started_tx = Arc::clone(&a_started_tx);
                let release_a_rx = Arc::clone(&release_a_rx);
                *a_validation_count.lock().unwrap() += 1;
                async move {
                    a_started_tx
                        .lock()
                        .unwrap()
                        .take()
                        .unwrap()
                        .send(())
                        .unwrap();
                    release_a_rx.lock().await.take().unwrap().await.unwrap();
                    Ok("token-a".to_owned())
                }
            },
            move |_, candidate| {
                let a_gate = Arc::clone(&a_gate);
                let a_runtime = Arc::clone(&a_runtime);
                async move {
                    let current_generation = a_gate.lock().await;
                    if !token_commit_is_current(*current_generation, a_generation, &None, &None)? {
                        return Ok(None);
                    }
                    *a_runtime.lock().unwrap() = candidate;
                    Ok(Some(()))
                }
            },
        ));

        a_started_rx.await.unwrap();

        let b_generation = begin_token_request(&settings_update).await;
        let b_gate = Arc::clone(&settings_update);
        let b_runtime = Arc::clone(&runtime_token);
        validate_with_consistent_proxy(
            || std::future::ready(None),
            |_| std::future::ready(Ok("token-b".to_owned())),
            move |_, candidate| {
                let b_gate = Arc::clone(&b_gate);
                let b_runtime = Arc::clone(&b_runtime);
                async move {
                    let current_generation = b_gate.lock().await;
                    assert_eq!(*current_generation, b_generation);
                    *b_runtime.lock().unwrap() = candidate;
                    Ok(Some(()))
                }
            },
        )
        .await
        .unwrap();

        release_a_tx.send(()).unwrap();
        let a_error = slow_a.await.unwrap().unwrap_err();

        assert_eq!(
            a_error,
            "A newer token replacement request superseded this one"
        );
        assert_eq!(*runtime_token.lock().unwrap(), "token-b");
        assert_eq!(*a_validations.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn newer_failed_token_request_still_supersedes_slow_request() {
        let settings_update = Arc::new(tokio::sync::Mutex::new(0_u64));
        let runtime_token = Arc::new(std::sync::Mutex::new("old-token".to_owned()));
        let (a_started_tx, a_started_rx) = tokio::sync::oneshot::channel();
        let (release_a_tx, release_a_rx) = tokio::sync::oneshot::channel();
        let a_started_tx = Arc::new(std::sync::Mutex::new(Some(a_started_tx)));
        let release_a_rx = Arc::new(tokio::sync::Mutex::new(Some(release_a_rx)));

        let a_generation = begin_token_request(&settings_update).await;
        let a_gate = Arc::clone(&settings_update);
        let a_runtime = Arc::clone(&runtime_token);
        let slow_a = tokio::spawn(validate_with_consistent_proxy(
            || std::future::ready(None),
            move |_| {
                let a_started_tx = Arc::clone(&a_started_tx);
                let release_a_rx = Arc::clone(&release_a_rx);
                async move {
                    a_started_tx
                        .lock()
                        .unwrap()
                        .take()
                        .unwrap()
                        .send(())
                        .unwrap();
                    release_a_rx.lock().await.take().unwrap().await.unwrap();
                    Ok("token-a".to_owned())
                }
            },
            move |_, candidate| {
                let a_gate = Arc::clone(&a_gate);
                let a_runtime = Arc::clone(&a_runtime);
                async move {
                    let current_generation = a_gate.lock().await;
                    if !token_commit_is_current(*current_generation, a_generation, &None, &None)? {
                        return Ok(None);
                    }
                    *a_runtime.lock().unwrap() = candidate;
                    Ok(Some(()))
                }
            },
        ));

        a_started_rx.await.unwrap();

        let _b_generation = begin_token_request(&settings_update).await;
        let b_error = validate_with_consistent_proxy(
            || std::future::ready(None),
            |_| std::future::ready(Err::<String, _>("validation failed".to_owned())),
            |_, _| std::future::ready(Ok(Some(()))),
        )
        .await
        .unwrap_err();
        assert_eq!(b_error, "validation failed");

        release_a_tx.send(()).unwrap();
        let a_error = slow_a.await.unwrap().unwrap_err();

        assert_eq!(
            a_error,
            "A newer token replacement request superseded this one"
        );
        assert_eq!(*runtime_token.lock().unwrap(), "old-token");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn proxy_update_reconciles_config_and_network_after_waiter_is_aborted() {
        let config = Arc::new(RwLock::new(Config {
            token: Some("old-token".to_owned()),
            ..Config::default()
        }));
        let network = Arc::new(RwLock::new(
            crate::network::NetworkClients::from_config(&*config.read().await).unwrap(),
        ));
        let old_hiker = network.read().await.hiker.clone().unwrap();
        let settings_update = Arc::new(tokio::sync::Mutex::new(0_u64));
        let (persist_started_tx, persist_started_rx) = tokio::sync::oneshot::channel();
        let (release_persist_tx, release_persist_rx) = std::sync::mpsc::channel();

        let update = spawn_proxy_update(
            Arc::clone(&settings_update),
            Arc::clone(&config),
            Arc::clone(&network),
            Some("http://127.0.0.1:8080".to_owned()),
            move |_| {
                persist_started_tx.send(()).unwrap();
                release_persist_rx
                    .recv_timeout(Duration::from_secs(1))
                    .map_err(|_| "persistence was not released".to_owned())?;
                Ok(())
            },
        );
        let waiter = tokio::spawn(update);

        persist_started_rx.await.unwrap();
        waiter.abort();
        assert!(waiter.await.unwrap_err().is_cancelled());
        release_persist_tx.send(()).unwrap();

        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                let proxy_committed =
                    config.read().await.proxy_url.as_deref() == Some("http://127.0.0.1:8080/");
                let new_hiker = network.read().await.hiker.clone().unwrap();
                if proxy_committed && !Arc::ptr_eq(&old_hiker, &new_hiker) {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("owned proxy update should finish after waiter cancellation");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn token_commit_reconciles_config_and_network_after_waiter_is_aborted() {
        let config = Arc::new(RwLock::new(Config {
            token: Some("old-token".to_owned()),
            ..Config::default()
        }));
        let network = Arc::new(RwLock::new(
            crate::network::NetworkClients::from_config(&*config.read().await).unwrap(),
        ));
        let old_hiker = network.read().await.hiker.clone().unwrap();
        let settings_update = Arc::new(tokio::sync::Mutex::new(0_u64));
        let generation = begin_token_request(&settings_update).await;
        let probe = HikerClient::with_proxy("new-token".to_owned(), None).unwrap();
        let balance = Balance {
            requests: 1,
            rate: None,
            amount: None,
            currency: None,
        };
        let (persist_started_tx, persist_started_rx) = tokio::sync::oneshot::channel();
        let (release_persist_tx, release_persist_rx) = std::sync::mpsc::channel();

        let commit = spawn_token_commit(
            Arc::clone(&settings_update),
            generation,
            None,
            Arc::clone(&config),
            Arc::clone(&network),
            "new-token".to_owned(),
            probe,
            balance,
            move |_| {
                persist_started_tx.send(()).unwrap();
                release_persist_rx
                    .recv_timeout(Duration::from_secs(1))
                    .map_err(|_| "persistence was not released".to_owned())?;
                Ok(())
            },
        );
        let waiter = tokio::spawn(commit);

        persist_started_rx.await.unwrap();
        waiter.abort();
        assert!(waiter.await.unwrap_err().is_cancelled());
        release_persist_tx.send(()).unwrap();

        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                let token_committed = config.read().await.token.as_deref() == Some("new-token");
                let new_hiker = network.read().await.hiker.clone().unwrap();
                if token_committed && !Arc::ptr_eq(&old_hiker, &new_hiker) {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("owned token commit should finish after waiter cancellation");
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
    fn whitespace_only_loaded_proxy_is_cleared_in_memory() {
        let mut config = Config {
            proxy_url: Some("   \t  ".to_owned()),
            ..Config::default()
        };

        let network = network_from_loaded_config(&mut config);
        let state = super::ConfigState::from(&config);

        assert_eq!(config.proxy_url, None);
        assert!(!state.has_proxy);
        assert_eq!(state.proxy_hint, None);
        assert!(network.hiker.is_none());
    }

    #[test]
    fn valid_loaded_proxy_is_normalized_in_memory_before_clients_are_built() {
        let mut config = Config {
            proxy_url: Some("  http://127.0.0.1:8080  ".to_owned()),
            ..Config::default()
        };

        let _network = network_from_loaded_config(&mut config);

        assert_eq!(config.proxy_url.as_deref(), Some("http://127.0.0.1:8080/"));
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
