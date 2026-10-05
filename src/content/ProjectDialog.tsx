import { useEffect, useState } from 'react';
import type {
  ContentCreateRequest,
  ContentDetails,
  ContentPlan,
  ContentWorld,
} from '../bindings/core';
import { useNavigate } from 'react-router-dom';
import { NewInstanceSetup } from './NewInstanceSetup';
import { ProjectIcon } from './ProjectIcon';
import { useFoundation } from '../app/context';
import { useLibrary } from '../library/context';
import { Modal } from '../components/Modal';
import { ErrorNotice } from '../components/ui';
import { backend, normalizeError } from '../services/backend';
import type { BackendError } from '../services/backend';
import { useContent } from './context';
import { loaderNames } from '../library/view-model';
import { showPackPreview } from '../packs/events';

export function ProjectDialog({
  projectId,
  instanceId,
  onClose,
  stayInContext = false,
  disabled = false,
  projectKind,
}: {
  projectId: string;
  instanceId: string | null;
  onClose: () => void;
  stayInContext?: boolean;
  disabled?: boolean;
  projectKind?: string;
}) {
  const { t } = useFoundation();
  const { snapshot } = useLibrary();
  const content = useContent();
  const navigate = useNavigate();
  const [creation, setCreation] = useState<ContentCreateRequest | null>(null);
  const [details, setDetails] = useState<ContentDetails | null>(null);
  const [target, setTarget] = useState(instanceId ?? '');
  const [version, setVersion] = useState('');
  const [error, setError] = useState<BackendError | null>(null);
  const [worlds, setWorlds] = useState<ContentWorld[]>([]);
  const [world, setWorld] = useState('');
  const datapack =
    details?.versions.find((v) => v.id === version)?.loaders.includes('datapack') === true;
  useEffect(() => {
    if (!target || target === '__new__' || !datapack) return;
    let active = true;
    void backend
      .contentWorlds(target)
      .then((value) => {
        if (active) {
          setWorlds(value);
          setWorld('');
        }
      })
      .catch((reason) => {
        if (active) setError(normalizeError(reason));
      });
    return () => {
      active = false;
    };
  }, [target, datapack]);
  const [plan, setPlan] = useState<ContentPlan | null>(null);
  const [loading, setLoading] = useState(true);
  const [working, setWorking] = useState(false);
  const [attempt, setAttempt] = useState(0);
  // Global details provide the compatible destination list; each choice loads only its versions.
  const [eligible, setEligible] = useState<string[]>([]);
  useEffect(() => {
    let active = true;
    backend
      .contentDetails(projectId, target && target !== '__new__' ? target : null)
      .then((value) => {
        if (!active) return;
        if (projectKind === 'datapack' || projectKind === 'mod')
          value = {
            ...value,
            versions: value.versions.filter(
              (v) => v.loaders.includes('datapack') === (projectKind === 'datapack'),
            ),
          };
        setDetails(value);
        if (!target || target === '__new__') setEligible(value.compatibleInstances);
        setVersion(
          value.versions.find((v) => v.version_type === 'release')?.id ??
            value.versions[0]?.id ??
            '',
        );
        setError(null);
      })
      .catch((reason) => {
        if (active) setError(normalizeError(reason));
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [projectId, target, attempt, projectKind]);
  async function prepare() {
    if (!target || (target === '__new__' ? !creation : !version)) return;
    setWorking(true);
    setError(null);
    try {
      setPlan(
        target === '__new__' && creation
          ? await backend.contentCreatePlan(creation)
          : datapack
            ? await backend.worldProjectPlan({
                instanceId: target,
                projectId,
                versionId: version,
                world,
              })
            : await backend.contentPlan({ instanceId: target, projectId, versionId: version }),
      );
    } catch (reason) {
      setError(normalizeError(reason));
    } finally {
      setWorking(false);
    }
  }
  const busy = working || content.busy || disabled;
  return (
    <Modal title={details?.project.title ?? t('content.loading')} onClose={onClose} busy={working}>
      {loading ? (
        <p role="status">{t('content.loading')}</p>
      ) : (
        details && (
          <>
            <div className="content-project-heading">
              <ProjectIcon
                url={details.project.icon_url}
                title={details.project.title}
                kind={details.project.project_type}
              />
              <p>{details.project.description}</p>
            </div>
            <p className="setting-hint">
              {t('content.license')}: {details.project.license.name || details.project.license.id}
            </p>
            <button
              className="button secondary"
              onClick={() =>
                void backend
                  .openContentProject(projectId)
                  .catch((reason) => setError(normalizeError(reason)))
              }
            >
              {t('content.onSite')}
            </button>
            <details className="content-description">
              <summary>{t('content.descriptionFull')}</summary>
              <p>{details.project.body}</p>
            </details>
            {details.project.project_type === 'modpack' ? (
              <section className="content-plan">
                <p>{t('pack.hint')}</p>
                <label className="field">
                  <span>{t('pack.version')}</span>
                  <select
                    value={version}
                    disabled={working}
                    onChange={(e) => setVersion(e.target.value)}
                  >
                    {details.versions.map((item) => (
                      <option key={item.id} value={item.id}>
                        {item.name} · {item.game_versions.join(', ')}
                      </option>
                    ))}
                  </select>
                </label>
                <button
                  className="button primary"
                  disabled={working || !version}
                  onClick={() => {
                    setWorking(true);
                    setError(null);
                    void backend
                      .providerPackPreview(projectId, version)
                      .then((preview) => {
                        onClose();
                        showPackPreview(preview);
                      })
                      .catch((reason) => setError(normalizeError(reason)))
                      .finally(() => setWorking(false));
                  }}
                >
                  {t(working ? 'pack.reading' : 'pack.import')}
                </button>
              </section>
            ) : (
              <>
                {details.project.project_type === 'shader' && (
                  <p className="setting-hint">{t('content.shaderHint')}</p>
                )}
                {details.project.project_type === 'resourcepack' && (
                  <p className="setting-hint">{t('content.resourceHint')}</p>
                )}
                {plan ? (
                  <section className="content-plan">
                    <h3>{t('content.planTitle')}</h3>
                    {datapack && (
                      <p>
                        {t('worlds.target')}:{' '}
                        {worlds.find((item) => item.id === world)?.title ?? world}
                      </p>
                    )}
                    {plan.newInstance && (
                      <div className="content-create-summary">
                        <strong>
                          {t('content.newInstance')}: {plan.newInstance.name}
                        </strong>
                        <p>
                          Minecraft {plan.newInstance.minecraftVersion} ·{' '}
                          {loaderNames[plan.newInstance.loader]}
                        </p>
                        <p className="setting-hint">{t('content.prepareHint')}</p>
                      </div>
                    )}
                    <ul>
                      {plan.files.map((file) => (
                        <li key={`${file.directory}/${file.file.filename}`}>
                          <ProjectIcon url={file.iconUrl} title={file.title} kind={file.kind} />
                          <div>
                            <strong>{file.title}</strong> · {file.version.version_number}
                            {file.dependency && <small>{t('content.required')}</small>}
                            <details>
                              <summary>{t('content.fileDetails')}</summary>
                              <small>
                                {file.directory}/{file.file.filename}
                              </small>
                            </details>
                          </div>
                        </li>
                      ))}
                    </ul>
                    <p>
                      {t('content.total')}: {(plan.totalBytes / 1048576).toFixed(1)} MB ·{' '}
                      {t('content.existing')}: {plan.alreadyInstalled}
                    </p>
                    {plan.optionalDependencies > 0 && (
                      <p>
                        {t('content.optional')}: {plan.optionalDependencies}
                      </p>
                    )}
                    <p className="setting-hint">{t('content.preserve')}</p>
                    <div className="modal-actions">
                      <button
                        className="button secondary"
                        disabled={busy}
                        onClick={() => setPlan(null)}
                      >
                        {t('content.back')}
                      </button>
                      <button
                        className="button primary"
                        disabled={busy}
                        onClick={() =>
                          void content.install(plan.token).then((ok) => {
                            if (ok) {
                              onClose();
                              if (!stayInContext) void navigate('/downloads');
                            }
                          })
                        }
                      >
                        {t(plan.newInstance ? 'content.createInstall' : 'content.install')}
                      </button>
                    </div>
                  </section>
                ) : (
                  <>
                    <label className="field">
                      <span>{t('content.target')}</span>
                      <select
                        value={target}
                        disabled={!!instanceId || busy}
                        onChange={(event) => {
                          setTarget(event.target.value);
                          setLoading(true);
                          setDetails(null);
                          setPlan(null);
                        }}
                      >
                        <option value="">{t('content.choose')}</option>
                        {!instanceId &&
                          details.project.project_type !== 'shader' &&
                          !details.versions.some((v) => v.loaders.includes('datapack')) && (
                            <option value="__new__">{t('content.newInstance')}</option>
                          )}
                        {snapshot.instances
                          .filter((i) =>
                            instanceId ? i.id === instanceId : eligible.includes(i.id),
                          )
                          .map((i) => (
                            <option key={i.id} value={i.id}>
                              {i.name} · {loaderNames[i.loader]} {i.minecraftVersion}
                            </option>
                          ))}
                      </select>
                    </label>
                    {target === '__new__' && (
                      <NewInstanceSetup
                        details={details}
                        disabled={busy}
                        onChange={setCreation}
                        initial={creation}
                      />
                    )}
                    {target && target !== '__new__' && details.versions.length > 0 && (
                      <label className="field">
                        <span>{t('content.version')}</span>
                        <select
                          value={version}
                          disabled={busy}
                          onChange={(event) => setVersion(event.target.value)}
                        >
                          {details.versions.map((v) => (
                            <option key={v.id} value={v.id}>
                              {v.version_number} · {v.version_type} ·{' '}
                              {v.date_published.slice(0, 10)}
                            </option>
                          ))}
                        </select>
                      </label>
                    )}
                    {((!target && eligible.length === 0) ||
                      (target && details.versions.length === 0)) && (
                      <p>{t('content.noneCompatible')}</p>
                    )}
                    {datapack && target && target !== '__new__' && (
                      <>
                        <label className="field">
                          <span>{t('worlds.target')}</span>
                          <select
                            aria-label={t('worlds.target')}
                            value={world}
                            disabled={busy}
                            onChange={(event) => setWorld(event.target.value)}
                          >
                            <option value="">{t('content.choose')}</option>
                            {worlds.map((item) => (
                              <option key={item.id} value={item.id}>
                                {item.title}
                              </option>
                            ))}
                          </select>
                        </label>
                        <p className="setting-hint">
                          {t(worlds.length ? 'worlds.datapackHint' : 'worlds.none')}
                        </p>
                      </>
                    )}
                    <div className="modal-actions">
                      <button
                        className="button primary"
                        disabled={
                          !target ||
                          (target === '__new__' ? !creation : !version) ||
                          (datapack && !world) ||
                          busy
                        }
                        onClick={() => void prepare()}
                      >
                        {working ? t('content.loading') : t('content.plan')}
                      </button>
                    </div>
                  </>
                )}
              </>
            )}
          </>
        )
      )}
      {error && (
        <ErrorNotice
          error={error}
          action={
            <button
              className="button secondary"
              onClick={() => {
                setLoading(true);
                setAttempt(attempt + 1);
              }}
            >
              {t('error.retry')}
            </button>
          }
        />
      )}
      {content.error && <ErrorNotice error={content.error} />}
    </Modal>
  );
}
