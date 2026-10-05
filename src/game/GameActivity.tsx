import { Download, X } from 'lucide-react';
import { useFoundation } from '../app/context';
import { useLibrary } from '../library/context';
import { ErrorNotice } from '../components/ui';
import { BackendError } from '../services/backend';
import { useGame, jobActive } from './context';

export function GameAlerts() {
  const { t } = useFoundation();
  const { error, closeWarning, dismiss } = useGame();
  if (!error && !closeWarning) return null;
  return (
    <div className="game-alerts">
      {error && <ErrorNotice error={error} />}
      {closeWarning && <p role="alert">{t('game.closeWarning')}</p>}
      <button className="icon-button" aria-label={t('common.close')} onClick={dismiss}>
        <X size={18} />
      </button>
    </div>
  );
}
export function GameActivity({ compact = false }: { compact?: boolean }) {
  const { t } = useFoundation();
  const { snapshot } = useLibrary();
  const { state, cancel, pause, start, starting } = useGame();
  const job = state.job;
  if (!job) return compact ? null : <p className="setting-hint">{t('game.noJobs')}</p>;
  const instance = snapshot.instances.find((item) => item.id === job.instanceId);
  return (
    <section
      className={`game-activity ${compact ? 'compact' : ''}`}
      aria-label={t('game.jobTitle')}
    >
      <div className="activity-heading">
        <Download size={18} />
        <div>
          <strong>{instance?.name ?? job.instanceId}</strong>
          <p role="status">{t(job.paused ? 'download.paused' : `job.${job.phase}`)}</p>
        </div>
        {jobActive(state) && (
          <button className="button secondary" onClick={() => void cancel()}>
            {t('game.cancel')}
          </button>
        )}
        {job.phase === 'downloading' && (
          <button
            className="button secondary"
            disabled={starting}
            onClick={() => void pause(!job.paused)}
          >
            {t(job.paused ? 'download.resume' : 'download.pause')}
          </button>
        )}
        {['failed', 'cancelled', 'interrupted'].includes(job.phase) && instance && (
          <button
            className="button secondary"
            disabled={starting}
            onClick={() => void start({ id: job.instanceId, action: job.action })}
          >
            {t(job.action === 'local' ? 'download.retryLaunch' : 'download.retry')}
          </button>
        )}
      </div>
      {jobActive(state) && (
        <progress
          max={job.totalBytes || undefined}
          value={
            job.phase === 'downloading' && job.totalBytes
              ? job.verifiedBytes + job.activeFiles.reduce((n, f) => n + f.received, 0)
              : undefined
          }
          aria-label={t(`job.${job.phase}`)}
        />
      )}
      <div className="activity-numbers">
        {job.totalFiles > 0 && (
          <span>
            {t('game.files')}: {job.completedFiles} / {job.totalFiles}
          </span>
        )}
        <span>
          {t('game.transferred')}: {(job.downloadedBytes / 1048576).toFixed(1)} MB
        </span>
        {jobActive(state) && <span>{(job.bytesPerSecond / 1048576).toFixed(1)} MB/s</span>}
        {job.etaSeconds !== null && (
          <span>
            {t('download.eta')}: {Math.ceil(job.etaSeconds / 60)} {t('download.minutes')}
          </span>
        )}
        <span>
          {t('download.cached')}: {job.cachedFiles}
        </span>
        <span>
          {t('download.repaired')}: {job.repairedFiles}
        </span>
        {job.retries > 0 && (
          <span>
            {t('download.retries')}: {job.retries}
          </span>
        )}
      </div>
      {!compact && (
        <>
          {job.phase === 'interrupted' && (
            <p className="setting-hint">{t('download.interruptedHint')}</p>
          )}
          {job.phase === 'downloading' && (
            <p className="setting-hint">
              {t('download.waiting')}:{' '}
              {Math.max(0, job.totalFiles - job.completedFiles - job.activeFiles.length)}
            </p>
          )}
          <details>
            <summary>{t('ui.details')}</summary>
            <ul className="transfer-files">
              {job.activeFiles.map((file) => (
                <li key={file.index}>
                  <span title={file.name}>{file.name}</span>
                  <small>
                    {(file.received / 1048576).toFixed(1)} / {(file.total / 1048576).toFixed(1)} MB
                  </small>
                </li>
              ))}
            </ul>
            <p className="setting-hint">{t('download.repairScope')}</p>
          </details>
        </>
      )}
      {job.error && <ErrorNotice error={new BackendError(job.error.code, job.error.retryable)} />}
    </section>
  );
}
