use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", default)]
pub struct Config {
    pub token: Option<String>,
    pub dest_dir: String,
    #[serde(rename = "sidecar")]
    pub sidecar: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            token: None,
            dest_dir: default_dest_dir(),
            sidecar: true,
        }
    }
}

fn default_dest_dir() -> String {
    dirs::download_dir()
        .or_else(dirs::home_dir)
        .map(|p| p.join("insta-dl").to_string_lossy().into_owned())
        .unwrap_or_else(|| ".".into())
}

impl Config {
    pub fn path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("insta-dl-gui").join("config.json"))
    }

    pub fn load() -> Self {
        let Some(path) = Self::path() else {
            return Self::default();
        };
        match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let Some(path) = Self::path() else {
            return Err("no config directory".into());
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            restrictive_dir_perms(&parent);
        }
        let json = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        let tmp = path.with_extension("json.tmp");
        {
            let mut f = fs::File::create(&tmp).map_err(|e| e.to_string())?;
            f.write_all(&json).map_err(|e| e.to_string())?;
            f.sync_all().ok();
        }
        restrictive_file_perms(&tmp);
        fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
        restrictive_file_perms(&path);
        Ok(())
    }

    pub fn token_hint(&self) -> Option<String> {
        self.token.as_ref().map(|t| format!("***{}", &t[t.len().saturating_sub(4)..]))
    }
}

#[cfg(unix)]
fn restrictive_file_perms(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(mut perms) = fs::metadata(path).map(|m| m.permissions()) {
        perms.set_mode(0o600);
        fs::set_permissions(path, perms).ok();
    }
}

#[cfg(unix)]
fn restrictive_dir_perms(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(mut perms) = fs::metadata(path).map(|m| m.permissions()) {
        perms.set_mode(0o700);
        fs::set_permissions(path, perms).ok();
    }
}

#[cfg(not(unix))]
fn restrictive_file_perms(_path: &Path) {}

#[cfg(not(unix))]
fn restrictive_dir_perms(_path: &Path) {}
