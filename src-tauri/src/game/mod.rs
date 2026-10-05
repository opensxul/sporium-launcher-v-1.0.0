pub mod cache;
pub mod catalog;
mod download_state;
pub mod fs;
mod install;
pub mod java;
mod job_object;
pub mod launch;
pub mod loaders;
pub mod local_profile;
pub mod metadata;
pub mod model;
pub mod network;
pub mod rules;
pub(crate) mod skinmod;
pub mod transfer;
#[cfg(test)]
mod transfer_tests;

use crate::{
    error::{CommandError, CoreError},
    instances::{Library, filesystem::Paths, model::now},
    storage::Database,
};
use model::*;
use std::{
    fs::File,
    process::Child,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

struct Tracked {
    child: Child,
    view: GameSession,
    _group: job_object::ProcessGroup,
    _lease: File,
}
#[derive(Default)]
struct State {
    rate_sample: Option<(std::time::Instant, u64)>,
    job: Option<GameJob>,
    active: Vec<Tracked>,
    ended: Vec<GameSession>,
}
#[derive(Clone)]
pub struct GameManager {
    pub(crate) library: Library,
    pub(crate) database: Database,
    state: Arc<Mutex<State>>,
    cancel: Arc<AtomicBool>,
    pause: Arc<AtomicBool>,
    external_cancel: Option<Arc<AtomicBool>>,
}

impl GameManager {
    pub fn new(library: Library, database: Database) -> Self {
        let recovered = download_state::recovery(library.root());
        Self {
            library,
            database,
            state: Arc::new(Mutex::new(State {
                job: recovered,
                ..State::default()
            })),
            cancel: Arc::new(AtomicBool::new(false)),
            pause: Arc::new(AtomicBool::new(false)),
            external_cancel: None,
        }
    }
    fn active_job(job: &Option<GameJob>) -> bool {
        job.as_ref().is_some_and(|job| {
            !matches!(
                job.phase,
                JobPhase::Completed
                    | JobPhase::Cancelled
                    | JobPhase::Failed
                    | JobPhase::Interrupted
            )
        })
    }
    pub fn snapshot(&self) -> GameState {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        for index in (0..state.active.len()).rev() {
            if let Ok(Some(status)) = state.active[index].child.try_wait() {
                let mut tracked = state.active.remove(index);
                tracked.view.running = false;
                tracked.view.exit_code = status.code();
                state
                    .ended
                    .retain(|view| view.instance_id != tracked.view.instance_id);
                state.ended.push(tracked.view.clone());
            }
        }
        if state.ended.len() > 20 {
            state.ended.remove(0);
        }
        Self::sample_rate(&mut state);
        GameState {
            job: state.job.clone(),
            sessions: state
                .active
                .iter()
                .map(|session| session.view.clone())
                .chain(state.ended.iter().cloned())
                .collect(),
        }
    }
    pub fn is_active(&self) -> bool {
        let view = self.snapshot();
        (Self::active_job(&view.job) && !view.job.as_ref().is_some_and(|j| j.paused))
            || view.sessions.iter().any(|session| session.running)
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
    fn cancellation_requested(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
            || self
                .external_cancel
                .as_ref()
                .is_some_and(|c| c.load(Ordering::SeqCst))
    }
    pub fn cancelled(&self) -> bool {
        while self.pause.load(Ordering::SeqCst) && !self.cancellation_requested() {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        self.cancellation_requested()
    }
    pub fn phase(&self, phase: JobPhase) {
        let mut state = self.state.lock().unwrap();
        self.pause.store(false, Ordering::SeqCst);
        if let Some(job) = &mut state.job {
            job.phase = phase;
            job.paused = false;
        }
    }
    pub fn bytes(&self, bytes: u64) {
        if let Some(job) = &mut self.state.lock().unwrap().job {
            job.downloaded_bytes += bytes;
        }
    }
    pub fn download_count(&self, count: u32) {
        if let Some(job) = &mut self.state.lock().unwrap().job {
            job.phase = JobPhase::Downloading;
            job.total_files = count;
            job.completed_files = 0;
        }
    }
    pub fn file_completed(&self) {
        if let Some(job) = &mut self.state.lock().unwrap().job {
            job.completed_files += 1;
        }
    }
    pub fn catalog(&self, refresh: bool) -> Result<VersionCatalog, CoreError> {
        catalog::catalog(
            &Paths::new(self.library.root())?,
            &network::Network::new()?,
            refresh,
        )
    }
    pub fn java_runtimes(&self) -> Result<Vec<JavaRuntime>, CoreError> {
        Ok(java::discover(&Paths::new(self.library.root())?))
    }
    pub fn inspect_java(&self, path: &str) -> Result<JavaRuntime, CoreError> {
        java::inspect(std::path::Path::new(path), false)
    }

    // The content worker owns the instance lease throughout this preparation.
    pub(crate) fn prepare_content_base(
        &self,
        instance: &crate::instances::model::Instance,
        directory: &std::path::Path,
        cancellation: Arc<AtomicBool>,
    ) -> Result<(), CoreError> {
        let request = GameRequest {
            id: instance.id.clone(),
            action: GameAction::Install,
        };
        let nickname = crate::profiles::active(&self.database)?.nickname;
        {
            let mut state = self.state.lock().unwrap();
            if Self::active_job(&state.job) {
                return Err(CoreError::InstanceBusy);
            }
            self.record_operation(Some(&request))?;
            self.cancel.store(false, Ordering::SeqCst);
            self.pause.store(false, Ordering::SeqCst);
            state.job = Some(GameJob::new(&request, JobPhase::Resolving));
            state.rate_sample = None;
        }
        let mut worker = self.clone();
        worker.external_cancel = Some(cancellation);
        let result = (|| {
            if worker.cancelled() {
                return Err(CoreError::Cancelled);
            }
            install::prepare(&worker, instance, directory, &nickname)?;
            if worker.cancelled() {
                return Err(CoreError::Cancelled);
            }
            self.library.mark_game(&instance.id, false)
        })();
        if result.is_ok() || matches!(result, Err(CoreError::Cancelled)) {
            let _ = self.record_operation(None);
        }
        if let Some(job) = &mut self.state.lock().unwrap().job {
            job.paused = false;
            job.active_files.clear();
            match &result {
                Ok(()) => job.phase = JobPhase::Completed,
                Err(CoreError::Cancelled) => job.phase = JobPhase::Cancelled,
                Err(_) => {
                    job.phase = JobPhase::Failed;
                    // Preserve the exact typed error in the content job returned by the caller.
                    tracing::warn!("content_game_preparation_failed");
                }
            }
        }
        result
    }

    pub fn start(&self, request: GameRequest) -> Result<GameState, CoreError> {
        self.snapshot();
        let mut state = self.state.lock().unwrap();
        if Self::active_job(&state.job)
            || state
                .active
                .iter()
                .any(|session| session.view.instance_id == request.id)
        {
            return Err(CoreError::InstanceBusy);
        }
        let (instance, directory, lease) = self.library.lease_game(&request.id)?;
        if matches!(request.action, GameAction::Local) {
            crate::projects::validate_launch(
                &Paths::new(self.library.root())?,
                &directory,
                &instance,
            )?;
        }
        let nickname = crate::profiles::active(&self.database)?.nickname;
        self.record_operation(Some(&request))?;
        state.job = Some(GameJob::new(&request, JobPhase::Resolving));
        state.rate_sample = None;
        self.pause.store(false, Ordering::SeqCst);
        self.cancel.store(false, Ordering::SeqCst);
        drop(state);
        let manager = self.clone();
        let spawn = std::thread::Builder::new()
            .name("sporium-game-operation".into())
            .spawn(move || {
                let result = (|| {
                    let plan = install::prepare(&manager, &instance, &directory, &nickname)?;
                    let limit = manager.database.load_settings()?.values.cache_limit_mb;
                    if limit > 0
                        && cache::trim(manager.library.root(), u64::from(limit) * 1048576).is_err()
                    {
                        tracing::warn!("optional_cache_limit_cleanup_failed");
                    }
                    if manager.cancelled() {
                        return Err(CoreError::Cancelled);
                    }
                    manager.library.mark_game(&instance.id, false)?;
                    if !matches!(request.action, GameAction::Install) {
                        manager.phase(JobPhase::Launching);
                        let paths = Paths::new(manager.library.root())?;
                        let log = paths.checked(
                            &directory
                                .join("logs")
                                .join(format!("sporium-{}.log", now())),
                        )?;
                        let mut child = launch::spawn(&plan, &log)?;
                        let group = job_object::ProcessGroup::attach(&mut child)?;
                        manager.library.mark_game(&instance.id, true)?;
                        let view = GameSession {
                            instance_id: instance.id.clone(),
                            running: true,
                            exit_code: None,
                            log_path: fs::java_path(&log),
                        };
                        let mut state = manager.state.lock().unwrap();
                        state.ended.retain(|view| view.instance_id != instance.id);
                        state.active.push(Tracked {
                            child,
                            view,
                            _group: group,
                            _lease: lease,
                        });
                    }
                    Ok(())
                })();
                if (result.is_ok() || matches!(result, Err(CoreError::Cancelled)))
                    && manager.record_operation(None).is_err()
                {
                    tracing::warn!("download_recovery_record_cleanup_failed");
                }
                let mut state = manager.state.lock().unwrap();
                if let Some(job) = &mut state.job {
                    job.paused = false;
                    job.active_files.clear();
                    match result {
                        Ok(()) => job.phase = JobPhase::Completed,
                        Err(CoreError::Cancelled) => job.phase = JobPhase::Cancelled,
                        Err(error) => {
                            job.phase = JobPhase::Failed;
                            job.error = Some(CommandError::from(error));
                        }
                    }
                }
            });
        if spawn.is_err() {
            self.phase(JobPhase::Failed);
            return Err(CoreError::Worker);
        }
        Ok(self.snapshot())
    }

    pub fn stop(&self, id: &str) -> Result<GameState, CoreError> {
        crate::instances::model::valid_id(id)?;
        {
            let mut state = self.state.lock().unwrap();
            let session = state
                .active
                .iter_mut()
                .find(|session| session.view.instance_id == id)
                .ok_or(CoreError::NotFound)?;
            session.child.kill().map_err(|_| CoreError::LaunchFailed)?;
            session.child.wait()?;
        }
        Ok(self.snapshot())
    }
}
