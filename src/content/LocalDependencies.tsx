import { useEffect, useState } from 'react';
import type { ContentSelection, LocalDependencyPlan } from '../bindings/core';
import { useFoundation } from '../app/context';
import { backend, normalizeError } from '../services/backend';
import type { BackendError } from '../services/backend';
import { Modal } from '../components/Modal';
import { ErrorNotice } from '../components/ui';
import { useContent } from './context';
import { ProjectIcon } from './ProjectIcon';
export function LocalDependencies({
  id,
  files,
  onClose,
}: {
  id: string;
  files: ContentSelection[];
  onClose: () => void;
}) {
  const { t } = useFoundation();
  const content = useContent();
  const [plan, setPlan] = useState<LocalDependencyPlan | null>(null);
  const [error, setError] = useState<BackendError | null>(null);
  useEffect(() => {
    let active = true;
    void backend
      .contentDependencyPlan(id, files)
      .then((value) => {
        if (active) setPlan(value);
      })
      .catch((reason) => {
        if (active) setError(normalizeError(reason));
      });
    return () => {
      active = false;
    };
  }, [id, files]);
  return (
    <Modal title={t('dependencies.title')} onClose={onClose} busy={content.busy}>
      <p className="setting-hint">{t('dependencies.hint')}</p>
      {!plan && !error && <p role="status">{t('content.loading')}</p>}
      {plan && (
        <>
          <p>
            {t('dependencies.matched')}:{' '}
            {plan.parents
              .map((parent) => `${parent.title} ${parent.version.version_number}`)
              .join(', ')}
          </p>
          <ul className="local-plan-files">
            {plan.plan.files.map((file) => (
              <li key={`${file.directory}/${file.file.filename}`}>
                <ProjectIcon title={file.title} kind={file.kind} url={file.iconUrl} />
                <div>
                  <strong>{file.title}</strong> · {file.version.version_number}
                  <small>Modrinth</small>
                  <details>
                    <summary>{t('content.fileDetails')}</summary>
                    {file.directory}/{file.file.filename}
                  </details>
                </div>
              </li>
            ))}
          </ul>
          {!plan.plan.files.length && <p>{t('dependencies.none')}</p>}
          <p>
            {t('content.total')}: {(plan.plan.totalBytes / 1048576).toFixed(1)} MB
          </p>
        </>
      )}
      {error && <ErrorNotice error={error} />}
      {content.error && <ErrorNotice error={content.error} />}
      <div className="modal-actions">
        <button className="button secondary" disabled={content.busy} onClick={onClose}>
          {t('common.cancel')}
        </button>
        <button
          className="button primary"
          disabled={!plan?.plan.files.length || content.busy}
          onClick={() => {
            if (plan)
              void content.install(plan.plan.token).then((ok) => {
                if (ok) onClose();
              });
          }}
        >
          {t('dependencies.installAll')}
        </button>
      </div>
    </Modal>
  );
}
