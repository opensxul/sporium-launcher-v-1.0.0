import { useEffect, useState } from 'react';
import type { CacheStats, Settings } from '../bindings/core';
import { useFoundation } from '../app/context';
import { backend, normalizeError, type BackendError } from '../services/backend';
import { ErrorNotice } from '../components/ui';
import { Modal } from '../components/Modal';
import { useGame } from './context';

export function DownloadSettings({
  draft,
  update,
  disabled,
}: {
  draft: Settings;
  update: (patch: Partial<Settings>) => void;
  disabled: boolean;
}) {
  const { t } = useFoundation();
  return (
    <div className="java-settings form-fields">
      <label>
        {t('download.concurrency')}
        <select
          aria-label={t('download.concurrency')}
          value={draft.downloadConcurrency}
          disabled={disabled}
          onChange={(e) => update({ downloadConcurrency: Number(e.target.value) })}
        >
          {[1, 2, 3, 4, 6, 8, 12].map((n) => (
            <option key={n} value={n}>
              {n}
            </option>
          ))}
        </select>
      </label>
      <p className="form-hint">{t('download.settingsHint')}</p>
    </div>
  );
}
export function CacheSettings({
  draft,
  update,
  disabled,
}: {
  draft: Settings;
  update: (patch: Partial<Settings>) => void;
  disabled: boolean;
}) {
  const { desktop, t } = useFoundation();
  const game = useGame();
  const [stats, setStats] = useState<CacheStats | null>(null);
  const [error, setError] = useState<BackendError | null>(null);
  const [busy, setBusy] = useState(false);
  const [confirm, setConfirm] = useState(false);
  useEffect(() => {
    let active = true;
    if (desktop)
      void backend
        .downloadCache()
        .then((value) => {
          if (active) setStats(value);
        })
        .catch((reason) => {
          if (active) setError(normalizeError(reason));
        });
    return () => {
      active = false;
    };
  }, [desktop]);
  async function run(cleanup: boolean) {
    setBusy(true);
    setError(null);
    try {
      setStats(await backend.downloadCache(cleanup));
      setConfirm(false);
    } catch (reason) {
      setError(normalizeError(reason));
    } finally {
      setBusy(false);
    }
  }
  return (
    <div className="java-settings">
      <h3>{t('cache.title')}</h3>
      <p className="setting-hint">{t('cache.scope')}</p>
      {stats && (
        <p>
          {t('cache.size')}: {(stats.disposableBytes / 1048576).toFixed(1)} MB ·{' '}
          {t('cache.partial')}: {(stats.partialBytes / 1048576).toFixed(1)} MB
        </p>
      )}
      <div className="instance-actions">
        <button
          className="button secondary"
          disabled={!desktop || busy}
          onClick={() => void run(false)}
        >
          {t('cache.refresh')}
        </button>
        <button
          className="button secondary"
          disabled={!desktop || busy || game.busy || !stats?.files}
          onClick={() => setConfirm(true)}
        >
          {t('cache.clear')}
        </button>
      </div>
      <div className="form-fields">
        <label>
          {t('cache.limit')}
          <select
            aria-label={t('cache.limit')}
            value={draft.cacheLimitMb}
            disabled={disabled}
            onChange={(e) => update({ cacheLimitMb: Number(e.target.value) })}
          >
            {[0, 64, 256, 512, 1024, 4096].map((n) => (
              <option value={n} key={n}>
                {n ? `${n} MB` : t('cache.unlimited')}
              </option>
            ))}
          </select>
        </label>
        <p className="form-hint">{t('cache.limitHint')}</p>
      </div>
      {error && <ErrorNotice error={error} />}
      {confirm && (
        <Modal title={t('cache.clear')} busy={busy} onClose={() => setConfirm(false)}>
          <p>{t('cache.confirm')}</p>
          <div className="modal-actions">
            <button className="button secondary" disabled={busy} onClick={() => setConfirm(false)}>
              {t('common.cancel')}
            </button>
            <button className="button primary" disabled={busy} onClick={() => void run(true)}>
              {t('cache.clear')}
            </button>
          </div>
        </Modal>
      )}
    </div>
  );
}
