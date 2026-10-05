import { useEffect, useState } from 'react';
import type { Instance, InstanceLogo } from '../bindings/core';
import { useFoundation } from '../app/context';
import { useLibrary } from './context';
import { InstanceIcon } from './InstanceIcon';
import { backend, normalizeError, type BackendError } from '../services/backend';
import { ErrorNotice } from '../components/ui';
export function InstanceAppearance({ instance }: { instance: Instance }) {
  const { t, desktop } = useFoundation();
  const library = useLibrary();
  const [logos, setLogos] = useState<InstanceLogo[]>([]);
  const [working, setWorking] = useState(false);
  const [error, setError] = useState<BackendError | null>(null);
  useEffect(() => {
    if (desktop)
      void backend
        .instanceIconCatalog()
        .then(setLogos)
        .catch((error) => setError(normalizeError(error)));
  }, [desktop]);
  async function change(choice?: string) {
    setWorking(true);
    setError(null);
    try {
      const request = { id: instance.id, expectedRevision: instance.revision };
      if (choice) await backend.setInstanceIcon(request, choice);
      else await backend.pickInstanceIcon(request);
      await library.reload();
    } catch (reason) {
      setError(normalizeError(reason));
    } finally {
      setWorking(false);
    }
  }
  const disabled = !desktop || library.busy || working;
  return (
    <section className="surface appearance-panel">
      <h2>{t('ui.icon')}</h2>
      <div className="appearance-preview">
        <InstanceIcon instance={instance} />
        <p>{t('ui.iconHint')}</p>
      </div>
      <div className="instance-actions">
        <button className="button secondary" disabled={disabled} onClick={() => void change()}>
          {t('ui.iconUpload')}
        </button>
        <button
          className="button secondary"
          disabled={disabled || !instance.iconRef}
          onClick={() => void change('automatic')}
        >
          {t('ui.iconReset')}
        </button>
        <button
          className="button secondary"
          disabled={disabled || !logos.length}
          onClick={() => void change('random')}
        >
          {t('ui.iconRandom')}
        </button>
      </div>
      {logos.length ? (
        <div className="logo-options" role="group" aria-label={t('ui.iconChoose')}>
          {logos.map((logo) => (
            <button
              className="icon-button"
              key={logo.id}
              aria-label={logo.name}
              aria-pressed={instance.iconRef === `builtin:${logo.id}`}
              disabled={disabled}
              onClick={() => void change(logo.id)}
            >
              <img src={logo.image} alt="" width={56} height={56} />
            </button>
          ))}
        </div>
      ) : (
        <p className="setting-hint">{t('ui.iconPending')}</p>
      )}
      {error && <ErrorNotice error={error} />}
    </section>
  );
}
