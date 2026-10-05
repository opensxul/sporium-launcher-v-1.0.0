import { useCallback, useEffect, useRef, useState } from 'react';
import { Download, FolderInput, PackageOpen, Upload } from 'lucide-react';
import { Link } from 'react-router-dom';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import type { ExternalCandidate, PackJob, PackPreview } from '../bindings/core';
import { useFoundation } from '../app/context';
import { useLibrary } from '../library/context';
import { Modal } from '../components/Modal';
import { backend, normalizeError, type BackendError } from '../services/backend';
import { loaderNames } from '../library/view-model';

export function ImportPackButton() {
  const { t, desktop } = useFoundation();
  return (
    <button
      className="button secondary"
      disabled={!desktop}
      onClick={() => window.dispatchEvent(new Event('sporium-import'))}
    >
      <FolderInput size={17} />
      {t('pack.import')}
    </button>
  );
}
const activeJob = (job: PackJob | null) => !!job && ['downloading', 'applying'].includes(job.phase);
const size = (bytes: number) => `${(bytes / 1_000_000).toFixed(1)} MB`;

export function PackImports() {
  const { t, desktop } = useFoundation();
  const { reload } = useLibrary();
  const [open, setOpen] = useState(false);
  const [preview, setPreview] = useState<PackPreview | null>(null);
  const [candidates, setCandidates] = useState<ExternalCandidate[]>([]);
  const [name, setName] = useState('');
  const [optional, setOptional] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<BackendError | null>(null);
  const [job, setJob] = useState<PackJob | null>(null);
  const [registered, setRegistered] = useState(false);
  const previousPhase = useRef('');
  const guard = useRef(false);
  const opening = useRef<Promise<string[]> | null>(null);
  const openingHandled = useRef(false);
  const accept = useCallback((value: PackPreview) => {
    setPreview(value);
    setName(value.name);
    setOptional([]);
    setOpen(true);
    setError(null);
  }, []);
  const read = useCallback(
    async (action: () => Promise<PackPreview | null>) => {
      if (guard.current) return;
      guard.current = true;
      setBusy(true);
      setError(null);
      setOpen(true);
      try {
        const value = await action();
        if (value) accept(value);
      } catch (reason) {
        setError(normalizeError(reason));
      } finally {
        guard.current = false;
        setBusy(false);
      }
    },
    [accept],
  );
  useEffect(() => {
    const show = () => {
      if (!guard.current) {
        setOpen(true);
        setError(null);
      }
    };
    const receive = (event: Event) => accept((event as CustomEvent<PackPreview>).detail);
    window.addEventListener('sporium-import', show);
    window.addEventListener('sporium-pack-preview', receive);
    return () => {
      window.removeEventListener('sporium-import', show);
      window.removeEventListener('sporium-pack-preview', receive);
    };
  }, [accept]);
  useEffect(() => {
    if (!desktop) return;
    let alive = true;
    // Retain the consumed argument request across React StrictMode's effect replay.
    opening.current ??= backend.packOpening();
    void opening.current
      .then((paths) => {
        const path = paths[0];
        if (alive && path && !openingHandled.current) {
          openingHandled.current = true;
          void read(() => backend.packPreview(path));
        }
      })
      .catch((reason) => {
        if (alive) {
          setOpen(true);
          setError(normalizeError(reason));
        }
      });
    return () => {
      alive = false;
    };
  }, [desktop, read]);
  useEffect(() => {
    if (!desktop) return;
    let alive = true;
    const listening = getCurrentWebview().onDragDropEvent(({ payload }) => {
      if (!alive || payload.type !== 'drop' || guard.current || activeJob(job) || preview) return;
      const paths = payload.paths.filter((p) => /\.(mrpack|sporium)$/i.test(p));
      // Exactly one recognized pack; the local JAR and world drop zones retain their own behavior.
      const path = paths[0];
      if (path && paths.length === 1 && payload.paths.length === 1)
        void read(() => backend.packPreview(path));
    });
    void listening.catch((reason) => {
      if (alive) setError(normalizeError(reason));
    });
    return () => {
      alive = false;
      void listening.then((stop) => stop()).catch(() => {});
    };
  }, [desktop, read, job, preview]);
  useEffect(() => {
    if (!desktop) return;
    let alive = true;
    async function poll() {
      try {
        const next = await backend.packState();
        if (!alive) return;
        setJob(next);
        if (next && next.phase !== previousPhase.current) {
          previousPhase.current = next.phase;
          if (next.phase === 'completed') await reload();
        }
      } catch (reason) {
        if (alive) setError(normalizeError(reason));
      }
    }
    void poll();
    const timer = setInterval(() => void poll(), 1000);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, [desktop, reload]);
  function close() {
    if (busy) return;
    if (preview) void backend.packDismiss(preview.token).catch(() => {});
    setOpen(false);
    setPreview(null);
    setCandidates([]);
    setError(null);
  }
  async function scan() {
    setBusy(true);
    guard.current = true;
    setError(null);
    try {
      const value = await backend.pickExternal();
      if (value) setCandidates(value);
    } catch (reason) {
      setError(normalizeError(reason));
    } finally {
      setBusy(false);
      guard.current = false;
    }
  }
  async function install() {
    if (!preview) return;
    setBusy(true);
    setError(null);
    guard.current = true;
    try {
      setJob(await backend.packImport(preview.token, name, optional));
      setOpen(false);
      setPreview(null);
      setCandidates([]);
    } catch (reason) {
      setError(normalizeError(reason));
    } finally {
      setBusy(false);
      guard.current = false;
    }
  }
  return (
    <>
      {job && (
        <section className="surface pack-progress" aria-live="polite">
          <PackageOpen size={21} />
          <div>
            <strong>
              {t(`pack.phase.${job.phase}` as keyof typeof import('../i18n/messages').ru)} ·{' '}
              {job.name}
            </strong>
            <p>
              {job.completedFiles} / {job.totalFiles} · {size(job.downloadedBytes)} /{' '}
              {size(job.totalBytes)}
            </p>
            {job.error && <p role="alert">{t(`error.${job.error.code}`)}</p>}
          </div>
          {activeJob(job) && (
            <button
              className="button secondary"
              onClick={() =>
                void backend.packCancel().catch((reason) => setError(normalizeError(reason)))
              }
            >
              {t('common.cancel')}
            </button>
          )}
          {job.phase === 'completed' && job.instanceId && (
            <Link className="button secondary" to={`/instance/${job.instanceId}`}>
              {t('pack.open')}
            </Link>
          )}
        </section>
      )}
      {open && (
        <Modal title={t('pack.title')} busy={busy} onClose={close} className="pack-modal">
          {error && (
            <p role="alert" className="error-notice">
              {t(`error.${error.code}`)}
            </p>
          )}
          {busy && <p role="status">{t('pack.reading')}</p>}
          {preview ? (
            <>
              <p className="setting-hint">{t('pack.copy')}</p>
              <label className="field">
                <span>{t('pack.name')}</span>
                <input
                  maxLength={120}
                  value={name}
                  disabled={busy}
                  onChange={(e) => setName(e.target.value)}
                />
              </label>
              <p>{preview.description}</p>
              <dl className="pack-summary">
                <div>
                  <dt>{t('pack.version')}</dt>
                  <dd>{preview.version}</dd>
                </div>
                <div>
                  <dt>Minecraft</dt>
                  <dd>{preview.minecraft}</dd>
                </div>
                <div>
                  <dt>{t('instance.loader')}</dt>
                  <dd>
                    {loaderNames[preview.loader]} {preview.loaderVersion}
                  </dd>
                </div>
                <div>
                  <dt>{t('pack.required')}</dt>
                  <dd>{preview.requiredFiles}</dd>
                </div>
                <div>
                  <dt>{t('pack.size')}</dt>
                  <dd>{size(preview.downloadBytes)}</dd>
                </div>
                <div>
                  <dt>{t('pack.embedded')}</dt>
                  <dd>{size(preview.embeddedBytes)}</dd>
                </div>
              </dl>
              {preview.optionalFiles.length > 0 && (
                <section className="pack-optionals">
                  <h3>{t('pack.optional')}</h3>
                  <p className="setting-hint">{t('pack.optionalHint')}</p>
                  {preview.optionalFiles.map((file) => (
                    <label key={file} className="checkbox-label">
                      <input
                        type="checkbox"
                        disabled={busy}
                        checked={optional.includes(file)}
                        onChange={(e) =>
                          setOptional((items) =>
                            e.target.checked ? [...items, file] : items.filter((v) => v !== file),
                          )
                        }
                      />
                      <span>{file}</span>
                    </label>
                  ))}
                </section>
              )}
              <p className="setting-hint">{t('pack.prepare')}</p>
              {preview.warnings.length > 0 && (
                <details>
                  <summary>{t('pack.details')}</summary>
                  <p>{t('pack.settingsSkip')}</p>
                  {preview.warnings.some((w) => w.startsWith('server_')) && (
                    <p>{t('pack.serverSkip')}</p>
                  )}
                  <ul>
                    {preview.warnings.map((warning) => (
                      <li key={warning}>
                        {warning.startsWith('source_item_skipped:')
                          ? `${t('pack.skipped')}: ${warning.slice(20)}`
                          : warning}
                      </li>
                    ))}
                  </ul>
                </details>
              )}
              <div className="modal-actions">
                <button className="button secondary" disabled={busy} onClick={close}>
                  {t('common.cancel')}
                </button>
                <button
                  className="button primary"
                  disabled={busy || !name.trim() || activeJob(job)}
                  onClick={() => void install()}
                >
                  <Download size={16} />
                  {t('pack.confirm')}
                </button>
              </div>
            </>
          ) : (
            <>
              <p>{t('pack.hint')}</p>
              <div className="modal-actions">
                <button
                  className="button primary"
                  disabled={busy || activeJob(job)}
                  onClick={() => void read(backend.pickPack)}
                >
                  <Upload size={17} />
                  {t('pack.file')}
                </button>
                <button
                  className="button secondary"
                  disabled={busy || activeJob(job)}
                  onClick={() => void scan()}
                >
                  <FolderInput size={17} />
                  {t('pack.external')}
                </button>
              </div>
              <p className="setting-hint">{t('pack.externalHint')}</p>
              <p className="setting-hint">{t('pack.allowed')}</p>
              {candidates.map((candidate) => (
                <button
                  key={candidate.key}
                  className="button secondary pack-candidate"
                  disabled={busy}
                  onClick={() => void read(() => backend.externalPreview(candidate.key))}
                >
                  <strong>{candidate.name}</strong>
                  <span>
                    {candidate.source} · {candidate.minecraft} · {loaderNames[candidate.loader]}
                  </span>
                </button>
              ))}
              <button
                className="button secondary"
                disabled={busy}
                onClick={() => {
                  setBusy(true);
                  void backend
                    .registerPackFormats()
                    .then(() => setRegistered(true))
                    .catch((reason) => setError(normalizeError(reason)))
                    .finally(() => setBusy(false));
                }}
              >
                {t('pack.formats')}
              </button>
              {registered && <p role="status">{t('pack.formatsDone')}</p>}
            </>
          )}
        </Modal>
      )}
    </>
  );
}
