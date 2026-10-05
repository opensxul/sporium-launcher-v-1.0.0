import { useState } from 'react';
import { Archive, Camera, FlaskConical, Folder, Leaf, Users } from 'lucide-react';
import { Link } from 'react-router-dom';
import type { Collection } from '../bindings/core';
import { useFoundation } from '../app/context';
import { useLibrary } from './context';
import { backend, normalizeError } from '../services/backend';
import type { BackendError } from '../services/backend';
import { ErrorNotice } from '../components/ui';

const icons = {
  folder: Folder,
  leaf: Leaf,
  camera: Camera,
  users: Users,
  archive: Archive,
  flask: FlaskConical,
};

export function CollectionCard({
  collection,
  compact = false,
}: {
  collection: Collection;
  compact?: boolean;
}) {
  const { t } = useFoundation();
  const { snapshot, busy, run } = useLibrary();
  const [over, setOver] = useState(false);
  const [error, setError] = useState<BackendError | null>(null);
  const Icon = icons[collection.icon];
  const count = snapshot.instances.filter((item) => item.collectionId === collection.id).length;
  return (
    <div
      className={`collection-card accent-${collection.accent} ${compact ? 'compact' : ''} ${over ? 'drop-active' : ''}`}
      onDragOver={(event) => {
        if (!busy && event.dataTransfer.types.includes('application/x-sporium-instance')) {
          event.preventDefault();
          setOver(true);
          event.dataTransfer.dropEffect = 'move';
        }
      }}
      onDragLeave={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setOver(false);
      }}
      onDrop={async (event) => {
        event.preventDefault();
        setOver(false);
        const instance = snapshot.instances.find(
          (item) => item.id === event.dataTransfer.getData('application/x-sporium-instance'),
        );
        if (!instance || busy) return;
        setError(null);
        try {
          await run(() =>
            backend.updateInstance({
              id: instance.id,
              expectedRevision: instance.revision,
              name: instance.name,
              collectionId: collection.id,
            }),
          );
        } catch (reason) {
          setError(normalizeError(reason));
        }
      }}
    >
      <Link to={`/folders/${collection.id}`}>
        <Icon size={compact ? 18 : 28} strokeWidth={1.4} />
        <div>
          <h3>{collection.name}</h3>
          {!compact && collection.description && <p>{collection.description}</p>}
          <small>
            {t('library.instanceCount')}: {count}
          </small>
        </div>
      </Link>
      {error && <ErrorNotice error={error} />}
    </div>
  );
}
