import { useEffect, useState } from 'react';
import type { Instance, LoaderCatalog } from '../bindings/core';
import { useFoundation } from '../app/context';
import { useLibrary } from '../library/context';
import { backend, normalizeError, type BackendError } from '../services/backend';
import { ErrorNotice } from '../components/ui';

export function InstanceLaunchSettings({
  instance,
  disabled,
}: {
  instance: Instance;
  disabled: boolean;
}) {
  const { t, desktop } = useFoundation();
  const library = useLibrary();
  const [versions, setVersions] = useState<LoaderCatalog | null>(null);
  const [error, setError] = useState<BackendError | null>(null);
  const [attempt, setAttempt] = useState(0);
  const [loaderVersion, setLoaderVersion] = useState(instance.loaderVersion ?? '');
  const [saving, setSaving] = useState(false);
  useEffect(() => {
    let active = true;
    if (!desktop || instance.loader === 'vanilla') return;
    backend
      .loaderVersions(instance.loader, instance.minecraftVersion)
      .then((v) => {
        if (active) {
          setVersions(v);
          setError(null);
        }
      })
      .catch((e) => {
        if (active) setError(normalizeError(e));
      });
    return () => {
      active = false;
    };
  }, [desktop, instance.loader, instance.minecraftVersion, attempt]);
  if (instance.loader === 'vanilla') return null;
  return (
    <section className="surface launch-settings">
      <h2>{t('loader.version')}</h2>
      <div className="form-fields">
        <label>
          {t('loader.version')}
          <select
            value={loaderVersion}
            disabled={!desktop || disabled || saving || !versions}
            onChange={(e) => setLoaderVersion(e.target.value)}
          >
            <option value="">{t('loader.auto')}</option>
            {instance.loaderVersion &&
              !versions?.versions.some((v) => v.id === instance.loaderVersion) && (
                <option value={instance.loaderVersion}>{instance.loaderVersion}</option>
              )}
            {versions?.versions.map((v) => (
              <option key={v.id} value={v.id}>
                {v.id}
                {v.stable ? '' : ` · ${t('loader.preview')}`}
              </option>
            ))}
          </select>
        </label>
        <p className="form-hint">
          {t(
            !versions
              ? 'loader.loading'
              : versions.versions.length
                ? 'loader.hint'
                : 'loader.unavailable',
          )}
        </p>
      </div>
      {error && (
        <ErrorNotice
          error={error}
          action={
            <button className="button secondary" onClick={() => setAttempt((a) => a + 1)}>
              {t('error.retry')}
            </button>
          }
        />
      )}
      <button
        className="button secondary"
        disabled={
          !desktop || disabled || saving || loaderVersion === (instance.loaderVersion ?? '')
        }
        onClick={async () => {
          setSaving(true);
          setError(null);
          try {
            await library.run(() =>
              backend.configureLaunch({
                id: instance.id,
                expectedRevision: instance.revision,
                loaderVersion: loaderVersion || null,
              }),
            );
          } catch (e) {
            setError(normalizeError(e));
          } finally {
            setSaving(false);
          }
        }}
      >
        {t('common.save')}
      </button>
    </section>
  );
}
