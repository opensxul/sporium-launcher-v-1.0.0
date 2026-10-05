import { useState } from 'react';
import { FolderOpen } from 'lucide-react';
import type { FolderTarget } from '../bindings/core';
import { backend, normalizeError } from '../services/backend';
import type { BackendError } from '../services/backend';
import { useFoundation } from '../app/context';
import { useLibrary } from './context';
import { ErrorNotice } from '../components/ui';

export function OpenFolderButton({
  target,
  id,
  label,
}: {
  target: FolderTarget;
  id?: string;
  label: string;
}) {
  const { desktop } = useFoundation();
  const { busy, loading } = useLibrary();
  const [opening, setOpening] = useState(false);
  const [error, setError] = useState<BackendError | null>(null);
  return (
    <span className="open-folder-control">
      <button
        type="button"
        className="button secondary"
        disabled={!desktop || busy || loading || opening}
        onClick={async () => {
          setOpening(true);
          setError(null);
          try {
            await backend.openLibraryFolder({ target, id: id ?? null });
          } catch (reason) {
            setError(normalizeError(reason));
          } finally {
            setOpening(false);
          }
        }}
      >
        <FolderOpen size={16} />
        {label}
      </button>
      {error && <ErrorNotice error={error} />}
    </span>
  );
}
