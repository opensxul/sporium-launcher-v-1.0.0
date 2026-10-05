import { useState } from 'react';
import { LoaderCircle, Plus } from 'lucide-react';
import { useNavigate } from 'react-router-dom';
import type { Instance, Loader } from '../bindings/core';
import { useFoundation } from '../app/context';
import { useLibrary } from './context';
import { backend, normalizeError } from '../services/backend';
import type { BackendError } from '../services/backend';
import { ErrorNotice } from '../components/ui';
import { Modal } from '../components/Modal';
import { loaderNames } from './view-model';
import { VersionPicker } from '../game/VersionPicker';

export function CreateInstanceButton({
  loader,
  collectionId,
  secondary = false,
}: {
  loader?: Loader;
  collectionId?: string;
  secondary?: boolean;
}) {
  const { t, desktop } = useFoundation();
  const { loading, busy, error } = useLibrary();
  const [open, setOpen] = useState(false);
  return (
    <>
      <button
        className={`button ${secondary ? 'secondary' : 'primary'}`}
        disabled={!desktop || loading || busy || !!error}
        onClick={() => setOpen(true)}
      >
        <Plus size={17} />
        {t('quick.create')}
      </button>
      {open && (
        <InstanceEditor
          mode="create"
          loader={loader}
          collectionId={collectionId}
          onClose={() => setOpen(false)}
        />
      )}
    </>
  );
}

export function InstanceEditor({
  mode,
  instance,
  loader,
  collectionId,
  onClose,
}: {
  mode: 'create' | 'edit' | 'duplicate';
  instance?: Instance;
  loader?: Loader;
  collectionId?: string;
  onClose: () => void;
}) {
  const { t } = useFoundation();
  const { snapshot, busy, run, reload } = useLibrary();
  const navigate = useNavigate();
  const [name, setName] = useState(
    mode === 'duplicate'
      ? `${instance?.name ?? ''} ${t('instance.copySuffix')}`.slice(0, 80)
      : (instance?.name ?? ''),
  );
  const [version, setVersion] = useState('');
  const [selectedLoader, setLoader] = useState<Loader>(loader ?? 'vanilla');
  const [folder, setFolder] = useState(collectionId ?? instance?.collectionId ?? '');
  const [error, setError] = useState<BackendError | null>(null);
  const title = t(
    mode === 'create'
      ? 'quick.create'
      : mode === 'duplicate'
        ? 'instance.duplicate'
        : 'instance.edit',
  );
  return (
    <Modal title={title} onClose={onClose} busy={busy}>
      <form
        onSubmit={async (event) => {
          event.preventDefault();
          setError(null);
          try {
            const result = await run(() =>
              mode === 'create'
                ? backend.createInstance({
                    name,
                    minecraftVersion: version,
                    loader: selectedLoader,
                    collectionId: folder || null,
                  })
                : mode === 'duplicate' && instance
                  ? backend.duplicateInstance({
                      id: instance.id,
                      expectedRevision: instance.revision,
                      name,
                    })
                  : instance
                    ? backend.updateInstance({
                        id: instance.id,
                        expectedRevision: instance.revision,
                        name,
                        collectionId: folder || null,
                      })
                    : Promise.reject(new Error('Missing instance')),
            );
            onClose();
            if (mode !== 'edit') void navigate(`/instance/${result.affectedId}`);
          } catch (reason) {
            setError(normalizeError(reason));
          }
        }}
      >
        <fieldset disabled={busy} className="form-fields">
          <label>
            {t('instance.name')}
            <input
              required
              maxLength={80}
              value={name}
              onChange={(event) => setName(event.target.value)}
              autoFocus
            />
          </label>
          {mode === 'create' && (
            <>
              <VersionPicker value={version} onChange={setVersion} />
              <p className="form-hint">{t('instance.versionHint')}</p>
              <label>
                {t('instance.loader')}
                <select
                  aria-label={t('instance.loader')}
                  value={selectedLoader}
                  onChange={(event) => setLoader(event.target.value as Loader)}
                >
                  {Object.entries(loaderNames).map(([value, label]) => (
                    <option key={value} value={value}>
                      {label}
                    </option>
                  ))}
                </select>
              </label>
            </>
          )}
          {mode !== 'duplicate' && (
            <label>
              {t('instance.collection')}
              <select
                aria-label={t('instance.collection')}
                value={folder}
                onChange={(event) => setFolder(event.target.value)}
              >
                <option value="">{t('instance.noCollection')}</option>
                {snapshot.collections.map((item) => (
                  <option key={item.id} value={item.id}>
                    {item.name}
                  </option>
                ))}
              </select>
            </label>
          )}
        </fieldset>
        {mode === 'duplicate' && <p className="form-hint">{t('instance.duplicateHint')}</p>}
        {error && (
          <ErrorNotice
            error={error}
            action={
              error.code === 'RECORD_CONFLICT' && (
                <button
                  type="button"
                  className="button secondary"
                  onClick={async () => {
                    await reload();
                    onClose();
                  }}
                >
                  {t('library.reload')}
                </button>
              )
            }
          />
        )}
        <div className="modal-actions">
          <button type="button" className="button secondary" disabled={busy} onClick={onClose}>
            {t('common.cancel')}
          </button>
          <button className="button primary" disabled={busy}>
            {busy && <LoaderCircle className="spin" size={16} />}
            {t(
              busy
                ? 'common.working'
                : mode === 'create'
                  ? 'common.create'
                  : mode === 'duplicate'
                    ? 'instance.duplicate'
                    : 'common.save',
            )}
          </button>
        </div>
      </form>
    </Modal>
  );
}

export function DeleteInstanceDialog({
  instance,
  onClose,
}: {
  instance: Instance;
  onClose: () => void;
}) {
  const { t } = useFoundation();
  const { busy, run } = useLibrary();
  const navigate = useNavigate();
  const [preserve, setPreserve] = useState(true);
  const [error, setError] = useState<BackendError | null>(null);
  return (
    <Modal title={t('instance.deleteTitle')} onClose={onClose} busy={busy}>
      <form
        onSubmit={async (event) => {
          event.preventDefault();
          setError(null);
          try {
            await run(() =>
              backend.deleteInstance({
                id: instance.id,
                expectedRevision: instance.revision,
                mode: preserve ? 'preserve_worlds' : 'everything',
              }),
            );
            onClose();
            void navigate('/instances');
          } catch (reason) {
            setError(normalizeError(reason));
          }
        }}
      >
        <p className="delete-target">{instance.name}</p>
        <p>{t('instance.deleteHint')}</p>
        <fieldset className="delete-options" disabled={busy}>
          <legend>{t('instance.deleteFiles')}</legend>
          <label>
            <input
              type="radio"
              name="delete-mode"
              checked={preserve}
              onChange={() => setPreserve(true)}
            />
            {t('instance.preserve')}
          </label>
          <label>
            <input
              type="radio"
              name="delete-mode"
              checked={!preserve}
              onChange={() => setPreserve(false)}
            />
            {t('instance.deleteEverything')}
          </label>
        </fieldset>
        {error && <ErrorNotice error={error} />}
        <div className="modal-actions">
          <button type="button" className="button secondary" disabled={busy} onClick={onClose}>
            {t('common.cancel')}
          </button>
          <button className="button danger" disabled={busy}>
            {busy && <LoaderCircle size={16} className="spin" />}
            {t('common.delete')}
          </button>
        </div>
      </form>
    </Modal>
  );
}
