import { useState } from 'react';
import type { JavaRuntime, Settings, VersionVisibility } from '../bindings/core';
import { useFoundation } from '../app/context';
import { ErrorNotice, Toggle } from '../components/ui';
import { backend, normalizeError } from '../services/backend';
import type { BackendError } from '../services/backend';

export function VersionSettings({
  value,
  onChange,
  disabled,
}: {
  value: VersionVisibility;
  onChange: (value: VersionVisibility) => void;
  disabled: boolean;
}) {
  const { t } = useFoundation();
  const options = [
    ['releases', 'release'],
    ['snapshots', 'snapshot'],
    ['beta', 'beta'],
    ['alpha', 'alpha'],
    ['other', 'other'],
  ] as const;
  return (
    <>
      <p className="setting-hint">{t('game.visibilityHint')}</p>
      {options.map(([key, label]) => (
        <Toggle
          key={key}
          checked={value[key]}
          onChange={(checked) => onChange({ ...value, [key]: checked })}
          label={t(`game.${label}`)}
          description=""
          disabled={disabled}
        />
      ))}
    </>
  );
}
export function JavaSettings({
  draft,
  update,
  disabled,
}: {
  draft: Settings;
  update: (patch: Partial<Settings>) => void;
  disabled: boolean;
}) {
  const { t } = useFoundation();
  const [runtimes, setRuntimes] = useState<JavaRuntime[] | null>(null);
  const [verified, setVerified] = useState<JavaRuntime | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<BackendError | null>(null);
  async function run(check: boolean) {
    setLoading(true);
    setError(null);
    try {
      if (check && draft.customJavaPath)
        setVerified(await backend.inspectJava(draft.customJavaPath));
      else setRuntimes(await backend.javaRuntimes());
    } catch (reason) {
      setError(normalizeError(reason));
    } finally {
      setLoading(false);
    }
  }
  return (
    <div className="java-settings">
      <p className="setting-hint">{t('java.autoHint')}</p>
      <div className="form-fields">
        <label>
          {t('java.custom')}
          <input
            value={draft.customJavaPath ?? ''}
            disabled={disabled || loading}
            placeholder={t('instance.auto')}
            onChange={(event) => {
              update({ customJavaPath: event.target.value || null });
              setVerified(null);
            }}
          />
        </label>
        <p className="form-hint">{t('java.customHint')}</p>
      </div>
      <div className="instance-actions">
        <button
          className="button secondary"
          disabled={disabled || loading || !draft.customJavaPath}
          onClick={() => void run(true)}
        >
          {t('java.check')}
        </button>
        <button
          className="button secondary"
          disabled={disabled || loading || !draft.customJavaPath}
          onClick={() => {
            update({ customJavaPath: null });
            setVerified(null);
          }}
        >
          {t('java.reset')}
        </button>
      </div>
      {verified && (
        <p role="status">
          {t('java.valid')}: {verified.version} · {verified.architecture}
        </p>
      )}
      <h3>{t('java.detected')}</h3>
      <button
        className="button secondary"
        disabled={disabled || loading}
        onClick={() => void run(false)}
      >
        {t(loading ? 'common.working' : 'java.scan')}
      </button>
      {error && <ErrorNotice error={error} />}
      {runtimes?.length === 0 && <p className="setting-hint">{t('java.none')}</p>}
      <div className="runtime-list">
        {runtimes?.map((runtime) => (
          <article key={runtime.executable}>
            <strong>Java {runtime.version}</strong>
            <small>
              {t(runtime.managed ? 'java.managed' : 'java.system')} · {runtime.architecture}
            </small>
            <code>{runtime.executable}</code>
            <button
              className="button secondary"
              disabled={disabled || loading}
              onClick={() => update({ customJavaPath: runtime.executable })}
            >
              {t('java.use')}
            </button>
          </article>
        ))}
      </div>
    </div>
  );
}
