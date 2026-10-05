import { useEffect, useState } from 'react';
import type {
  FilePolicy,
  ProjectFile,
  ProjectPlan,
  ProjectUpdate,
  ProjectView,
} from '../bindings/core';
import { useFoundation } from '../app/context';
import { useLibrary } from '../library/context';
import { Modal } from '../components/Modal';
import { ErrorNotice } from '../components/ui';
import { backend, normalizeError, type BackendError } from '../services/backend';
import { AutomaticSettings } from './AutomaticSettings';
import type { MessageKey } from '../i18n/messages';

export function ProjectPanel({ id, disabled }: { id: string; disabled: boolean }) {
  const { t, desktop } = useFoundation();
  const { reload } = useLibrary();
  const [view, setView] = useState<ProjectView | null>(null);
  const [update, setUpdate] = useState<ProjectUpdate | null>(null);
  const [revision, setRevision] = useState(0);
  const [working, setWorking] = useState(false);
  const [error, setError] = useState<BackendError | null>(null);
  const [studio, setStudio] = useState(false);
  const [files, setFiles] = useState<ProjectFile[]>([]);
  const [name, setName] = useState('');
  const [version, setVersion] = useState('1.0.0');
  const [forbid, setForbid] = useState(false);
  const [plan, setPlan] = useState<ProjectPlan | null>(null);
  const [accepted, setAccepted] = useState(false);
  useEffect(() => {
    if (!desktop || disabled) return;
    let active = true;
    void backend
      .projectView(id)
      .then((result) => {
        if (active) setView(result);
      })
      .catch((reason) => {
        if (active) setError(normalizeError(reason));
      });
    return () => {
      active = false;
    };
  }, [id, desktop, disabled, revision]);
  useEffect(() => {
    const changed = () => {
      setRevision((v) => v + 1);
      setUpdate(null);
    };
    window.addEventListener('sporium-project-changed', changed);
    return () => window.removeEventListener('sporium-project-changed', changed);
  }, []);
  async function run<T>(action: () => Promise<T>): Promise<T | undefined> {
    setWorking(true);
    setError(null);
    try {
      return await action();
    } catch (reason) {
      setError(normalizeError(reason));
    } finally {
      setWorking(false);
    }
  }
  async function edit() {
    const value = await run(() => backend.studioFiles(id));
    if (!value) return;
    setFiles(value);
    setName(view?.manifest?.name ?? 'Creator Studio');
    setVersion(view?.manifest?.version ?? '1.0.0');
    setForbid(view?.manifest?.forbidExternalMods ?? false);
    setStudio(true);
  }
  async function prepare(action: () => Promise<ProjectPlan | null>) {
    const value = await run(action);
    if (value) {
      setStudio(false);
      setPlan(value);
      setAccepted(false);
    }
  }
  async function closePlan() {
    if (plan) await backend.projectDismiss(plan.token);
    setPlan(null);
    setAccepted(false);
  }
  async function apply() {
    if (!plan) return;
    const result = await run(() => backend.projectApply(plan.token, accepted));
    if (result) {
      setPlan(null);
      await reload();
      window.dispatchEvent(new Event('sporium-project-changed'));
    }
  }
  const locked = disabled || working || !desktop || !view;
  const policies: Array<[FilePolicy, MessageKey]> = [
    ['REQUIRED_LOCKED', 'project.required'],
    ['OPTIONAL', 'project.optional'],
    ['USER_ALLOWED', 'project.user'],
    ['USER_FORBIDDEN', 'project.forbidden'],
  ];
  return (
    <section className="surface project-panel">
      <div className="content-section-heading">
        <h2>{t('project.title')}</h2>
        <button className="button secondary" disabled={locked} onClick={() => void edit()}>
          {t(view?.manifest?.creatorStudio ? 'project.edit' : 'project.create')}
        </button>
      </div>
      <p className="setting-hint">{t('project.hint')}</p>
      {view?.manifest ? (
        <p>
          <strong>{view.manifest.name}</strong> · {view.manifest.version} ·{' '}
          {view.manifest.files.length} {t('restore.files')}
        </p>
      ) : (
        <p className="setting-hint">{t(view ? 'project.none' : 'project.loading')}</p>
      )}
      {error && !studio && !plan && <ErrorNotice error={error} />}
      <div className="project-actions">
        <button
          className="button secondary"
          disabled={locked || (!view?.manifest && !view?.packSource)}
          onClick={() =>
            void run(() => backend.projectCheck(id)).then((result) => {
              if (result) setUpdate(result);
            })
          }
        >
          {t('project.check')}
        </button>
        <button
          className="button secondary"
          disabled={locked || (!view?.manifest && !view?.packSource)}
          onClick={() => void prepare(() => backend.projectRepairPlan(id))}
        >
          {t('project.repair')}
        </button>
        <button
          className="button secondary"
          disabled={locked}
          onClick={() => void prepare(() => backend.projectPickManifest(id, []))}
        >
          {t('project.import')}
        </button>
        <button
          className="button secondary"
          disabled={locked || !view?.manifest}
          onClick={() => void run(() => backend.projectExport(id))}
        >
          {t('project.export')}
        </button>
        {working && (
          <button className="button secondary" onClick={() => void backend.projectCancel()}>
            {t('project.cancelWork')}
          </button>
        )}
      </div>
      {update && (
        <div className="project-update">
          <p>
            {t(`project.${update.status}` as MessageKey)}
            {update.available && ` · ${update.current} → ${update.available}`}
          </p>
          {update.status === 'available' && (
            <button
              className="button primary"
              disabled={locked}
              onClick={() =>
                void prepare(() =>
                  update.versionId
                    ? backend.projectPackPlan(id, update.versionId)
                    : backend.projectSourcePlan(id, view?.manifest?.optionalGroups ?? []),
                )
              }
            >
              {t('project.update')}
            </button>
          )}
        </div>
      )}
      {view?.manifest && (
        <details>
          <summary>{t('project.details')}</summary>
          <ul className="project-files">
            {view.manifest.files.map((file) => (
              <li key={file.path}>
                <span>{file.path}</span>
                <span>
                  {t(policies.find(([policy]) => policy === file.policy)?.[1] ?? 'project.user')}
                </span>
              </li>
            ))}
          </ul>
        </details>
      )}
      <details className="project-auto">
        <summary>{t('auto.title')}</summary>
        <AutomaticSettings id={id} disabled={locked} />
      </details>
      {studio && (
        <Modal title={t('project.studio')} busy={working} onClose={() => setStudio(false)}>
          <p className="setting-hint">{t('project.studioHint')}</p>
          {error && <ErrorNotice error={error} />}
          <div className="project-form">
            <label>
              {t('project.name')}
              <input
                value={name}
                maxLength={160}
                onChange={(e) => setName(e.target.value)}
                disabled={working}
              />
            </label>
            <label>
              {t('project.version')}
              <input
                value={version}
                maxLength={160}
                onChange={(e) => setVersion(e.target.value)}
                disabled={working}
              />
            </label>
          </div>
          <label className="project-consent">
            <input
              type="checkbox"
              checked={forbid}
              onChange={(e) => setForbid(e.target.checked)}
              disabled={working}
            />
            {t('project.forbid')}
          </label>
          <p className="setting-hint">{t('project.forbidHint')}</p>
          <details>
            <summary>{t('project.details')}</summary>
            <ul className="project-policy-files">
              {files.map((file, index) => (
                <li key={file.path}>
                  <span>{file.path}</span>
                  <select
                    aria-label={`${t('project.policy')}: ${file.path}`}
                    value={file.policy}
                    disabled={working}
                    onChange={(event) =>
                      setFiles((old) =>
                        old.map((value, i) =>
                          i === index
                            ? {
                                ...value,
                                policy: event.target.value as FilePolicy,
                                group:
                                  event.target.value === 'OPTIONAL'
                                    ? (value.group ?? 'extras')
                                    : null,
                              }
                            : value,
                        ),
                      )
                    }
                  >
                    {policies.map(([value, label]) => (
                      <option value={value} key={value}>
                        {t(label)}
                      </option>
                    ))}
                  </select>
                  {file.policy === 'OPTIONAL' && (
                    <input
                      aria-label={`${t('project.group')}: ${file.path}`}
                      value={file.group ?? 'extras'}
                      maxLength={160}
                      onChange={(e) =>
                        setFiles((old) =>
                          old.map((value, i) =>
                            i === index ? { ...value, group: e.target.value } : value,
                          ),
                        )
                      }
                    />
                  )}
                </li>
              ))}
            </ul>
          </details>
          <div className="modal-actions">
            <button
              className="button secondary"
              disabled={working}
              onClick={() => setStudio(false)}
            >
              {t('common.cancel')}
            </button>
            <button
              className="button primary"
              disabled={working || !name.trim() || !version.trim()}
              onClick={() =>
                void prepare(() =>
                  backend.studioPlan({
                    id,
                    name,
                    version,
                    forbidExternalMods: forbid,
                    policies: files.map((file) => ({
                      path: file.path,
                      policy: file.policy,
                      group: file.group,
                    })),
                  }),
                )
              }
            >
              {t('project.preview')}
            </button>
          </div>
        </Modal>
      )}
      {plan && (
        <Modal title={t('project.plan')} busy={working} onClose={() => void closePlan()}>
          <p>
            <strong>
              {plan.current} → {plan.available}
            </strong>
          </p>
          <p className="setting-hint">{t('project.planHint')}</p>
          {error && <ErrorNotice error={error} />}
          <p>
            {t('project.changed')}: {plan.changes.length}
          </p>
          {plan.optionalGroups.length > 0 && (
            <fieldset disabled={working}>
              <legend>{t('project.group')}</legend>
              <p className="setting-hint">{t('project.optionalHint')}</p>
              {plan.optionalGroups.map((group) => (
                <label className="project-consent" key={group}>
                  <input
                    type="checkbox"
                    checked={plan.selectedGroups.includes(group)}
                    onChange={() =>
                      void prepare(() =>
                        backend.projectReplan(
                          plan.token,
                          plan.selectedGroups.includes(group)
                            ? plan.selectedGroups.filter((g) => g !== group)
                            : [...plan.selectedGroups, group],
                        ),
                      )
                    }
                  />
                  {group}
                </label>
              ))}
            </fieldset>
          )}
          <details>
            <summary>{t('project.details')}</summary>
            <ul className="project-plan-files">
              {plan.changes.map((name) => (
                <li key={name}>{name}</li>
              ))}
            </ul>
            {plan.warnings.length > 0 && (
              <ul className="project-plan-files">
                {plan.warnings.map((warning, index) => {
                  const [kind, ...name] = warning.split(':');
                  return (
                    <li key={index}>
                      {t(`project.${kind === 'forbidden' ? 'remove' : kind}` as MessageKey)}:{' '}
                      {kind === 'unverified' ? t('project.unverified') : name.join(':').trim()}
                    </li>
                  );
                })}
              </ul>
            )}
          </details>
          {plan.warnings.length > 0 && (
            <label className="project-consent">
              <input
                type="checkbox"
                checked={accepted}
                onChange={(e) => setAccepted(e.target.checked)}
                disabled={working}
              />
              {t('project.accept')}
            </label>
          )}
          <div className="modal-actions">
            <button
              className="button secondary"
              disabled={working}
              onClick={() => void closePlan()}
            >
              {t('common.cancel')}
            </button>
            <button
              className="button primary"
              disabled={working || (plan.warnings.length > 0 && !accepted)}
              onClick={() => void apply()}
            >
              {t(working ? 'project.applying' : 'project.apply')}
            </button>
          </div>
        </Modal>
      )}
    </section>
  );
}
