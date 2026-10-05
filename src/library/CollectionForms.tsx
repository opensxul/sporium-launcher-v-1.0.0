import { useState } from 'react';
import { Plus } from 'lucide-react';
import type { Accent, Collection, CollectionIcon } from '../bindings/core';
import { useFoundation } from '../app/context';
import { useLibrary } from './context';
import { backend, normalizeError } from '../services/backend';
import type { BackendError } from '../services/backend';
import { Modal } from '../components/Modal';
import { ErrorNotice } from '../components/ui';

export function CreateCollectionButton() {
  const { t, desktop } = useFoundation();
  const { busy, loading, error } = useLibrary();
  const [open, setOpen] = useState(false);
  return (
    <>
      <button
        className="button secondary"
        disabled={!desktop || busy || loading || !!error}
        onClick={() => setOpen(true)}
      >
        <Plus size={16} />
        {t('folders.create')}
      </button>
      {open && <CollectionEditor onClose={() => setOpen(false)} />}
    </>
  );
}

export function CollectionEditor({
  collection,
  onClose,
}: {
  collection?: Collection;
  onClose: () => void;
}) {
  const { t } = useFoundation();
  const { run, busy } = useLibrary();
  const [name, setName] = useState(collection?.name ?? '');
  const [description, setDescription] = useState(collection?.description ?? '');
  const [accent, setAccent] = useState<Accent>(collection?.accent ?? 'sage');
  const [icon, setIcon] = useState<CollectionIcon>(collection?.icon ?? 'folder');
  const [error, setError] = useState<BackendError | null>(null);
  return (
    <Modal title={t(collection ? 'folders.edit' : 'folders.create')} onClose={onClose} busy={busy}>
      <form
        onSubmit={async (event) => {
          event.preventDefault();
          setError(null);
          try {
            await run(() =>
              backend.saveCollection({
                id: collection?.id ?? null,
                expectedRevision: collection?.revision ?? null,
                name,
                description,
                accent,
                icon,
              }),
            );
            onClose();
          } catch (reason) {
            setError(normalizeError(reason));
          }
        }}
      >
        <fieldset className="form-fields" disabled={busy}>
          <label>
            {t('folders.name')}
            <input
              required
              autoFocus
              maxLength={80}
              value={name}
              onChange={(event) => setName(event.target.value)}
            />
          </label>
          <label>
            {t('folders.description')}
            <input
              maxLength={280}
              value={description}
              onChange={(event) => setDescription(event.target.value)}
            />
          </label>
          <div className="form-columns">
            <label>
              {t('folders.accent')}
              <select
                aria-label={t('folders.accent')}
                value={accent}
                onChange={(event) => setAccent(event.target.value as Accent)}
              >
                {(['sage', 'amber', 'rose', 'teal', 'slate'] as const).map((value) => (
                  <option key={value} value={value}>
                    {t(`accent.${value}`)}
                  </option>
                ))}
              </select>
            </label>
            <label>
              {t('folders.icon')}
              <select
                aria-label={t('folders.icon')}
                value={icon}
                onChange={(event) => setIcon(event.target.value as CollectionIcon)}
              >
                {(['folder', 'leaf', 'camera', 'users', 'archive', 'flask'] as const).map(
                  (value) => (
                    <option key={value} value={value}>
                      {t(`icon.${value}`)}
                    </option>
                  ),
                )}
              </select>
            </label>
          </div>
        </fieldset>
        {error && <ErrorNotice error={error} />}
        <div className="modal-actions">
          <button type="button" className="button secondary" onClick={onClose} disabled={busy}>
            {t('common.cancel')}
          </button>
          <button className="button primary" disabled={busy}>
            {t(busy ? 'common.working' : 'common.save')}
          </button>
        </div>
      </form>
    </Modal>
  );
}

export function DeleteCollectionDialog({
  collection,
  onClose,
  onDeleted,
}: {
  collection: Collection;
  onClose: () => void;
  onDeleted: () => void;
}) {
  const { t } = useFoundation();
  const { run, busy } = useLibrary();
  const [error, setError] = useState<BackendError | null>(null);
  return (
    <Modal title={t('folders.delete')} onClose={onClose} busy={busy}>
      <p className="delete-target">{collection.name}</p>
      <p>{t('folders.deleteHint')}</p>
      {error && <ErrorNotice error={error} />}
      <div className="modal-actions">
        <button className="button secondary" onClick={onClose} disabled={busy}>
          {t('common.cancel')}
        </button>
        <button
          className="button danger"
          disabled={busy}
          onClick={async () => {
            setError(null);
            try {
              await run(() =>
                backend.deleteCollection({
                  id: collection.id,
                  expectedRevision: collection.revision,
                }),
              );
              onClose();
              onDeleted();
            } catch (reason) {
              setError(normalizeError(reason));
            }
          }}
        >
          {t('common.delete')}
        </button>
      </div>
    </Modal>
  );
}
