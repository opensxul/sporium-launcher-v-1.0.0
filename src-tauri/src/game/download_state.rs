use super::{
    GameManager,
    fs::{read_limited, write_atomic},
    model::*,
    transfer::{Event, Outcome},
};
use crate::{error::CoreError, instances::filesystem::Paths};
use std::{sync::atomic::Ordering, time::Instant};

pub(super) fn recovery(root: &std::path::Path) -> Option<GameJob> {
    let paths = Paths::new(root).ok()?;
    let bytes = read_limited(
        &paths,
        &paths.root().join("launcher/download-operation.json"),
        4096,
    )
    .ok()?;
    let request: GameRequest = serde_json::from_slice(&bytes).ok()?;
    crate::instances::model::valid_id(&request.id).ok()?;
    Some(GameJob::new(&request, JobPhase::Interrupted))
}

impl GameManager {
    pub(super) fn record_operation(&self, request: Option<&GameRequest>) -> Result<(), CoreError> {
        let paths = Paths::new(self.library.root())?;
        let path = paths.root().join("launcher/download-operation.json");
        if let Some(request) = request {
            write_atomic(&paths, &path, &serde_json::to_vec(request)?)
        } else {
            paths.remove(&path)
        }
    }
    pub fn pause_downloads(&self, paused: bool) -> Result<GameState, CoreError> {
        {
            let mut state = self.state.lock().unwrap();
            let job = state.job.as_mut().ok_or(CoreError::NotFound)?;
            if !matches!(job.phase, JobPhase::Downloading) {
                return Err(CoreError::InvalidInput);
            }
            job.paused = paused;
            job.bytes_per_second = 0;
            job.eta_seconds = None;
            self.pause.store(paused, Ordering::SeqCst);
            state.rate_sample = None;
        }
        Ok(self.snapshot())
    }
    pub(super) fn download_plan(&self, count: u32, size: u64) {
        let mut state = self.state.lock().unwrap();
        state.rate_sample = None;
        if let Some(job) = &mut state.job {
            job.phase = JobPhase::Downloading;
            job.total_files = count;
            job.completed_files = 0;
            job.total_bytes = size;
            job.verified_bytes = 0;
            job.active_files.clear();
        }
    }
    pub(super) fn transfer_started(&self, index: u32, name: String, total: u64) {
        if let Some(job) = &mut self.state.lock().unwrap().job {
            job.active_files.push(TransferFile {
                index,
                name,
                received: 0,
                total,
            });
        }
    }
    pub(super) fn transfer_event(&self, index: u32, event: Event) {
        if let Some(job) = &mut self.state.lock().unwrap().job {
            match event {
                Event::Retry => job.retries += 1,
                Event::Progress { received } => {
                    if let Some(file) = job.active_files.iter_mut().find(|f| f.index == index) {
                        file.received = received;
                    }
                }
            }
        }
    }
    pub(super) fn transfer_done(&self, index: u32, size: u64, outcome: Outcome) {
        if let Some(job) = &mut self.state.lock().unwrap().job {
            job.active_files.retain(|file| file.index != index);
            job.completed_files += 1;
            job.verified_bytes += size;
            if outcome.cached {
                job.cached_files += 1;
            }
            if outcome.repaired {
                job.repaired_files += 1;
            }
        }
    }
    pub(super) fn sample_rate(state: &mut super::State) {
        let Some(job) = &mut state.job else { return };
        let now = Instant::now();
        if job.paused
            || !matches!(
                job.phase,
                JobPhase::Downloading | JobPhase::Java | JobPhase::Loader
            )
        {
            job.bytes_per_second = 0;
            job.eta_seconds = None;
            state.rate_sample = None;
            return;
        }
        if let Some((time, bytes)) = state.rate_sample {
            let elapsed = time.elapsed().as_secs_f64();
            if elapsed >= 0.5 {
                job.bytes_per_second =
                    ((job.downloaded_bytes.saturating_sub(bytes)) as f64 / elapsed) as u64;
                state.rate_sample = Some((now, job.downloaded_bytes));
            }
        } else {
            state.rate_sample = Some((now, job.downloaded_bytes));
        }
        let active: u64 = job.active_files.iter().map(|f| f.received).sum();
        let remaining = job.total_bytes.saturating_sub(job.verified_bytes + active);
        job.eta_seconds = if matches!(job.phase, JobPhase::Downloading) && job.bytes_per_second > 0
        {
            Some(remaining / job.bytes_per_second)
        } else {
            None
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{instances::Library, storage::Database};
    #[test]
    fn interrupted_operation_is_recoverable_but_never_runs_automatically() {
        let temp = tempfile::tempdir().unwrap();
        let db = Database::new(temp.path().join("launcher/sporium.sqlite3"));
        let library = Library::new(temp.path().into(), db.clone());
        let manager = GameManager::new(library.clone(), db.clone());
        let request = GameRequest {
            id: uuid::Uuid::new_v4().to_string(),
            action: GameAction::Local,
        };
        manager.record_operation(Some(&request)).unwrap();
        let restored = GameManager::new(library, db);
        let view = restored.snapshot();
        assert!(matches!(view.job.unwrap().phase, JobPhase::Interrupted));
        assert!(view.sessions.is_empty());
        assert!(!restored.is_active());
        restored.download_plan(2, 200);
        restored.pause_downloads(true).unwrap();
        assert!(restored.snapshot().job.unwrap().paused);
        let worker = restored.clone();
        let (sender, receiver) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || sender.send(worker.cancelled()).unwrap());
        assert!(
            receiver
                .recv_timeout(std::time::Duration::from_millis(200))
                .is_err()
        );
        restored.cancel();
        assert!(
            receiver
                .recv_timeout(std::time::Duration::from_secs(2))
                .unwrap()
        );
        thread.join().unwrap();
        restored.record_operation(None).unwrap();
        assert!(recovery(temp.path()).is_none());
    }
}
