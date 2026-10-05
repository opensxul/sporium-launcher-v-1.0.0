import { useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';
import type { AppUpdate } from '../bindings/core';
import { useFoundation } from './context';
import { Modal } from '../components/Modal';
import { ErrorNotice } from '../components/ui';
import { normalizeError, type BackendError } from '../services/backend';
export function AppUpdates() {
  const { desktop, data } = useFoundation();
  const ru = data.settings.values.locale === 'ru-RU';
  const [update, setUpdate] = useState<AppUpdate | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<BackendError | null>(null);
  const [progress, setProgress] = useState({ received: 0, total: null as number | null });
  useEffect(() => {
    if (!desktop) return;
    let active = true;
    const timer = setTimeout(() => {
      void invoke<AppUpdate | null>('app_update_check')
        .then((value) => {
          if (active) setUpdate(value);
        })
        .catch(() => {});
    }, 1500);
    const listener = listen<{ received: number; total: number | null }>(
      'app-update-progress',
      (event) => {
        if (active) setProgress(event.payload);
      },
    );
    return () => {
      active = false;
      clearTimeout(timer);
      void listener.then((unlisten) => unlisten()).catch(() => {});
    };
  }, [desktop]);
  if (!update) return null;
  return (
    <Modal
      title={ru ? 'Доступно обновление Sporium' : 'Sporium update available'}
      busy={busy}
      onClose={() => setUpdate(null)}
    >
      <h3>Sporium {update.version}</h3>
      {update.size && <p>{(update.size / 1048576).toFixed(1)} MB</p>}
      <p className="update-notes">{update.notes}</p>
      <p className="setting-hint">
        {ru
          ? 'Перед обновлением закрой игру и заверши загрузки. После проверки подписи откроется установщик. Сборки и настройки сохранятся.'
          : 'Close games and finish downloads before updating. The installer opens after signature verification. Instances and settings are preserved.'}
      </p>
      {busy && (
        <>
          <progress
            max={progress.total ?? undefined}
            value={progress.total ? progress.received : undefined}
          />
          <p role="status">
            {ru ? 'Загрузка и проверка обновления…' : 'Downloading and verifying update…'}{' '}
            {(progress.received / 1048576).toFixed(1)} MB
          </p>
        </>
      )}
      {error && <ErrorNotice error={error} />}
      <div className="modal-actions">
        <button className="button secondary" disabled={busy} onClick={() => setUpdate(null)}>
          {ru ? 'Позже' : 'Later'}
        </button>
        <button
          className="button primary"
          disabled={busy}
          onClick={async () => {
            setBusy(true);
            setError(null);
            try {
              await invoke('app_update_install');
            } catch (reason) {
              setError(normalizeError(reason));
            } finally {
              setBusy(false);
            }
          }}
        >
          {ru ? 'Обновить' : 'Update'}
        </button>
      </div>
    </Modal>
  );
}
