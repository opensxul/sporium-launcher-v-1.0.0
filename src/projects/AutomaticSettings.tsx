import { useEffect, useState } from 'react';
import type { AutomaticPolicy, AutomaticReport, UpdateMode } from '../bindings/core';
import { useFoundation } from '../app/context';
import { backend, normalizeError, type BackendError } from '../services/backend';
import { ErrorNotice } from '../components/ui';
import type { MessageKey } from '../i18n/messages';

export function AutomaticSettings({ id, disabled = false }: { id?: string; disabled?: boolean }) {
  const { t, desktop, data } = useFoundation();
  const [draft, setDraft] = useState<AutomaticPolicy | null>(null);
  const [original, setOriginal] = useState('');
  const [accepted, setAccepted] = useState(false);
  const [working, setWorking] = useState(false);
  const [saved, setSaved] = useState(false);
  const [report, setReport] = useState<AutomaticReport | null>(null);
  const [error, setError] = useState<BackendError | null>(null);
  useEffect(() => {
    if (!desktop) return;
    let active = true;
    void Promise.all([backend.automaticPolicy(), backend.automaticReports()])
      .then(([policy, reports]) => {
        if (active) {
          setDraft(policy);
          setOriginal(JSON.stringify(policy));
          setReport(reports.find((r) => r.instanceId === id) ?? null);
        }
      })
      .catch((reason) => {
        if (active) setError(normalizeError(reason));
      });
    return () => {
      active = false;
    };
  }, [id, desktop]);
  const override = draft?.instances.find((value) => value.id === id);
  const dirty = !!draft && JSON.stringify(draft) !== original;
  const needsConsent =
    dirty &&
    (id
      ? override?.content === 'install' || override?.project === 'install'
      : draft?.content === 'install' || draft?.project === 'install');
  function change(key: 'content' | 'project', value: UpdateMode | 'inherit') {
    if (!draft) return;
    setSaved(false);
    setAccepted(false);
    if (!id) setDraft({ ...draft, [key]: value as UpdateMode });
    else {
      const item = {
        id,
        content: override?.content ?? null,
        project: override?.project ?? null,
        [key]: value === 'inherit' ? null : value,
      };
      setDraft({ ...draft, instances: [...draft.instances.filter((i) => i.id !== id), item] });
    }
  }
  async function save() {
    if (!draft) return;
    setWorking(true);
    setError(null);
    try {
      await backend.saveAutomaticPolicy(draft);
      setOriginal(JSON.stringify(draft));
      setSaved(true);
      setAccepted(false);
    } catch (reason) {
      setError(normalizeError(reason));
    } finally {
      setWorking(false);
    }
  }
  async function check() {
    if (!id) return;
    setWorking(true);
    setError(null);
    try {
      const result = await backend.automaticCheck(id);
      setReport(result);
      if (result.status === 'project_installed')
        window.dispatchEvent(new Event('sporium-project-changed'));
    } catch (reason) {
      setError(normalizeError(reason));
    } finally {
      setWorking(false);
    }
  }
  const locked = disabled || working || !desktop || !draft;
  return (
    <section className="surface project-automatic">
      <h3>{t('auto.title')}</h3>
      <p className="setting-hint">{t('auto.hint')}</p>
      {error && <ErrorNotice error={error} />}
      {(['content', 'project'] as const).map((key) => (
        <label className="setting-row" key={key}>
          <span className="setting-label">{t(`auto.${key}`)}</span>
          <select
            aria-label={t(`auto.${key}`)}
            disabled={locked}
            value={id ? (override?.[key] ?? 'inherit') : (draft?.[key] ?? 'check')}
            onChange={(event) => change(key, event.target.value as UpdateMode | 'inherit')}
          >
            {id && <option value="inherit">{t('auto.inherit')}</option>}
            <option value="off">{t('auto.off')}</option>
            <option value="check">{t('auto.check')}</option>
            <option value="install">{t('auto.install')}</option>
          </select>
        </label>
      ))}
      {needsConsent && (
        <label className="project-consent">
          <input
            type="checkbox"
            checked={accepted}
            onChange={(event) => setAccepted(event.target.checked)}
            disabled={locked}
          />
          {t('auto.accept')}
        </label>
      )}
      <div className="project-actions">
        <button
          className="button secondary"
          disabled={locked || !dirty || (needsConsent && !accepted)}
          onClick={() => void save()}
        >
          {t('auto.save')}
        </button>
        {id && (
          <button
            className="button secondary"
            disabled={locked || dirty}
            onClick={() => void check()}
          >
            {t('auto.checkNow')}
          </button>
        )}
      </div>
      {saved && <p role="status">{t('auto.saved')}</p>}
      {id && (
        <p className="setting-hint">
          {report
            ? `${t('auto.last')}: ${new Date(report.checkedAt).toLocaleString(data.settings.values.locale)} · ${t(`auto.${report.status}` as MessageKey)}`
            : t('auto.empty')}
        </p>
      )}
      {report && (
        <details>
          <summary>{t('project.details')}</summary>
          <ul>
            {report.updates.map((item) => (
              <li key={item.projectId}>
                {item.title} · {t(`updates.${item.status}` as MessageKey)}
              </li>
            ))}
          </ul>
          {report.project && (
            <p>
              {t(`project.${report.project.status}` as MessageKey)}
              {report.project.available &&
                ` · ${report.project.current} → ${report.project.available}`}
            </p>
          )}
        </details>
      )}
    </section>
  );
}
