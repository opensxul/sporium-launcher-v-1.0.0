import { Link } from 'react-router-dom';
import { useFoundation } from '../app/context';
import { useLibrary } from '../library/context';
import { ErrorNotice } from '../components/ui';
import { BackendError } from '../services/backend';
import { useContent, contentActive } from './context';
import type { MessageKey } from '../i18n/messages';
export function ContentActivity() {
  const { job, error, cancel } = useContent();
  const { t } = useFoundation();
  const { snapshot } = useLibrary();
  if (!job) return error ? <ErrorNotice error={error} /> : null;
  return (
    <section className="game-activity content-activity" aria-label={t('content.installed')}>
      <div className="activity-heading">
        <div>
          <Link to={`/instance/${job.instanceId}`}>
            {snapshot.instances.find((i) => i.id === job.instanceId)?.name ?? job.instanceId}
          </Link>
          <p role="status">{t(`content.phase.${job.phase}` as MessageKey)}</p>
        </div>
        {['downloading', 'preparing_game'].includes(job.phase) && (
          <button className="button secondary" onClick={() => void cancel()}>
            {t('game.cancel')}
          </button>
        )}
      </div>
      {contentActive(job) && (
        <progress
          max={job.totalFiles || 1}
          value={job.completedFiles}
          aria-label={t('content.installed')}
        />
      )}
      <div className="activity-numbers">
        <span>
          {job.completedFiles} / {job.totalFiles}
        </span>
        <span>{(job.downloadedBytes / 1048576).toFixed(1)} MB</span>
      </div>
      {job.currentFile && (
        <details>
          <summary>{t('content.fileDetails')}</summary>
          <p className="content-filename">{job.currentFile}</p>
        </details>
      )}
      {job.error && <ErrorNotice error={new BackendError(job.error.code, job.error.retryable)} />}
      {error && <ErrorNotice error={error} />}
    </section>
  );
}
