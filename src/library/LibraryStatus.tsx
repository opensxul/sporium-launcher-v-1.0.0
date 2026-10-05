import { LoaderCircle, X } from 'lucide-react';
import { useLibrary } from './context';
import { useFoundation } from '../app/context';
import { ErrorNotice } from '../components/ui';
import { OpenFolderButton } from './OpenFolderButton';

export function LibraryStatus() {
  const { loading, error, reload } = useLibrary();
  const { t } = useFoundation();
  if (loading)
    return (
      <p className="library-loading" role="status">
        <LoaderCircle size={17} className="spin" />
        {t('library.loading')}
      </p>
    );
  return (
    error && (
      <ErrorNotice
        error={error}
        action={
          <button className="button secondary" onClick={() => void reload()}>
            {t('library.reload')}
          </button>
        }
      />
    )
  );
}

export function BackupNotice() {
  const { backup, dismissBackup } = useLibrary();
  const { t } = useFoundation();
  if (!backup) return null;
  return (
    <div className="backup-notice" role="status">
      <div>
        <strong>{t('instance.preserved')}</strong>
        <code>{backup.directory}</code>
      </div>
      <OpenFolderButton target="backup" id={backup.id} label={t('common.openFolder')} />
      <button className="icon-button" onClick={dismissBackup} aria-label={t('common.close')}>
        <X size={17} />
      </button>
    </div>
  );
}
