import type { UntrackedContent } from '../bindings/core';
import type { MessageKey } from '../i18n/messages';
import { useFoundation } from '../app/context';
import { ProjectIcon } from './ProjectIcon';
import { useEffect, useRef, useState } from 'react';
import type { ContentAdoptionPlan } from '../bindings/core';
import { backend, normalizeError } from '../services/backend';
import type { BackendError } from '../services/backend';
import { Modal } from '../components/Modal';
import { ErrorNotice } from '../components/ui';
import { useLibrary } from '../library/context';
import { DependencyReport } from './ContentDiagnostics';

export function UntrackedFiles({
  files,
  query,
  kind,
  id,
  disabled,
  revision,
  onAdopted,
}: {
  files: UntrackedContent[];
  query: string;
  kind: string;
  id: string;
  disabled: boolean;
  revision: number;
  onAdopted: () => void;
}) {
  const { t } = useFoundation();
  const { snapshot } = useLibrary();
  const instance = snapshot.instances.find((item) => item.id === id);
  const [plan, setPlan] = useState<ContentAdoptionPlan | null>(null);
  const [file, setFile] = useState<UntrackedContent | null>(null);
  const [working, setWorking] = useState(false);
  const [accepted, setAccepted] = useState(false);
  const [error, setError] = useState<BackendError | null>(null);
  const mounted = useRef(false);
  const token = useRef<string | null>(null);
  const inflight = useRef(false);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      if (token.current) void backend.finishAdoption(token.current, false, true).catch(() => {});
    };
  }, []);
  async function begin(selected: UntrackedContent, recognize = false) {
    if (disabled || inflight.current) return;
    inflight.current = true;
    setWorking(true);
    setError(null);
    try {
      const value = await backend.adoptionPlan({
        instanceId: id,
        directory: selected.directory,
        filename: selected.filename,
        recognize,
      });
      if (!mounted.current) {
        await backend.finishAdoption(value.plan.token, false, true);
        return;
      }
      token.current = value.plan.token;
      setFile(selected);
      setPlan(value);
      setAccepted(false);
    } catch (reason) {
      if (mounted.current) setError(normalizeError(reason));
    } finally {
      inflight.current = false;
      if (mounted.current) setWorking(false);
    }
  }
  async function finish(cancel: boolean) {
    if (!plan || inflight.current) return;
    inflight.current = true;
    setWorking(true);
    setError(null);
    try {
      await backend.finishAdoption(plan.plan.token, accepted, cancel);
      token.current = null;
      if (mounted.current) {
        setPlan(null);
        setFile(null);
        if (!cancel) onAdopted();
      }
    } catch (reason) {
      if (mounted.current) setError(normalizeError(reason));
    } finally {
      inflight.current = false;
      if (mounted.current) setWorking(false);
    }
  }
  const visible = files.filter(
    (file) =>
      (!kind || file.kind === kind) &&
      `${file.title} ${file.filename}`
        .toLocaleLowerCase()
        .includes(query.trim().toLocaleLowerCase()),
  );
  if (!visible.length) return null;
  return (
    <div className="untracked-content">
      <h3>
        {t('local.untracked')} · {visible.length}
      </h3>
      <p className="setting-hint">{t('local.untrackedHint')}</p>
      <ul className="untracked-files">
        {visible.map((file) => (
          <li key={`${file.directory}/${file.filename}`}>
            <ProjectIcon
              title={file.title}
              kind={file.kind}
              instanceId={file.manageable || file.status === 'launcher' ? id : undefined}
              directory={file.directory}
              filename={file.filename}
              revision={revision}
            />
            <div>
              <strong>{file.title}</strong>
              {file.version && <> · {file.version}</>}
              <small>
                {t(file.status === 'launcher' ? 'local.launcherSource' : 'local.untrackedSource')}
                {file.disabled && ` · ${t('content.status.disabled')}`}
              </small>
              <details>
                <summary>{t('content.fileDetails')}</summary>
                <small>
                  {file.directory}/{file.filename}
                </small>
                {file.metadata && (
                  <>
                    <small>
                      {file.metadata.loader || t('local.unknown')} ·{' '}
                      {file.metadata.modIds.join(', ')}
                    </small>
                    {!!file.metadata.required.length && (
                      <small>
                        {t('local.required')}: {file.metadata.required.join(', ')}
                      </small>
                    )}
                    {file.metadata.warnings.map((code) => (
                      <small key={code}>{t(`local.warning.${code}` as MessageKey)}</small>
                    ))}
                  </>
                )}
              </details>
            </div>
            <span className="setting-hint">{t(`local.status.${file.status}` as MessageKey)}</span>
            {file.manageable && (
              <button
                className="button secondary"
                disabled={disabled || working}
                onClick={() => void begin(file)}
              >
                {t('local.adopt')}
              </button>
            )}
            {!file.manageable && file.status === 'unknown' && (
              <small>{t('local.folderReadOnly')}</small>
            )}
          </li>
        ))}
      </ul>
      {error && !plan && <ErrorNotice error={error} />}
      {plan && file && (
        <Modal title={t('local.adopt')} busy={working} onClose={() => void finish(true)}>
          <p>{t('local.adoptHint')}</p>
          <p>
            {t('local.destination')} <strong>{instance?.name}</strong>
            <br />
            {instance?.minecraftVersion} · {instance?.loader}
          </p>
          {plan.plan.files.map((record) => (
            <div key={record.projectId} className="local-plan-files">
              <strong>
                {record.title} · {record.version.version_number}
              </strong>
              <p>{record.provider === 'modrinth' ? 'Modrinth' : t('local.source')}</p>
              <details>
                <summary>{t('content.fileDetails')}</summary>
                <p>
                  {record.directory}/{record.file.filename}
                </p>
                <>
                  <p>
                    {plan.metadata.loader || t('local.unknown')} · {plan.metadata.modIds.join(', ')}
                  </p>
                  {!!plan.metadata.required.length && (
                    <p>
                      {t('local.required')}: {plan.metadata.required.join(', ')}
                    </p>
                  )}
                </>
              </details>
            </div>
          ))}
          {plan.warnings.length > 0 && (
            <div className="content-warning">
              <ul>
                {plan.warnings.map((code) => (
                  <li key={code}>{t(`local.warning.${code}` as MessageKey)}</li>
                ))}
              </ul>
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
          )}
          {error && <ErrorNotice error={error} />}
          {plan.plan.files.some((record) => record.kind === 'mod') && (
            <DependencyReport
              report={plan.diagnostics}
              files={plan.plan.files.map((file) => ({
                directory: file.directory,
                filename: file.file.filename,
              }))}
            />
          )}
          {working && <p role="status">{t('local.inspecting')}</p>}
          <div className="modal-actions">
            <button
              className="button secondary"
              disabled={working}
              onClick={() => void finish(true)}
            >
              {t('common.cancel')}
            </button>
            {plan.plan.files[0]?.provider === 'local' && (
              <button
                className="button secondary"
                disabled={working || disabled}
                onClick={() => void begin(file, true)}
              >
                {t('local.recognize')}
              </button>
            )}
            <button
              className="button primary"
              disabled={working || disabled || (plan.warnings.length > 0 && !accepted)}
              onClick={() => void finish(false)}
            >
              {t('local.adoptConfirm')}
            </button>
          </div>
        </Modal>
      )}
    </div>
  );
}
