use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::cdn;
use crate::config::Config;
use crate::hiker::map_post;
use crate::models::Post;
use crate::targets::Target;
use crate::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "state")]
enum JobState {
    Fetching,
    Downloading {
        current_file: usize,
        total_files: usize,
        bytes_done: u64,
        file_name: String,
    },
    Done {
        files: Vec<String>,
    },
    Failed {
        error: String,
    },
}

#[derive(Debug, Clone, Serialize)]
struct JobProgress {
    job_id: String,
    label: String,
    #[serde(flatten)]
    state: JobState,
}

fn emit(app: &AppHandle, payload: &JobProgress) {
    app.emit("job-progress", payload).ok();
}

fn taken_at_name(ts: Option<i64>, fallback_code: &str) -> String {
    let base = match ts {
        Some(unix) => {
            let dt = chrono::DateTime::<chrono::Utc>::from_timestamp(unix, 0)
                .map(|u| u.with_timezone(&chrono::Local));
            dt.map(|d| d.format("%Y-%m-%d_%H-%M-%S").to_string())
        }
        None => None,
    }
    .unwrap_or_else(|| chrono::Local::now().format("%Y-%m-%d_%H-%M-%S").to_string());
    format!("{base}_{fallback_code}")
}

fn safe_segment(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() || "-_.@".contains(c) { c } else { '_' })
        .collect()
}

fn post_dir(cfg: &Config, post: &Post) -> PathBuf {
    let owner = safe_segment(
        post.owner_username
            .as_deref()
            .or(post.owner_pk.as_deref())
            .unwrap_or("unknown"),
    );
    Path::new(&cfg.dest_dir).join(owner)
}

fn write_sidecar(cfg: &Config, dir: &Path, post: &Post, first_file: &Path) -> Result<(), String> {
    if !cfg.sidecar {
        return Ok(());
    }
    let stem = first_file.file_stem().and_then(|s| s.to_str()).unwrap_or("post");
    let sidecar = serde_json::json!({
        "pk": post.pk,
        "code": post.code,
        "taken_at": post.taken_at,
        "caption": post.caption,
        "like_count": post.like_count,
        "comment_count": post.comment_count,
        "owner": {
            "pk": post.owner_pk,
            "username": post.owner_username,
        },
    });
    let path = dir.join(format!("{stem}.json"));
    let json = serde_json::to_vec_pretty(&sidecar).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn resolve_input(input: String) -> Result<Target, String> {
    Target::parse(&input).ok_or_else(|| format!("Unrecognized input: {input}"))
}

#[tauri::command]
pub async fn fetch_post(
    code: String,
    state: State<'_, AppState>,
) -> Result<Post, String> {
    let client = state.client.read().await.clone().ok_or("No HikerAPI token configured")?;
    let media = client.media_by_code(&code).await.map_err(|e| e.to_string())?;
    map_post(&media).ok_or_else(|| "Could not parse media payload".to_string())
}

/// Download a single post/reel by shortcode. Emits `job-progress` events.
#[tauri::command]
pub async fn download_post(
    code: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let (client, cfg, cdn_http) = {
        let client = state.client.read().await.clone().ok_or("No HikerAPI token configured")?;
        let cfg = state.cfg.read().await.clone();
        (client, cfg, state.cdn_http.clone())
    };

    let media = client.media_by_code(&code).await.map_err(|e| e.to_string())?;
    let post = map_post(&media).ok_or("Could not parse media payload")?;
    let job_id = uuid::Uuid::new_v4().to_string();
    let label = post
        .owner_username
        .clone()
        .unwrap_or_else(|| post.code.clone());

    emit(
        &app,
        &JobProgress {
            job_id: job_id.clone(),
            label: label.clone(),
            state: JobState::Downloading {
                current_file: 0,
                total_files: post.resources.len(),
                bytes_done: 0,
                file_name: post.code.clone(),
            },
        },
    );

    let dir = post_dir(&cfg, &post);
    let base = taken_at_name(post.taken_at, &post.code);
    let total = post.resources.len();
    let mut written_files: Vec<String> = Vec::new();

    for (idx, resource) in post.resources.iter().enumerate() {
        let dest_base = if total > 1 {
            dir.join(format!("{base}_{}", idx + 1))
        } else {
            dir.join(&base)
        };
        let dest_label = format!("{}_{}", base, idx + 1);
        let job_id_clone = job_id.clone();
        let label_clone = label.clone();
        let app_for_progress = app.clone();
        let outcome = cdn::stream_to_file(&cdn_http, &resource.url, &dest_base, post.taken_at, move |bytes| {
            emit(
                &app_for_progress,
                &JobProgress {
                    job_id: job_id_clone.clone(),
                    label: label_clone.clone(),
                    state: JobState::Downloading {
                        current_file: idx + 1,
                        total_files: total,
                        bytes_done: bytes,
                        file_name: dest_label.clone(),
                    },
                },
            );
        })
        .await
        .map_err(|e| e.to_string());

        match outcome {
            Ok(outcome) => {
                if written_files.is_empty() {
                    write_sidecar(&cfg, &dir, &post, &outcome.path)?;
                }
                written_files.push(outcome.path.to_string_lossy().into_owned());
            }
            Err(e) => {
                emit(
                    &app,
                    &JobProgress {
                        job_id: job_id.clone(),
                        label,
                        state: JobState::Failed { error: e },
                    },
                );
                return Ok(job_id);
            }
        }
    }

    if written_files.is_empty() {
        emit(
            &app,
            &JobProgress {
                job_id: job_id.clone(),
                label,
                state: JobState::Failed {
                    error: "No downloadable media found in post".into(),
                },
            },
        );
        return Ok(job_id);
    }

    emit(
        &app,
        &JobProgress {
            job_id: job_id.clone(),
            label,
            state: JobState::Done {
                files: written_files,
            },
        },
    );
    Ok(job_id)
}
