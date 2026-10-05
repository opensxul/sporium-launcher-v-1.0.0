import { useState } from 'react';
import { Star } from 'lucide-react';
import type { Instance } from '../bindings/core';
import { useFoundation } from '../app/context';
import { useLibrary } from './context';
import { backend, normalizeError } from '../services/backend';
import type { BackendError } from '../services/backend';
import { ErrorNotice } from '../components/ui';

export function FavoriteButton({ instance }: { instance: Instance }) {
  const { t } = useFoundation();
  const { run, busy } = useLibrary();
  const [error, setError] = useState<BackendError | null>(null);
  return (
    <div>
      <button
        className="button secondary"
        aria-pressed={instance.favorite}
        disabled={busy}
        onClick={async () => {
          setError(null);
          try {
            await run(() =>
              backend.setFavorite({
                id: instance.id,
                expectedRevision: instance.revision,
                favorite: !instance.favorite,
              }),
            );
          } catch (reason) {
            setError(normalizeError(reason));
          }
        }}
      >
        <Star size={17} fill={instance.favorite ? 'currentColor' : 'none'} />
        {t(instance.favorite ? 'instance.unfavorite' : 'instance.favorite')}
      </button>
      {error && <ErrorNotice error={error} />}
    </div>
  );
}
