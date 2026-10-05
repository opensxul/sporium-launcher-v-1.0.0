import { useEffect, useState } from 'react';
import type { ModDiagnostics, ModDiagnostic } from '../bindings/core';
import { backend, normalizeError } from '../services/backend';
import type { BackendError } from '../services/backend';
import { useFoundation } from '../app/context';
import { Modal } from '../components/Modal';
import { ErrorNotice } from '../components/ui';
import type { MessageKey } from '../i18n/messages';

const finding = (mod: ModDiagnostic) =>
  mod.status !== 'readable' ||
  mod.warnings.length > 0 ||
  mod.checks.some((check) => check.severity !== 'info');
export function DependencyReport({
  report,
  files,
  onSearch,
}: {
  report: ModDiagnostics;
  files?: { directory: string; filename: string }[];
  onSearch?: (id: string) => void;
}) {
  const { t } = useFoundation();
  const [issuesOnly, setIssuesOnly] = useState(true);
  const rows = report.mods.filter(
    (mod) =>
      !files ||
      files.some(
        (file) =>
          file.directory === mod.directory &&
          file.filename.toLocaleLowerCase() === mod.filename.toLocaleLowerCase(),
      ),
  );
  const visible = rows.filter((mod) => !issuesOnly || finding(mod));
  return (
    <section className="dependency-report">
      <h3>{t('diagnostics.title')}</h3>
      <p className="setting-hint">{t('diagnostics.hint')}</p>
      {!report.complete && <p className="content-warning">{t('diagnostics.incomplete')}</p>}
      {!files && (
        <p>
          {t('diagnostics.errors')}: {report.errors} · {t('diagnostics.warnings')}:{' '}
          {report.warnings}
        </p>
      )}
      <label className="diagnostic-filter">
        <input
          type="checkbox"
          checked={issuesOnly}
          onChange={(event) => setIssuesOnly(event.target.checked)}
        />{' '}
        {t('diagnostics.onlyIssues')}
      </label>
      {visible.length === 0 && <p>{t('diagnostics.noFindings')}</p>}
      <ul className="diagnostic-mods">
        {visible.map((mod) => (
          <li key={`${mod.directory}/${mod.filename}`}>
            <details open={files !== undefined || finding(mod)}>
              <summary>
                <strong>{mod.title}</strong>{' '}
                {!mod.enabled && <span>· {t('content.status.disabled')}</span>}
              </summary>
              {!mod.enabled && <p className="setting-hint">{t('diagnostics.disabledOwner')}</p>}
              {mod.status !== 'readable' && (
                <p className="content-warning">
                  {t(`diagnostics.file.${mod.status}` as MessageKey)}
                </p>
              )}
              {mod.warnings.map((warning) => (
                <p className="setting-hint" key={warning}>
                  {t(`local.warning.${warning}` as MessageKey)}
                </p>
              ))}
              <ul className="diagnostic-checks">
                {mod.checks
                  .filter((check) => !issuesOnly || check.severity !== 'info')
                  .map((check, index) => (
                    <li
                      key={`${check.dependency.ownerId}/${check.dependency.id}/${index}`}
                      data-status={check.status}
                      className={`diagnostic-${check.severity}`}
                    >
                      <div>
                        <strong>{check.dependency.id}</strong>{' '}
                        <small>
                          · {t(`diagnostics.relation.${check.dependency.relation}` as MessageKey)}
                        </small>
                      </div>
                      <span>{t(`diagnostics.status.${check.status}` as MessageKey)}</span>
                      {check.targets.map((target, i) => (
                        <small key={i}>
                          {target.title} · {target.version || t('local.unknown')}{' '}
                          {!target.enabled && `· ${t('content.status.disabled')}`}
                        </small>
                      ))}
                      <details>
                        <summary>{t('content.fileDetails')}</summary>
                        <p>
                          {t('diagnostics.requiredBy')}: {check.dependency.ownerId}
                        </p>
                        <p>
                          {t('diagnostics.range')}:{' '}
                          {check.dependency.ranges.join(' | ') || t('local.unknown')}
                        </p>
                        {check.targets
                          .filter((target) => target.filename)
                          .map((target, i) => (
                            <p key={i}>
                              {target.directory}/{target.filename}
                            </p>
                          ))}
                      </details>
                      {onSearch &&
                        check.dependency.relation === 'required' &&
                        ['missing', 'unknown'].includes(check.status) &&
                        !['minecraft', 'java', 'fabricloader', 'forge', 'neoforge'].includes(
                          check.dependency.id,
                        ) && (
                          <button
                            className="button secondary"
                            onClick={() => onSearch(check.dependency.id)}
                          >
                            {t('diagnostics.search')}
                          </button>
                        )}
                    </li>
                  ))}
              </ul>
              <details>
                <summary>{t('content.fileDetails')}</summary>
                <p>
                  {mod.directory}/{mod.filename}
                </p>
              </details>
            </details>
          </li>
        ))}
      </ul>
    </section>
  );
}

export function ContentDiagnostics({
  id,
  onClose,
  onSearch,
}: {
  id: string;
  onClose: () => void;
  onSearch: (id: string) => void;
}) {
  const { t } = useFoundation();
  const [report, setReport] = useState<ModDiagnostics | null>(null);
  const [error, setError] = useState<BackendError | null>(null);
  const [revision, setRevision] = useState(0);
  const [pending, setPending] = useState(true);
  useEffect(() => {
    let active = true;
    void backend
      .contentDiagnostics(id)
      .then((value) => {
        if (active) {
          setReport(value);
          setError(null);
        }
      })
      .catch((reason) => {
        if (active) setError(normalizeError(reason));
      })
      .finally(() => {
        if (active) setPending(false);
      });
    return () => {
      active = false;
    };
  }, [id, revision]);
  return (
    <Modal title={t('diagnostics.title')} className="instance-catalog-modal" onClose={onClose}>
      {pending ? (
        <p role="status">{t('diagnostics.checking')}</p>
      ) : (
        report && <DependencyReport report={report} onSearch={onSearch} />
      )}
      {error && <ErrorNotice error={error} />}
      <div className="modal-actions">
        <button
          className="button secondary"
          disabled={pending}
          onClick={() => {
            setPending(true);
            setError(null);
            setRevision((value) => value + 1);
          }}
        >
          {t('local.refresh')}
        </button>
        <button className="button primary" onClick={onClose}>
          {t('common.close')}
        </button>
      </div>
    </Modal>
  );
}
