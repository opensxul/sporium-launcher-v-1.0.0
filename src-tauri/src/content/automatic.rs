use super::{ContentManager, model::*};
use crate::{
    error::CoreError,
    game::fs::{read_limited, write_atomic},
    instances::{
        filesystem::Paths,
        model::{now, valid_id},
    },
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum UpdateMode {
    Off,
    Check,
    Install,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstanceUpdateMode {
    pub id: String,
    pub content: Option<UpdateMode>,
    pub project: Option<UpdateMode>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutomaticPolicy {
    pub schema_version: u32,
    pub content: UpdateMode,
    pub project: UpdateMode,
    pub instances: Vec<InstanceUpdateMode>,
}
impl Default for AutomaticPolicy {
    fn default() -> Self {
        Self {
            schema_version: 1,
            content: UpdateMode::Check,
            project: UpdateMode::Check,
            instances: vec![],
        }
    }
}
impl AutomaticPolicy {
    pub fn mode(&self, id: &str, project: bool) -> UpdateMode {
        self.instances
            .iter()
            .find(|i| i.id == id)
            .and_then(|i| if project { i.project } else { i.content })
            .unwrap_or(if project { self.project } else { self.content })
    }
    fn validate(&self) -> Result<(), CoreError> {
        if self.schema_version != 1 {
            return Err(CoreError::SchemaTooNew);
        }
        if self.instances.len() > 4096 {
            return Err(CoreError::InvalidInput);
        }
        let mut seen = HashSet::new();
        for item in &self.instances {
            valid_id(&item.id)?;
            if !seen.insert(&item.id) {
                return Err(CoreError::InvalidInput);
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AutomaticReport {
    pub instance_id: String,
    #[ts(type = "number")]
    pub checked_at: u64,
    pub updates: Vec<ContentUpdate>,
    pub status: String,
    pub project: Option<crate::projects::model::ProjectUpdate>,
}
#[derive(Default)]
pub(super) struct Runtime {
    pub reports: HashMap<String, AutomaticReport>,
    pub checked: HashMap<String, Instant>,
}

pub fn policy(paths: &Paths) -> Result<AutomaticPolicy, CoreError> {
    let file = paths.checked(&paths.root().join("launcher/automatic-updates.json"))?;
    if !file.exists() {
        return Ok(AutomaticPolicy::default());
    }
    let result: AutomaticPolicy = serde_json::from_slice(&read_limited(paths, &file, 1_000_000)?)?;
    result.validate()?;
    Ok(result)
}

impl ContentManager {
    pub fn automatic_policy(&self) -> Result<AutomaticPolicy, CoreError> {
        policy(&Paths::new(self.library.root())?)
    }
    pub fn save_automatic_policy(&self, value: AutomaticPolicy) -> Result<(), CoreError> {
        value.validate()?;
        let paths = Paths::new(self.library.root())?;
        let _lease = paths.named_lock(&paths.root().join("launcher/automatic-policy.lock"))?;
        let ids: HashSet<_> = self
            .library
            .snapshot()?
            .instances
            .into_iter()
            .map(|i| i.id)
            .collect();
        if value.instances.iter().any(|i| !ids.contains(&i.id)) {
            return Err(CoreError::NotFound);
        }
        write_atomic(
            &paths,
            &paths.root().join("launcher/automatic-updates.json"),
            &serde_json::to_vec(&value)?,
        )?;
        self.automatic.lock().unwrap().checked.clear();
        Ok(())
    }
    pub fn automatic_reports(&self) -> Vec<AutomaticReport> {
        self.automatic
            .lock()
            .unwrap()
            .reports
            .values()
            .cloned()
            .collect()
    }
    pub fn automatic_check(&self, id: &str) -> Result<AutomaticReport, CoreError> {
        let mode = self.automatic_policy()?.mode(id, false);
        if mode == UpdateMode::Off {
            return Ok(AutomaticReport {
                instance_id: id.into(),
                checked_at: now(),
                updates: vec![],
                status: "off".into(),
                project: None,
            });
        }
        let updates = self.updates(id)?;
        let mut report = AutomaticReport {
            instance_id: id.into(),
            checked_at: now(),
            status: if updates.iter().any(|u| u.status == "unavailable") {
                "unavailable"
            } else {
                "checked"
            }
            .into(),
            updates,
            project: None,
        };
        if mode == UpdateMode::Install && !self.is_active() {
            let projects: HashSet<_> = report
                .updates
                .iter()
                .filter(|u| u.status == "available")
                .map(|u| &u.project_id)
                .collect();
            if !projects.is_empty() {
                let rows = self.installed(id)?;
                let selections = rows
                    .iter()
                    .filter(|r| {
                        r.record.provider == "modrinth" && projects.contains(&r.record.project_id)
                    })
                    .map(|r| ContentSelection {
                        directory: r.record.directory.clone(),
                        filename: r.record.file.filename.clone(),
                        sha512: r.record.file.hashes.sha512.clone(),
                    })
                    .collect();
                match self
                    .update_plan(ContentUpdateRequest {
                        instance_id: id.into(),
                        files: selections,
                    })
                    .and_then(|plan| {
                        if self.automatic_policy()?.mode(id, false) != UpdateMode::Install {
                            return Err(CoreError::RecordConflict);
                        }
                        self.start(&plan.plan.token)
                    }) {
                    Ok(_) => report.status = "installing".into(),
                    Err(CoreError::InstanceBusy | CoreError::LibraryBusy) => {
                        report.status = "busy".into()
                    }
                    Err(_) => report.status = "skipped".into(),
                }
            }
        }
        let mut runtime = self.automatic.lock().unwrap();
        runtime.checked.insert(id.into(), Instant::now());
        runtime.reports.insert(id.into(), report.clone());
        Ok(report)
    }
    pub fn automatic_cycle(
        &self,
        id: &str,
        projects: &crate::projects::ProjectManager,
    ) -> Result<AutomaticReport, CoreError> {
        let mut report = self.automatic_check(id)?;
        let policy = self.automatic_policy()?;
        if policy.mode(id, true) != UpdateMode::Off {
            let update = projects.check(id)?;
            if update.status == "available"
                && policy.mode(id, true) == UpdateMode::Install
                && !self.is_active()
                && !projects.is_active()
            {
                let plan = if let Some(version) = &update.version_id {
                    projects.pack_plan(id, version)
                } else {
                    let view = projects.view(id)?;
                    let directory = self.library.root().join("instances").join(id);
                    let groups = view.manifest.map_or(vec![], |m| {
                        m.files
                            .into_iter()
                            .filter(|f| {
                                f.policy == crate::projects::model::FilePolicy::Optional
                                    && directory.join(&f.path).exists()
                            })
                            .filter_map(|f| f.group)
                            .collect()
                    });
                    projects.source_plan(id, &groups)
                };
                match plan {
                    Ok(plan) => {
                        if plan.warnings.iter().any(|w| {
                            w.starts_with("modified:")
                                || w.starts_with("forbidden:")
                                || w.starts_with("unverified:")
                        }) || self.automatic_policy()?.mode(id, true) != UpdateMode::Install
                        {
                            projects.dismiss(&plan.token);
                            report.status = "skipped".into();
                        } else {
                            report.status = match projects.apply(&plan.token, true) {
                                Ok(_) => "project_installed",
                                Err(CoreError::LibraryBusy | CoreError::InstanceBusy) => "busy",
                                Err(_) => "skipped",
                            }
                            .into();
                        }
                    }
                    Err(CoreError::LibraryBusy | CoreError::InstanceBusy) => {
                        report.status = "busy".into()
                    }
                    Err(_) => report.status = "skipped".into(),
                }
            }
            report.project = Some(update);
        }
        let mut runtime = self.automatic.lock().unwrap();
        runtime.checked.insert(id.into(), Instant::now());
        runtime.reports.insert(id.into(), report.clone());
        Ok(report)
    }
    pub fn start_automatic_worker(
        &self,
        projects: crate::projects::ProjectManager,
    ) -> Result<(), CoreError> {
        let manager = self.clone();
        std::thread::Builder::new()
            .name("sporium-content-checks".into())
            .spawn(move || {
                let Ok(paths) = Paths::new(manager.library.root()) else {
                    return;
                };
                let Ok(_lease) =
                    paths.named_lock(&paths.root().join("launcher/automatic-worker.lock"))
                else {
                    return;
                };
                loop {
                    std::thread::sleep(Duration::from_secs(30));
                    if manager.is_active() {
                        continue;
                    }
                    let Ok(policy) = manager.automatic_policy() else {
                        continue;
                    };
                    let Ok(snapshot) = manager.library.snapshot() else {
                        continue;
                    };
                    let ids: HashSet<_> = snapshot.instances.iter().map(|i| i.id.clone()).collect();
                    manager
                        .automatic
                        .lock()
                        .unwrap()
                        .reports
                        .retain(|id, _| ids.contains(id));
                    for instance in snapshot.instances {
                        if manager.is_active() {
                            break;
                        }
                        if policy.mode(&instance.id, false) == UpdateMode::Off
                            && policy.mode(&instance.id, true) == UpdateMode::Off
                        {
                            continue;
                        }
                        let due = manager
                            .automatic
                            .lock()
                            .unwrap()
                            .checked
                            .get(&instance.id)
                            .is_none_or(|t| t.elapsed() >= Duration::from_secs(1800));
                        if due {
                            let _ = manager.automatic_cycle(&instance.id, &projects);
                        }
                    }
                }
            })
            .map_err(|_| CoreError::Worker)?;
        Ok(())
    }
}

pub(super) fn runtime() -> Arc<Mutex<Runtime>> {
    Arc::new(Mutex::new(Runtime::default()))
}
