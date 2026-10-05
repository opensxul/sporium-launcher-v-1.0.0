import { useEffect, useState } from 'react';
import type { ContentRestorePoint } from '../bindings/core';
import { useFoundation } from '../app/context';
import { useLibrary } from '../library/context';
import { Modal } from '../components/Modal';
import { ErrorNotice } from '../components/ui';
import { backend, normalizeError, type BackendError } from '../services/backend';

export function ContentRestore({
  id,
  onClose,
  onRestored,
}: {
  id: string;
  onClose: () => void;
  onRestored: () => void;
}) {
  const { t, data } = useFoundation();
  const { reload } = useLibrary();
  const [points, setPoints] = useState<ContentRestorePoint[]>([]);
  const [selected, setSelected] = useState<ContentRestorePoint | null>(null);
  const [working, setWorking] = useState(false);
  const [settings, setSettings] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<BackendError | null>(null);
  useEffect(() => {
    let active = true;
    void backend
      .contentRestorePoints(id)
      .then((value) => {
        if (active) setPoints(value);
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
  }, [id]);
  async function restore() {
    if (!selected) return;
    setWorking(true);
    setError(null);
    try {
      await backend.restoreContent(id, selected.id, settings);
      await reload();
      window.dispatchEvent(new Event('sporium-project-changed'));
      onRestored();
      onClose();
    } catch (reason) {
      setError(normalizeError(reason));
    } finally {
      setWorking(false);
    }
  }
  return (
    <Modal title={t('restore.title')} onClose={onClose} busy={working}>
      <p className="setting-hint">{t('restore.hint')}</p>
      {error && <ErrorNotice error={error} />}
      {loading ? (
        <p role="status">{t('restore.loading')}</p>
      ) : !points.length ? (
        <p>{t('restore.empty')}</p>
      ) : (
        <ul className="update-plan">
          {points.map((point) => (
            <li key={point.id}>
              <label>
                <input
                  type="radio"
                  name="restore-point"
                  disabled={working || !point.available}
                  checked={selected?.id === point.id}
                  onChange={() => {
                    setSelected(point);
                    setSettings(false);
                  }}
                />{' '}
                {point.timestamp
                  ? new Date(point.timestamp).toLocaleString(data.settings.values.locale)
                  : point.title}{' '}
                · {point.files} {t('restore.files')}
                {!point.available && ` · ${t('restore.unavailable')}`}
              </label>
            </li>
          ))}
        </ul>
      )}
      {selected && <p className="setting-hint">{t('restore.confirm')}</p>}
      {selected?.settingsAvailable && (
        <label className="setting-hint">
          <input
            type="checkbox"
            checked={settings}
            disabled={working}
            onChange={(event) => setSettings(event.target.checked)}
          />{' '}
          {t('restore.settings')}
          <p>{t('restore.settingsHint')}</p>
        </label>
      )}
      <div className="modal-actions">
        <button className="button secondary" disabled={working} onClick={onClose}>
          {t('common.cancel')}
        </button>
        <button
          className="button"
          disabled={!selected || working || loading}
          onClick={() => void restore()}
        >
          {t(working ? 'restore.working' : 'restore.apply')}
        </button>
      </div>
    </Modal>
  );
}
