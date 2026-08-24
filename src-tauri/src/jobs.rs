use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use tokio::sync::watch;

/// Tracks running jobs so they can be cancelled mid-download.
pub struct JobRegistry {
    cancels: Mutex<HashMap<String, watch::Sender<bool>>>,
}

#[derive(Clone, Debug)]
pub struct ScanCancellation {
    cancelled: Arc<AtomicBool>,
}

impl Default for ScanCancellation {
    fn default() -> Self {
        Self::new()
    }
}

impl ScanCancellation {
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

#[derive(Debug)]
struct ActiveScan {
    scan_id: String,
    cancellation: ScanCancellation,
}

#[derive(Debug, Default)]
pub struct ScanRegistry {
    active_by_root: Mutex<HashMap<i64, ActiveScan>>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ScanRegistryError {
    #[error("a library scan is already active for root {root_id}")]
    RootBusy { root_id: i64 },
}

pub struct ScanLease {
    registry: Weak<ScanRegistry>,
    root_id: i64,
    scan_id: String,
    cancellation: ScanCancellation,
}

impl ScanRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn try_register(
        self: &Arc<Self>,
        root_id: i64,
        scan_id: impl Into<String>,
    ) -> Result<ScanLease, ScanRegistryError> {
        let scan_id = scan_id.into();
        let cancellation = ScanCancellation::new();
        let mut active = self.active_by_root.lock().unwrap();
        if active.contains_key(&root_id) {
            return Err(ScanRegistryError::RootBusy { root_id });
        }
        active.insert(
            root_id,
            ActiveScan {
                scan_id: scan_id.clone(),
                cancellation: cancellation.clone(),
            },
        );
        Ok(ScanLease {
            registry: Arc::downgrade(self),
            root_id,
            scan_id,
            cancellation,
        })
    }

    pub fn cancel(&self, scan_id: &str) -> bool {
        let active = self.active_by_root.lock().unwrap();
        let Some(scan) = active.values().find(|scan| scan.scan_id == scan_id) else {
            return false;
        };
        scan.cancellation.cancel();
        true
    }

    #[cfg(test)]
    pub(crate) fn active_len(&self) -> usize {
        self.active_by_root.lock().unwrap().len()
    }

    fn finish(&self, root_id: i64, scan_id: &str) {
        let mut active = self.active_by_root.lock().unwrap();
        if active
            .get(&root_id)
            .is_some_and(|scan| scan.scan_id == scan_id)
        {
            active.remove(&root_id);
        }
    }
}

impl ScanLease {
    pub fn scan_id(&self) -> &str {
        &self.scan_id
    }

    pub fn cancellation(&self) -> &ScanCancellation {
        &self.cancellation
    }
}

impl Drop for ScanLease {
    fn drop(&mut self) {
        if let Some(registry) = self.registry.upgrade() {
            registry.finish(self.root_id, &self.scan_id);
        }
    }
}

impl Default for JobRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl JobRegistry {
    pub fn new() -> Self {
        Self {
            cancels: Mutex::new(HashMap::new()),
        }
    }

    /// Register a job and get its cancel receiver.
    pub fn register(self: &std::sync::Arc<Self>, job_id: &str) -> watch::Receiver<bool> {
        let (tx, rx) = watch::channel(false);
        self.cancels.lock().unwrap().insert(job_id.to_string(), tx);
        rx
    }

    pub fn cancel(&self, job_id: &str) -> bool {
        if let Some(tx) = self.cancels.lock().unwrap().get(job_id) {
            tx.send_replace(true);
            return true;
        }
        false
    }

    pub fn finish(&self, job_id: &str) {
        self.cancels.lock().unwrap().remove(job_id);
    }
}
