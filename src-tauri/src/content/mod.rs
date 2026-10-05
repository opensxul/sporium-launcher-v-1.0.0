mod adopt;
pub mod automatic;
mod dependencies;
mod diagnostics;
mod icons;
pub(crate) mod install;
mod local;
mod manage;
pub mod model;
pub mod modrinth;
pub mod provider;
pub mod resolve;
mod restore;
#[cfg(test)]
mod tests;
mod untracked;
mod update;
pub(crate) mod worlds;

use crate::{
    error::{CommandError, CoreError},
    game::network::{Download, Hash, Network},
    instances::{Library, filesystem::Paths, model::Instance},
};
use model::*;
pub(crate) fn project_diagnostics(
    paths: &Paths,
    directory: &std::path::Path,
    instance: &Instance,
) -> Result<ModDiagnostics, CoreError> {
    diagnostics::scan(paths, directory, instance, &[])
}
pub(crate) fn project_update_policies(
    paths: &Paths,
    directory: &std::path::Path,
) -> Result<Vec<ContentUpdatePolicy>, CoreError> {
    update::policies(paths, directory)
}
pub(crate) fn append_project_history(
    paths: &Paths,
    directory: &std::path::Path,
    event: ContentHistory,
) -> Result<(), CoreError> {
    manage::append_history(paths, directory, event)
}
use provider::ContentProvider;
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Default)]
struct State {
    dependency_plans: HashMap<String, dependencies::Proof>,
    world_targets: HashMap<String, (String, String)>,
    world_plans: HashMap<String, (Instant, worlds::Prepared, Vec<u8>, Vec<u8>)>,
    job: Option<ContentJob>,
    plans: HashMap<String, (Instant, ContentPlan, Vec<u8>)>,
    local_plans: HashMap<String, (Instant, LocalContentPlan, Vec<u8>, Vec<u8>)>,
    updates: HashMap<String, Vec<ContentRecord>>,
    adoption_plans: HashMap<String, (Instant, ContentAdoptionPlan, Vec<u8>, Vec<u8>)>,
}
#[derive(Clone)]
pub struct ContentManager {
    automatic: Arc<Mutex<automatic::Runtime>>,
    library: Library,
    provider: Arc<dyn ContentProvider>,
    state: Arc<Mutex<State>>,
    cancel: Arc<AtomicBool>,
}
impl ContentManager {
    pub fn dependency_plan(
        &self,
        id: &str,
        files: Vec<ContentSelection>,
    ) -> Result<LocalDependencyPlan, CoreError> {
        if files.is_empty() || files.len() > 16 {
            return Err(CoreError::InvalidInput);
        }
        let (instance, directory, _lease) = self.library.lease_game(id)?;
        let paths = Paths::new(self.library.root())?;
        let installed = install::records(&paths, &directory)?;
        let mut parents = vec![];
        for file in &files {
            let record = installed
                .iter()
                .find(|r| {
                    r.directory == file.directory
                        && r.file.filename == file.filename
                        && r.file.hashes.sha512 == file.sha512
                })
                .ok_or(CoreError::RecordConflict)?;
            if record.directory != "mods" || record.kind != "mod" {
                return Err(CoreError::ContentUnsupported);
            }
            if !install::verified(&install::target(&paths, &directory, record)?, &record.file)? {
                return Err(CoreError::SourceChanged);
            }
            parents.push(
                dependencies::matched(self.provider.as_ref(), &instance, record)?
                    .ok_or(CoreError::ContentUnsupported)?,
            );
        }
        let mut plan =
            dependencies::resolve(self.provider.as_ref(), &instance, &installed, &parents)?;
        plan.already_installed = install::preflight(&paths, &directory, &plan.files, &installed)?;
        self.save_plan(&plan, serde_json::to_vec(&installed)?);
        self.state.lock().unwrap().dependency_plans.insert(
            plan.token.clone(),
            (
                files,
                serde_json::to_vec(&diagnostics::scan(&paths, &directory, &instance, &[])?)?,
                serde_json::to_vec(&instance)?,
            ),
        );
        Ok(LocalDependencyPlan { plan, parents })
    }
    pub fn local_dependencies(&self, token: &str) -> Result<LocalContentPlan, CoreError> {
        let (created, original, receipt, context) = self
            .state
            .lock()
            .unwrap()
            .local_plans
            .get(token)
            .cloned()
            .ok_or(CoreError::NotFound)?;
        let (instance, directory, _lease) = self.library.lease_game(&original.plan.instance_id)?;
        let paths = Paths::new(self.library.root())?;
        let installed = install::records(&paths, &directory)?;
        if created.elapsed() > Duration::from_secs(900)
            || serde_json::to_vec(&instance)? != context
            || serde_json::to_vec(&installed)? != receipt
            || serde_json::to_vec(&dependencies::staged_diagnostics(
                &paths,
                &directory,
                &instance,
                &original.plan,
            )?)? != serde_json::to_vec(&original.diagnostics)?
        {
            return Err(CoreError::RecordConflict);
        }
        if original.plan.files.iter().any(|r| r.provider != "local") {
            return Err(CoreError::InvalidInput);
        }
        let mut roots = original.plan.files.clone();
        let mut parents = vec![];
        let mut warnings = vec![];
        for record in &mut roots {
            if let Some(found) = dependencies::matched(self.provider.as_ref(), &instance, record)? {
                parents.push(found.clone());
                *record = found;
            } else {
                warnings.push("match_not_found".into());
            }
        }
        let mut plan =
            dependencies::resolve(self.provider.as_ref(), &instance, &installed, &parents)?;
        install::preflight(&paths, &directory, &plan.files, &installed)?;
        plan.files.retain(|r| {
            !installed.iter().any(|old| {
                old.directory == r.directory
                    && old.file.filename == r.file.filename
                    && old.file.hashes.sha512 == r.file.hashes.sha512
            })
        });
        let root_count = roots.len();
        roots.extend(plan.files);
        let mut plan = original.plan;
        plan.files = roots;
        plan.total_bytes = plan.files.iter().map(|r| r.file.size).sum();
        let network = Network::new()?;
        for (index, record) in plan.files.iter().enumerate() {
            let staged = install::stage(&paths, &directory, token, index)?;
            if index < root_count {
                continue;
            }
            if !install::verified(&staged, &record.file)? {
                network.download(
                    &paths,
                    &Download {
                        url: record.file.url.clone(),
                        path: staged.clone(),
                        hash: Hash::Sha512(record.file.hashes.sha512.clone()),
                        size: record.file.size,
                    },
                    &|| false,
                    &|_| {},
                )?;
                if !install::verified(&staged, &record.file)? {
                    return Err(CoreError::Integrity);
                }
            }
        }
        let report = dependencies::staged_diagnostics(&paths, &directory, &instance, &plan)?;
        for row in report.mods.iter().filter(|m| {
            plan.files
                .iter()
                .any(|r| r.directory == m.directory && r.file.filename == m.filename)
        }) {
            warnings.extend(row.warnings.iter().cloned());
        }
        if diagnostics::selected_issues(&report, &plan.files) {
            warnings.push("dependency_issues".into());
        }
        dependencies::block_known_errors(&report, &dependencies::selections(&plan.files))?;
        warnings.sort();
        warnings.dedup();
        let value = LocalContentPlan {
            plan,
            diagnostics: report,
            warnings,
        };
        self.state
            .lock()
            .unwrap()
            .local_plans
            .insert(token.into(), (created, value.clone(), receipt, context));
        Ok(value)
    }
    pub fn worlds(&self, id: &str) -> Result<Vec<ContentWorld>, CoreError> {
        let (directory, _lease) = self.library.lease_content_read(id)?;
        worlds::list(&Paths::new(self.library.root())?, &directory)
    }
    pub fn world_plan(&self, request: WorldArchiveRequest) -> Result<WorldArchivePlan, CoreError> {
        let (instance, directory, _lease) = self.library.lease_game(&request.instance_id)?;
        let paths = Paths::new(self.library.root())?;
        let mut state = self.state.lock().unwrap();
        let old: Vec<_> = state
            .world_plans
            .iter()
            .filter(|(_, (_, p, _, _))| p.plan.instance_id == instance.id)
            .map(|(token, _)| token.clone())
            .collect();
        for token in old {
            state.world_plans.remove(&token);
            worlds::cancel(&paths, &directory, &token)?;
        }
        if state.world_plans.len() >= 8 {
            return Err(CoreError::InstanceBusy);
        }
        let prepared = worlds::prepare(&paths, &directory, request)?;
        let plan = prepared.plan.clone();
        state.world_plans.insert(
            plan.token.clone(),
            (
                Instant::now(),
                prepared,
                serde_json::to_vec(&instance)?,
                serde_json::to_vec(&install::records(&paths, &directory)?)?,
            ),
        );
        Ok(plan)
    }
    pub fn world_finish(
        &self,
        token: &str,
        accept_unknown: bool,
        cancel: bool,
    ) -> Result<(), CoreError> {
        let mut state = self.state.lock().unwrap();
        let (created, prepared, context, receipt) =
            state.world_plans.get(token).ok_or(CoreError::NotFound)?;
        let (instance, directory, _lease) = self.library.lease_game(&prepared.plan.instance_id)?;
        let paths = Paths::new(self.library.root())?;
        if !cancel {
            if created.elapsed() > Duration::from_secs(900)
                || serde_json::to_vec(&instance)? != *context
                || serde_json::to_vec(&install::records(&paths, &directory)?)? != *receipt
            {
                return Err(CoreError::RecordConflict);
            }
            if !prepared.plan.warnings.is_empty() && !accept_unknown {
                return Err(CoreError::ContentIncompatible);
            }
            worlds::finish(&paths, &directory, prepared)?;
        } else {
            worlds::cancel(&paths, &directory, token)?;
        }
        state.world_plans.remove(token);
        Ok(())
    }
    pub fn world_project_plan(
        &self,
        request: WorldProjectRequest,
    ) -> Result<ContentPlan, CoreError> {
        let (instance, directory, _lease) = self.library.lease_game(&request.instance_id)?;
        let paths = Paths::new(self.library.root())?;
        let plan = worlds::project_plan(
            self.provider.as_ref(),
            &paths,
            &directory,
            &instance,
            request,
        )?;
        self.save_plan(
            &plan,
            serde_json::to_vec(&install::records(&paths, &directory)?)?,
        );
        let folder = plan
            .files
            .iter()
            .find(|r| worlds::datapack_directory(&r.directory))
            .ok_or(CoreError::InvalidInput)?
            .directory
            .split('/')
            .nth(1)
            .ok_or(CoreError::UnsafePath)?
            .to_string();
        self.state.lock().unwrap().world_targets.insert(
            plan.token.clone(),
            (folder.clone(), worlds::marker(&paths, &directory, &folder)?),
        );
        Ok(plan)
    }
    pub fn diagnostics(&self, id: &str) -> Result<ModDiagnostics, CoreError> {
        let (directory, _lease) = self.library.lease_content_read(id)?;
        diagnostics::scan(
            &Paths::new(self.library.root())?,
            &directory,
            &self.instance(id)?,
            &[],
        )
    }
    pub fn adoption_plan(
        &self,
        request: ContentAdoptionRequest,
    ) -> Result<ContentAdoptionPlan, CoreError> {
        let (instance, directory, _lease) = self.library.lease_game(&request.instance_id)?;
        let paths = Paths::new(self.library.root())?;
        let plan = adopt::prepare(
            &paths,
            &directory,
            &instance,
            &request,
            self.provider.as_ref(),
        )?;
        let mut state = self.state.lock().unwrap();
        state.adoption_plans.retain(|_, (created, p, _, _)| {
            created.elapsed() < Duration::from_secs(900) && p.plan.instance_id != instance.id
        });
        if state.adoption_plans.len() >= 8 {
            return Err(CoreError::InstanceBusy);
        }
        state.adoption_plans.insert(
            plan.plan.token.clone(),
            (
                Instant::now(),
                plan.clone(),
                serde_json::to_vec(&install::records(&paths, &directory)?)?,
                serde_json::to_vec(&instance)?,
            ),
        );
        Ok(plan)
    }
    pub fn adoption_finish(
        &self,
        token: &str,
        accept_unknown: bool,
        cancel: bool,
    ) -> Result<(), CoreError> {
        if cancel {
            self.state.lock().unwrap().adoption_plans.remove(token);
            return Ok(());
        }
        let (created, plan, receipt, context) = self
            .state
            .lock()
            .unwrap()
            .adoption_plans
            .get(token)
            .cloned()
            .ok_or(CoreError::NotFound)?;
        let (instance, directory, _lease) = self.library.lease_game(&plan.plan.instance_id)?;
        let paths = Paths::new(self.library.root())?;
        if created.elapsed() > Duration::from_secs(900)
            || serde_json::to_vec(&instance)? != context
            || serde_json::to_vec(&install::records(&paths, &directory)?)? != receipt
        {
            return Err(CoreError::RecordConflict);
        }
        if !plan.warnings.is_empty() && !accept_unknown {
            return Err(CoreError::ContentIncompatible);
        }
        let record = plan.plan.files.first().ok_or(CoreError::Integrity)?;
        adopt::preflight(&paths, &directory, &instance, record)?;
        if record.kind == "mod"
            && serde_json::to_vec(&diagnostics::scan(&paths, &directory, &instance, &[])?)?
                != serde_json::to_vec(&plan.diagnostics)?
        {
            return Err(CoreError::RecordConflict);
        }
        let file = install::target(&paths, &directory, record)?;
        if crate::game::skinmod::owned(&file)? || !install::verified(&file, &record.file)? {
            return Err(CoreError::SourceChanged);
        }
        adopt::commit(&paths, &directory, record)?;
        self.state.lock().unwrap().adoption_plans.remove(token);
        Ok(())
    }
    pub fn local_icon(
        &self,
        id: &str,
        folder: &str,
        filename: &str,
        sha512: Option<&str>,
    ) -> Result<Option<String>, CoreError> {
        let (directory, _lease) = self.library.lease_content_read(id)?;
        let paths = Paths::new(self.library.root())?;
        let path = adopt::source(&paths, &directory, folder, filename)?;
        if let Some(hash) = sha512 {
            let record = install::records(&paths, &directory)?
                .into_iter()
                .find(|r| {
                    r.directory == folder
                        && r.file.filename == filename
                        && r.file.hashes.sha512 == hash
                })
                .ok_or(CoreError::NotFound)?;
            if !install::verified(&path, &record.file)? {
                return Err(CoreError::SourceChanged);
            }
        }
        icons::read(&path, folder)
    }
    pub fn updates(&self, id: &str) -> Result<Vec<ContentUpdate>, CoreError> {
        let (directory, lease) = self.library.lease_content_read(id)?;
        let paths = Paths::new(self.library.root())?;
        let installed = install::records(&paths, &directory)?;
        let policies = update::policies(&paths, &directory)?;
        let instance = self.instance(id)?;
        let project = crate::projects::manifest(&paths, &directory)?;
        drop(lease); // Provider availability must never hold the instance's play lock.
        let mut result = update::check(self.provider.as_ref(), &instance, &installed, &policies);
        if let Some(project) = project {
            for item in &mut result {
                if installed.iter().any(|r| {
                    r.project_id == item.project_id
                        && project.files.iter().any(|f| {
                            f.path == format!("{}/{}", r.directory, r.file.filename)
                                && f.policy == crate::projects::model::FilePolicy::RequiredLocked
                        })
                }) {
                    item.status = "locked".into();
                }
            }
        }
        Ok(result)
    }
    pub fn restore_points(&self, id: &str) -> Result<Vec<ContentRestorePoint>, CoreError> {
        let (directory, _lease) = self.library.lease_content_read(id)?;
        let instance = self.instance(id)?;
        let paths = Paths::new(self.library.root())?;
        let mut result = restore::points(&Paths::new(self.library.root())?, &directory, &instance)?;
        result.extend(crate::projects::restore_points(
            &paths, &directory, &instance,
        )?);
        result.sort_by(|a, b| b.timestamp.cmp(&a.timestamp).then_with(|| a.id.cmp(&b.id)));
        Ok(result)
    }
    pub fn restore_content(&self, id: &str, point: &str) -> Result<(), CoreError> {
        self.restore_content_options(id, point, false)
    }
    pub fn restore_content_options(
        &self,
        id: &str,
        point: &str,
        settings: bool,
    ) -> Result<(), CoreError> {
        let (instance, directory, _lease) = self.library.lease_game(id)?;
        let paths = Paths::new(self.library.root())?;
        if crate::projects::transaction::root(&directory, point)?
            .join("snapshot.json")
            .exists()
        {
            crate::projects::transaction::restore(&paths, &directory, &instance, point, settings)?;
            return Ok(());
        }
        if settings {
            return restore::restore_with_settings(&paths, &directory, &instance, point);
        }
        restore::restore(
            &Paths::new(self.library.root())?,
            &directory,
            &instance,
            point,
        )
    }
    pub fn update_policy(&self, id: &str, policy: ContentUpdatePolicy) -> Result<(), CoreError> {
        let (_, directory, _lease) = self.library.lease_game(id)?;
        update::set_policy(&Paths::new(self.library.root())?, &directory, policy)
    }
    pub fn update_plan(
        &self,
        request: ContentUpdateRequest,
    ) -> Result<ContentUpdatePlan, CoreError> {
        let (instance, directory, _lease) = self.library.lease_game(&request.instance_id)?;
        let paths = Paths::new(self.library.root())?;
        let installed = install::records(&paths, &directory)?;
        let policies = update::policies(&paths, &directory)?;
        let result = update::plan(
            self.provider.as_ref(),
            &instance,
            &installed,
            &policies,
            &request.files,
        )?;
        update::preflight(
            &paths,
            &directory,
            &installed,
            &result.previous,
            &result.plan.files,
        )?;
        self.save_plan(&result.plan, serde_json::to_vec(&installed)?);
        self.state
            .lock()
            .unwrap()
            .updates
            .insert(result.plan.token.clone(), result.previous.clone());
        Ok(result)
    }
    pub fn local_plan(
        &self,
        id: &str,
        sources: Vec<String>,
    ) -> Result<LocalContentPlan, CoreError> {
        let (instance, directory, _lease) = self.library.lease_game(id)?;
        let paths = Paths::new(self.library.root())?;
        let mut state = self.state.lock().unwrap();
        let old: Vec<_> = state
            .local_plans
            .iter()
            .filter(|(_, (_, p, _, _))| p.plan.instance_id == id)
            .map(|(t, _)| t.clone())
            .collect();
        for token in old {
            state.local_plans.remove(&token);
            paths.remove(&directory.join(".sporium/content-staging").join(token))?;
        }
        if state.local_plans.len() >= 8 {
            return Err(CoreError::InstanceBusy);
        }
        drop(state);
        let plan = local::prepare(&paths, &directory, &instance, sources)?;
        self.state.lock().unwrap().local_plans.insert(
            plan.plan.token.clone(),
            (
                Instant::now(),
                plan.clone(),
                serde_json::to_vec(&install::records(&paths, &directory)?)?,
                serde_json::to_vec(&instance)?,
            ),
        );
        Ok(plan)
    }
    pub fn local_finish(
        &self,
        token: &str,
        accept_unknown: bool,
        cancel: bool,
    ) -> Result<(), CoreError> {
        let (created, plan, receipt, context) = self
            .state
            .lock()
            .unwrap()
            .local_plans
            .get(token)
            .cloned()
            .ok_or(CoreError::NotFound)?;
        let (instance, directory, _lease) = self.library.lease_game(&plan.plan.instance_id)?;
        let paths = Paths::new(self.library.root())?;
        if !cancel {
            if created.elapsed() > Duration::from_secs(900)
                || serde_json::to_vec(&instance)? != context
                || serde_json::to_vec(&install::records(&paths, &directory)?)? != receipt
            {
                return Err(CoreError::RecordConflict);
            }
            if !plan.warnings.is_empty() && !accept_unknown {
                return Err(CoreError::ContentIncompatible);
            }
            let mut ids = local::scan_ids(&paths, &directory, &instance)?;
            if serde_json::to_vec(&dependencies::staged_diagnostics(
                &paths, &directory, &instance, &plan.plan,
            )?)? != serde_json::to_vec(&plan.diagnostics)?
            {
                return Err(CoreError::RecordConflict);
            }
            for (index, record) in plan.plan.files.iter().enumerate() {
                if !install::verified(
                    &install::stage(&paths, &directory, token, index)?,
                    &record.file,
                )? {
                    return Err(CoreError::Integrity);
                }
                if record.kind != "mod" {
                    continue;
                }
                for id in &local::inspect(
                    &install::stage(&paths, &directory, token, index)?,
                    &instance,
                )?
                .2
                .mod_ids
                {
                    if !ids.insert(id.clone()) {
                        return Err(CoreError::ContentConflict);
                    }
                }
            }
            install::commit(&paths, &directory, &plan.plan)?;
        }
        self.state.lock().unwrap().local_plans.remove(token);
        paths.remove(&directory.join(".sporium/content-staging").join(token))?;
        Ok(())
    }
    pub fn new(library: Library) -> Result<Self, CoreError> {
        Ok(Self {
            automatic: automatic::runtime(),
            provider: Arc::new(modrinth::Modrinth::new(library.root().into())?),
            library,
            state: Arc::new(Mutex::new(State::default())),
            cancel: Arc::new(AtomicBool::new(false)),
        })
    }
    fn instance(&self, id: &str) -> Result<Instance, CoreError> {
        self.library
            .snapshot()?
            .instances
            .into_iter()
            .find(|i| i.id == id)
            .ok_or(CoreError::NotFound)
    }
    pub fn search(&self, mut query: CatalogQuery) -> Result<CatalogPage, CoreError> {
        if query.query.len() > 160
            || query.offset > 10000
            || !matches!(
                query.kind.as_str(),
                "" | "mod" | "resourcepack" | "shader" | "modpack" | "plugin" | "datapack"
            )
            || !matches!(
                query.sort.as_str(),
                "relevance" | "downloads" | "follows" | "newest" | "updated"
            )
            || !matches!(
                query.environment.as_str(),
                "" | "client" | "both" | "server"
            )
        {
            return Err(CoreError::InvalidInput);
        }
        for filter in [&query.minecraft, &query.loader, &query.category] {
            if filter.len() > 80
                || !filter
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._- +".contains(&b))
            {
                return Err(CoreError::InvalidInput);
            }
        }
        let instance = query
            .instance_id
            .as_deref()
            .map(|id| self.instance(id))
            .transpose()?;
        if let Some(instance) = &instance {
            query.minecraft = instance.minecraft_version.clone();
            query.loader = if query.kind == "datapack" {
                "datapack".into()
            } else if query.kind == "mod" {
                resolve::loader_name(instance.loader).into()
            } else {
                String::new()
            };
            // Project-level environments can be unknown even for valid client files.
            // The concrete version and legacy side fallback are checked below instead.
            query.environment.clear();
        }
        if query.kind == "datapack" {
            query.loader = "datapack".into();
            query.environment.clear();
        }
        let mut page = self.provider.search(&query)?;
        if let Some(instance) = &instance {
            let mut compatible = vec![];
            for hit in page.hits {
                if !resolve::installable(&hit.kind) {
                    continue;
                }
                let project = self.provider.project(&hit.id)?;
                let versions = self
                    .provider
                    .versions(&hit.id, &instance.minecraft_version)?;
                if versions.iter().any(|v| {
                    resolve::compatible(&project, v, instance)
                        && if worlds::is_datapack(&project, v) {
                            query.kind == "datapack"
                                && worlds::selected(&project, v, "validation", false).is_ok()
                        } else {
                            query.kind != "datapack"
                                && resolve::selected_files(&project, v, false).is_ok()
                        }
                }) {
                    compatible.push(hit);
                }
            }
            page.hits = compatible;
        }
        Ok(page)
    }
    pub fn tags(&self) -> Result<ContentTags, CoreError> {
        self.provider.tags()
    }
    pub fn icon(&self, project_id: &str) -> Result<Option<String>, CoreError> {
        Ok(self.provider.project(project_id)?.icon_url)
    }
    pub fn details(
        &self,
        project_id: &str,
        instance_id: Option<&str>,
    ) -> Result<ContentDetails, CoreError> {
        let project = self.provider.project(project_id)?;
        let instances = self.library.snapshot()?.instances;
        let instance = instance_id
            .map(|id| {
                instances
                    .iter()
                    .find(|i| i.id == id)
                    .ok_or(CoreError::NotFound)
            })
            .transpose()?;
        let mut versions = self.provider.versions(
            &project.id,
            instance.map(|i| i.minecraft_version.as_str()).unwrap_or(""),
        )?;
        versions.retain(|v| {
            v.project_id == project.id
                && matches!(v.status.as_str(), "listed" | "archived" | "unlisted")
        });
        let compatible_instances = instances
            .iter()
            .filter(|i| {
                versions.iter().any(|v| {
                    resolve::compatible(&project, v, i)
                        && (resolve::selected_files(&project, v, false).is_ok()
                            || (worlds::is_datapack(&project, v)
                                && worlds::selected(&project, v, "validation", false).is_ok()))
                })
            })
            .map(|i| i.id.clone())
            .collect();
        if let Some(instance) = instance {
            versions.retain(|v| {
                resolve::compatible(&project, v, instance)
                    && (resolve::selected_files(&project, v, false).is_ok()
                        || (worlds::is_datapack(&project, v)
                            && worlds::selected(&project, v, "validation", false).is_ok()))
            });
        }
        versions.sort_by(|a, b| b.date_published.cmp(&a.date_published));
        Ok(ContentDetails {
            project,
            versions,
            compatible_instances,
        })
    }
    pub fn plan(&self, request: ContentRequest) -> Result<ContentPlan, CoreError> {
        let (instance, directory, _lease) = self.library.lease_game(&request.instance_id)?;
        let paths = Paths::new(self.library.root())?;
        let installed = install::records(&paths, &directory)?;
        let mut plan = resolve::resolve(self.provider.as_ref(), &request, &instance, &installed)?;
        plan.already_installed = install::preflight(&paths, &directory, &plan.files, &installed)?;
        self.save_plan(&plan, serde_json::to_vec(&installed)?);
        Ok(plan)
    }
    pub fn create_plan(&self, request: ContentCreateRequest) -> Result<ContentPlan, CoreError> {
        let instance = Library::draft(&request.instance)?;
        let mut plan = resolve::resolve(
            self.provider.as_ref(),
            &ContentRequest {
                instance_id: instance.id.clone(),
                project_id: request.project_id,
                version_id: request.version_id,
            },
            &instance,
            &[],
        )?;
        // Planning is read-only: an incompatible project or cancelled dialog creates no instance.
        plan.new_instance = Some(request.instance);
        self.save_plan(&plan, serde_json::to_vec(&Vec::<ContentRecord>::new())?);
        Ok(plan)
    }
    fn save_plan(&self, plan: &ContentPlan, receipt: Vec<u8>) {
        let mut state = self.state.lock().unwrap();
        state
            .plans
            .retain(|_, (created, _, _)| created.elapsed() < Duration::from_secs(900));
        if state.plans.len() >= 8 {
            state.plans.clear();
        }
        let tokens: std::collections::HashSet<_> = state.plans.keys().cloned().collect();
        state.updates.retain(|token, _| tokens.contains(token));
        state
            .world_targets
            .retain(|token, _| tokens.contains(token));
        state
            .dependency_plans
            .retain(|token, _| tokens.contains(token));
        state
            .plans
            .insert(plan.token.clone(), (Instant::now(), plan.clone(), receipt));
    }
    pub fn untracked(&self, id: &str) -> Result<Vec<UntrackedContent>, CoreError> {
        let (directory, _lease) = self.library.lease_content_read(id)?;
        untracked::scan(
            &Paths::new(self.library.root())?,
            &directory,
            &self.instance(id)?,
        )
    }
    pub fn installed(&self, id: &str) -> Result<Vec<InstalledContent>, CoreError> {
        let (directory, _lease) = self.library.lease_content_read(id)?;
        let paths = Paths::new(self.library.root())?;
        install::records(&paths, &directory)?
            .into_iter()
            .map(|record| {
                let target = install::target(&paths, &directory, &record)?;
                let status = if !target.exists() {
                    "missing"
                } else if install::verified(&target, &record.file)? {
                    if record.directory == "mods_disabled" {
                        "disabled"
                    } else {
                        "installed"
                    }
                } else {
                    "modified"
                };
                Ok(InstalledContent {
                    record,
                    status: status.into(),
                })
            })
            .collect()
    }
    pub fn change(&self, request: ContentChange) -> Result<(), CoreError> {
        let (_, directory, _lease) = self.library.lease_game(&request.instance_id)?;
        manage::change(&Paths::new(self.library.root())?, &directory, request)
    }
    pub fn history(&self, id: &str) -> Result<Vec<ContentHistory>, CoreError> {
        let (directory, _lease) = self.library.lease_content_read(id)?;
        manage::history(&Paths::new(self.library.root())?, &directory)
    }
    pub fn snapshot(&self) -> Option<ContentJob> {
        self.state.lock().unwrap().job.clone()
    }
    pub fn is_active(&self) -> bool {
        self.snapshot().is_some_and(|j| {
            matches!(
                j.phase.as_str(),
                "downloading" | "applying" | "preparing_game"
            )
        })
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
    pub fn start(&self, token: &str) -> Result<ContentJob, CoreError> {
        self.start_with_game(token, None)
    }
    pub fn start_with_game(
        &self,
        token: &str,
        game: Option<crate::game::GameManager>,
    ) -> Result<ContentJob, CoreError> {
        let mut state = self.state.lock().unwrap();
        if state.job.as_ref().is_some_and(|j| {
            matches!(
                j.phase.as_str(),
                "downloading" | "applying" | "preparing_game"
            )
        }) {
            return Err(CoreError::InstanceBusy);
        }
        let (created, mut plan, receipt) =
            state.plans.get(token).cloned().ok_or(CoreError::NotFound)?;
        let previous = state.updates.get(token).cloned();
        let dependency_context = state.dependency_plans.get(token).cloned();
        let world_context = state.world_targets.get(token).cloned();
        if created.elapsed() > Duration::from_secs(900) {
            return Err(CoreError::RecordConflict);
        }
        let paths = Paths::new(self.library.root())?;
        let download_lock = paths.named_lock(&paths.root().join("shared/cache/downloads.lock"))?;
        if let Some(request) = plan.new_instance.clone() {
            if game.is_none() {
                return Err(CoreError::InvalidInput);
            }
            let icon = plan
                .files
                .iter()
                .find(|r| !r.dependency)
                .and_then(|r| r.icon_url.clone());
            let change = self.library.create_with_icon(request, icon)?;
            plan.instance_id = change.affected_id;
            plan.new_instance = None;
            // Keep a retry bound to this same instance if acquiring its lease fails.
            state
                .plans
                .insert(token.into(), (created, plan.clone(), receipt.clone()));
        }
        let (instance, directory, lease) = self.library.lease_game(&plan.instance_id)?;
        let prepare_game = game.is_some()
            && instance.status == crate::instances::model::InstallStatus::NotInstalled
            && !plan
                .files
                .iter()
                .any(|r| worlds::datapack_directory(&r.directory));
        let installed = install::records(&paths, &directory)?;
        if serde_json::to_vec(&installed)? != receipt {
            return Err(CoreError::RecordConflict);
        }
        if let Some(proof) = &dependency_context {
            dependencies::verify_context(&paths, &directory, &instance, proof)?;
        }
        if let Some((world, hash)) = &world_context
            && worlds::marker(&paths, &directory, world)? != *hash
        {
            return Err(CoreError::RecordConflict);
        }
        if let Some(previous) = &previous {
            update::preflight(&paths, &directory, &installed, previous, &plan.files)?;
            let policies = update::policies(&paths, &directory)?;
            if plan.files.iter().any(|r| {
                policies.iter().any(|p| {
                    p.project_id == r.project_id
                        && (p.pinned || p.ignored_version.as_ref() == Some(&r.version.id))
                })
            }) {
                return Err(CoreError::RecordConflict);
            }
        } else {
            install::preflight(&paths, &directory, &plan.files, &installed)?;
        }
        for record in &plan.files {
            if !record
                .version
                .game_versions
                .contains(&instance.minecraft_version)
                || (record.kind == "mod"
                    && !record
                        .version
                        .loaders
                        .iter()
                        .any(|s| s == resolve::loader_name(instance.loader)))
            {
                return Err(CoreError::ContentIncompatible);
            }
        }
        self.cancel.store(false, Ordering::SeqCst);
        let job = ContentJob {
            instance_id: plan.instance_id.clone(),
            phase: "downloading".into(),
            completed_files: 0,
            total_files: plan.files.len() as u32,
            downloaded_bytes: 0,
            total_bytes: plan.total_bytes,
            current_file: None,
            error: None,
        };
        state.job = Some(job.clone());
        state.plans.remove(token);
        state.updates.remove(token);
        state.dependency_plans.remove(token);
        state.world_targets.remove(token);
        drop(state);
        let manager = self.clone();
        let spawn = std::thread::Builder::new()
            .name("sporium-content-install".into())
            .spawn(move || {
                let _lease = lease;
                let result = (|| {
                    let network = Network::new()?;
                    for (index, record) in plan.files.iter().enumerate() {
                        if manager.cancel.load(Ordering::SeqCst) {
                            return Err(CoreError::Cancelled);
                        }
                        let staged = install::stage(&paths, &directory, &plan.token, index)?;
                        if let Some(job) = &mut manager.state.lock().unwrap().job {
                            job.current_file = Some(record.file.filename.clone());
                        }
                        let target = install::target(&paths, &directory, record)?;
                        if target.exists() && install::verified(&target, &record.file)? {
                            paths.mkdir(staged.parent().ok_or(CoreError::UnsafePath)?)?;
                            std::fs::copy(target, &staged)?;
                        } else {
                            let cached = paths.checked(
                                &paths
                                    .root()
                                    .join("shared/cache/modrinth/artifacts")
                                    .join(format!("{}.bin", record.file.hashes.sha512)),
                            )?;
                            network.download(
                                &paths,
                                &Download {
                                    url: record.file.url.clone(),
                                    path: cached.clone(),
                                    hash: Hash::Sha512(record.file.hashes.sha512.clone()),
                                    size: record.file.size,
                                },
                                &|| manager.cancel.load(Ordering::SeqCst),
                                &|bytes| {
                                    if let Some(job) = &mut manager.state.lock().unwrap().job {
                                        job.downloaded_bytes += bytes;
                                    }
                                },
                            )?;
                            paths.mkdir(staged.parent().ok_or(CoreError::UnsafePath)?)?;
                            std::fs::copy(cached, &staged)?;
                        }
                        if !install::verified(&staged, &record.file)? {
                            return Err(CoreError::Integrity);
                        }
                        if record.kind == "datapack" {
                            worlds::validate_datapack(&staged)?;
                        }
                        std::fs::OpenOptions::new()
                            .read(true)
                            .write(true)
                            .open(&staged)?
                            .sync_all()?;
                        if let Some(job) = &mut manager.state.lock().unwrap().job {
                            job.completed_files += 1;
                        }
                    }
                    if manager.cancel.load(Ordering::SeqCst) {
                        return Err(CoreError::Cancelled);
                    }
                    if let Some(job) = &mut manager.state.lock().unwrap().job {
                        job.phase = "applying".into();
                        job.current_file = None;
                    }
                    if let Some(proof) = &dependency_context {
                        dependencies::verify_context(&paths, &directory, &instance, proof)?;
                        let report =
                            dependencies::staged_diagnostics(&paths, &directory, &instance, &plan)?;
                        let mut selected = proof.0.clone();
                        selected.extend(dependencies::selections(&plan.files));
                        dependencies::block_known_errors(&report, &selected)?;
                    }
                    if let Some((world, hash)) = &world_context
                        && worlds::marker(&paths, &directory, world)? != *hash
                    {
                        return Err(CoreError::RecordConflict);
                    }
                    if let Some(previous) = &previous {
                        update::commit(&paths, &directory, &instance, &plan, previous)?;
                    } else {
                        install::commit(&paths, &directory, &plan)?;
                    }
                    drop(download_lock);
                    if prepare_game {
                        if let Some(job) = &mut manager.state.lock().unwrap().job {
                            job.phase = "preparing_game".into();
                        }
                        game.as_ref()
                            .ok_or(CoreError::Worker)?
                            .prepare_content_base(&instance, &directory, manager.cancel.clone())?;
                    }
                    Ok(())
                })();
                if !directory.join(".sporium/content-pending.json").exists()
                    && !directory.join(".sporium/content-update.json").exists()
                {
                    let _ =
                        paths.remove(&directory.join(".sporium/content-staging").join(&plan.token));
                }
                if let Some(job) = &mut manager.state.lock().unwrap().job {
                    job.current_file = None;
                    match result {
                        Ok(()) => job.phase = "completed".into(),
                        Err(CoreError::Cancelled) => job.phase = "cancelled".into(),
                        Err(error) => {
                            job.phase = "failed".into();
                            job.error = Some(CommandError::from(error));
                        }
                    }
                }
            });
        if spawn.is_err() {
            if let Some(job) = &mut self.state.lock().unwrap().job {
                job.phase = "failed".into();
            }
            return Err(CoreError::Worker);
        }
        Ok(job)
    }
}
