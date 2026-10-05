import { useState } from 'react';
import { Upload } from 'lucide-react';
import type { PackExport as Report } from '../bindings/core';
import { useFoundation } from '../app/context';
import { Modal } from '../components/Modal';
import { backend, normalizeError, type BackendError } from '../services/backend';

export function ExportPackButton({ id, disabled }: { id: string; disabled: boolean }) {
  const { t, desktop } = useFoundation();
  const [open, setOpen] = useState(false);
  const [worlds, setWorlds] = useState(false);
  const [local, setLocal] = useState(false);
  const [busy, setBusy] = useState(false);
  const [report, setReport] = useState<Report | null>(null);
  const [error, setError] = useState<BackendError | null>(null);
  async function save() {
    setBusy(true);
    setError(null);
    try {
      const value = await backend.pickPackExport({
        id,
        includeWorlds: worlds,
        includeLocal: local,
      });
      if (value) setReport(value);
    } catch (reason) {
      setError(normalizeError(reason));
    } finally {
      setBusy(false);
    }
  }
  return (
    <>
      <button
        className="button secondary"
        disabled={disabled || !desktop}
        onClick={() => {
          setOpen(true);
          setReport(null);
          setError(null);
        }}
      >
        <Upload size={16} />
        {t('pack.export')}
      </button>
      {open && (
        <Modal title={t('pack.exportTitle')} busy={busy} onClose={() => setOpen(false)}>
          <p>{t('pack.exportHint')}</p>
          <p className="setting-hint">{t('pack.pickNew')}</p>
          <label className="checkbox-label">
            <input
              type="checkbox"
              disabled={busy}
              checked={worlds}
              onChange={(e) => setWorlds(e.target.checked)}
            />
            {t('pack.worlds')}
          </label>
          <label className="checkbox-label">
            <input
              type="checkbox"
              disabled={busy}
              checked={local}
              onChange={(e) => setLocal(e.target.checked)}
            />
            {t('pack.local')}
          </label>
          {error && (
            <p role="alert" className="error-notice">
              {t(`error.${error.code}`)}
            </p>
          )}
          {report && (
            <section role="status">
              <h3>
                {t('pack.saved')}: {report.fileName}
              </h3>
              <p>
                {t('pack.refs')}: {report.referencedFiles} · {t('pack.archiveFiles')}:{' '}
                {report.embeddedFiles}
              </p>
              {report.omitted.length > 0 && (
                <details>
                  <summary>
                    {t('pack.skipped')} ({report.omitted.length})
                  </summary>
                  <ul>
                    {report.omitted.map((name) => (
                      <li key={name}>{name}</li>
                    ))}
                  </ul>
                </details>
              )}
            </section>
          )}
          <div className="modal-actions">
            <button
              className="button primary"
              disabled={busy || disabled}
              onClick={() => void save()}
            >
              {t('pack.save')}
            </button>
          </div>
        </Modal>
      )}
    </>
  );
}
