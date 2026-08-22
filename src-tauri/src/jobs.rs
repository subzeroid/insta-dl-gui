use std::collections::HashMap;
use std::sync::Mutex;
use tokio::sync::watch;

/// Tracks running jobs so they can be cancelled mid-download.
pub struct JobRegistry {
    cancels: Mutex<HashMap<String, watch::Sender<bool>>>,
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
