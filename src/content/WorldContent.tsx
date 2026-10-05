import { useEffect, useRef, useState } from 'react';
import type { ContentWorld, WorldArchivePlan } from '../bindings/core';
import { useFoundation } from '../app/context';
import { backend, normalizeError } from '../services/backend';
import type { BackendError } from '../services/backend';
import { Modal } from '../components/Modal';
import { ErrorNotice } from '../components/ui';
import { ProjectIcon } from './ProjectIcon';

export function WorldContent({
  id,
  disabled,
  revision,
  onChanged,
}: {
  id: string;
  disabled: boolean;
  revision: number;
  onChanged: () => void;
}) {
  const { t, desktop } = useFoundation();
  const [worlds, setWorlds] = useState<ContentWorld[]>([]);
  const [opened, setOpened] = useState(false);
  const [kind, setKind] = useState('map');
  const [world, setWorld] = useState('');
  const [title, setTitle] = useState('');
  const [plan, setPlan] = useState<WorldArchivePlan | null>(null);
  const [working, setWorking] = useState(false);
  const [accepted, setAccepted] = useState(false);
  const [error, setError] = useState<BackendError | null>(null);
  const token = useRef<string | null>(null);
  const mounted = useRef(false);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      if (token.current)
        void backend.finishWorldArchive(token.current, false, true).catch(() => {});
    };
  }, []);
  useEffect(() => {
    let active = true;
    void backend
      .contentWorlds(id)
      .then((value) => {
        if (active) setWorlds(value);
      })
      .catch((reason) => {
        if (active) setError(normalizeError(reason));
      });
    return () => {
      active = false;
    };
  }, [id, revision]);
  async function pick() {
    setWorking(true);
    setError(null);
    try {
      const value = await backend.pickWorldArchive({
        instanceId: id,
        source: '',
        kind,
        world: kind === 'datapack' ? world : null,
        title,
      });
      if (!mounted.current) {
        if (value) await backend.finishWorldArchive(value.token, false, true);
        return;
      }
      token.current = value?.token ?? null;
      setPlan(value);
      setAccepted(false);
    } catch (reason) {
      if (mounted.current) setError(normalizeError(reason));
    } finally {
      if (mounted.current) setWorking(false);
    }
  }
  async function finish(cancel: boolean) {
    if (!plan) {
      setOpened(false);
      return;
    }
    setWorking(true);
    setError(null);
    try {
      await backend.finishWorldArchive(plan.token, accepted, cancel);
      token.current = null;
      if (mounted.current) {
        setPlan(null);
        setOpened(false);
        if (!cancel) onChanged();
      }
    } catch (reason) {
      if (mounted.current) setError(normalizeError(reason));
    } finally {
      if (mounted.current) setWorking(false);
    }
  }
  return (
    <section className="world-content">
      <div className="section-heading">
        <h3>
          {t('worlds.title')} · {worlds.length}
        </h3>
        <button
          className="button secondary"
          disabled={disabled || working || !desktop}
          onClick={() => {
            setOpened(true);
            setError(null);
          }}
        >
          {t('worlds.add')}
        </button>
      </div>
      <p className="setting-hint">{t('worlds.hint')}</p>
      <ul className="world-list">
        {worlds.map((item) => (
          <li key={item.id}>
            <ProjectIcon title={item.title} kind="map" />
            <div>
              <strong>{item.title}</strong>
              <small>{t(item.imported ? 'worlds.imported' : 'worlds.existing')}</small>
              <details>
                <summary>{t('content.fileDetails')}</summary>
                <p>saves/{item.id}</p>
                {item.imported && (
                  <>
                    <p>
                      {item.imported.source} · {t('local.source')}
                    </p>
                    <p>{new Date(item.imported.importedAt).toLocaleString()}</p>
                  </>
                )}
              </details>
            </div>
          </li>
        ))}
      </ul>
      {error && !opened && <ErrorNotice error={error} />}
      {opened && (
        <Modal title={t('worlds.add')} busy={working} onClose={() => void finish(true)}>
          {plan ? (
            <>
              <p>
                <strong>{plan.title}</strong> ·{' '}
                {t(plan.kind === 'map' ? 'worlds.map' : 'content.datapack')}
              </p>
              <p>
                {plan.kind === 'map'
                  ? t('worlds.newWorld')
                  : `${t('worlds.target')}: ${worlds.find((item) => item.id === plan.world)?.title || plan.world}`}
              </p>
              <p>
                {(plan.bytes / 1048576).toFixed(1)} MB · {t('worlds.files')}: {plan.files}
              </p>
              <details>
                <summary>{t('content.fileDetails')}</summary>
                <p>{plan.source}</p>
                <p>
                  {plan.kind === 'map' ? `saves/${plan.world}` : `saves/${plan.world}/datapacks`}
                </p>
              </details>
              <div className="content-warning">
                <p>{t('worlds.compatibility')}</p>
                <label>
                  <input
                    type="checkbox"
                    checked={accepted}
                    disabled={working}
                    onChange={(event) => setAccepted(event.target.checked)}
                  />{' '}
                  {t('local.acceptWarnings')}
                </label>
              </div>
            </>
          ) : (
            <>
              <label className="field">
                <span>{t('content.kind')}</span>
                <select
                  aria-label={t('content.kind')}
                  value={kind}
                  disabled={working}
                  onChange={(event) => setKind(event.target.value)}
                >
                  <option value="map">{t('worlds.map')}</option>
                  <option value="datapack">{t('content.datapack')}</option>
                </select>
              </label>
              <label className="field">
                <span>{t('worlds.name')}</span>
                <input
                  type="text"
                  value={title}
                  maxLength={80}
                  disabled={working}
                  onChange={(event) => setTitle(event.target.value)}
                />
              </label>
              {kind === 'datapack' && (
                <>
                  <label className="field">
                    <span>{t('worlds.target')}</span>
                    <select
                      aria-label={t('worlds.target')}
                      value={world}
                      disabled={working}
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
                  {!worlds.length && <p>{t('worlds.none')}</p>}
                </>
              )}
              <p className="setting-hint">
                {t(kind === 'map' ? 'worlds.mapHint' : 'worlds.datapackHint')}
              </p>
            </>
          )}
          {working && <p role="status">{t('worlds.working')}</p>}
          {error && <ErrorNotice error={error} />}
          <div className="modal-actions">
            <button
              className="button secondary"
              disabled={working}
              onClick={() => void finish(true)}
            >
              {t('common.cancel')}
            </button>
            <button
              className="button primary"
              disabled={
                disabled ||
                working ||
                (plan ? !accepted : !title.trim() || (kind === 'datapack' && !world))
              }
              onClick={() => void (plan ? finish(false) : pick())}
            >
              {t(plan ? 'content.install' : 'worlds.chooseZip')}
            </button>
          </div>
        </Modal>
      )}
    </section>
  );
}
