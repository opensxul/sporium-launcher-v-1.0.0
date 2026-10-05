import { useCallback, useEffect, useState } from 'react';
import { BrandMark } from '../components/BrandMark';
import type { ReactNode } from 'react';
import type { Bootstrap, Settings } from '../bindings/core';
import { translate } from '../i18n';
import { backend, browserPreview, normalizeError } from '../services/backend';
import type { BackendError } from '../services/backend';
import { FoundationContext } from './context';
import { AlertTriangle, LoaderCircle, RotateCw } from 'lucide-react';

export function Foundation({ children }: { children: ReactNode }) {
  const [data, setData] = useState<Bootstrap | null>(null);
  const [error, setError] = useState<BackendError | null>(null);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let active = true;
    const operation = backend.isDesktop ? backend.bootstrap() : Promise.resolve(browserPreview());
    operation
      .then((result) => {
        if (active) {
          setData(result);
          setError(null);
        }
      })
      .catch((reason: unknown) => {
        if (active) setError(normalizeError(reason));
      });
    return () => {
      active = false;
    };
  }, [attempt]);

  useEffect(() => {
    if (!data) return;
    document.documentElement.lang = data.settings.values.locale;
    document.documentElement.dataset.motion = data.settings.values.motion;
    document.documentElement.style.fontSize = `${data.settings.values.uiScale}%`;
  }, [data]);

  const reload = useCallback(async () => {
    const result = await backend.bootstrap();
    setData(result);
    return result.settings;
  }, []);

  const saveSettings = useCallback(
    async (values: Settings) => {
      if (!data) throw new Error('Not initialized');
      const settings = await backend.saveSettings({
        values,
        expectedRevision: data.settings.revision,
      });
      setData({ ...data, settings });
      return settings;
    },
    [data],
  );

  const t = useCallback(
    (key: Parameters<typeof translate>[1]) =>
      translate(data?.settings.values.locale ?? 'ru-RU', key),
    [data?.settings.values.locale],
  );

  if (!data)
    return (
      <div className="startup-screen">
        <BrandMark />
        {error ? (
          <div className="startup-card" role="alert">
            <AlertTriangle size={28} />
            <h1>{t('error.title')}</h1>
            <p>{t(`error.${error.code}`)}</p>
            {error.retryable && (
              <button
                className="button primary"
                onClick={() => {
                  setError(null);
                  setAttempt(attempt + 1);
                }}
              >
                <RotateCw size={16} />
                {t('error.retry')}
              </button>
            )}
          </div>
        ) : (
          <p className="loading-line" role="status">
            <LoaderCircle className="spin" size={20} />
            {t('app.loading')}
          </p>
        )}
      </div>
    );

  return (
    <FoundationContext value={{ data, desktop: backend.isDesktop, t, saveSettings, reload }}>
      {children}
    </FoundationContext>
  );
}
