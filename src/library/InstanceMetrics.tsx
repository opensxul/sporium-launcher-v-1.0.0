import { useEffect, useState } from 'react';
import type { InstanceSummary } from '../bindings/core';
import { useFoundation } from '../app/context';
import { backend, normalizeError, type BackendError } from '../services/backend';
import { ErrorNotice } from '../components/ui';

export function InstanceMetrics({ id, busy }: { id: string; busy: boolean }) {
  const { t, desktop } = useFoundation();
  const [value, setValue] = useState<InstanceSummary | null>(null);
  const [error, setError] = useState<BackendError | null>(null);
  useEffect(() => {
    let active = true;
    if (desktop && !busy)
      void backend
        .instanceSummary(id)
        .then((value) => {
          if (active) {
            setValue(value);
            setError(null);
          }
        })
        .catch((reason) => {
          if (active) setError(normalizeError(reason));
        });
    return () => {
      active = false;
    };
  }, [id, desktop, busy]);
  return (
    <>
      <dl className="instance-metrics">
        <div>
          <dt>{t('ui.modsCount')}</dt>
          <dd>{value ? `${value.enabledMods} / ${value.disabledMods}` : '—'}</dd>
        </div>
        <div>
          <dt>{t('ui.diskSize')}</dt>
          <dd>
            {value?.directoryBytes != null
              ? `${(value.directoryBytes / 1048576).toFixed(1)} MB`
              : '—'}
          </dd>
        </div>
      </dl>
      {error && <ErrorNotice error={error} />}
    </>
  );
}
