import { useState } from 'react';
import { ArrowLeft, Folder, Pencil, Trash2 } from 'lucide-react';
import { Link, useNavigate, useParams } from 'react-router-dom';
import { useFoundation } from '../app/context';
import { useLibrary } from '../library/context';
import { LibraryStatus } from '../library/LibraryStatus';
import {
  CreateCollectionButton,
  CollectionEditor,
  DeleteCollectionDialog,
} from '../library/CollectionForms';
import { CreateInstanceButton, InstanceEditor } from '../library/InstanceForms';
import { CollectionCard } from '../library/CollectionCard';
import { InstanceCard } from '../library/InstanceCard';
import { EmptyState, PageHeading } from '../components/ui';
import { NotFoundPage } from './StaticPages';

export function FoldersPage() {
  const { id } = useParams();
  const navigate = useNavigate();
  const { t } = useFoundation();
  const { snapshot, busy, loading, error } = useLibrary();
  const [dialog, setDialog] = useState<'edit' | 'delete' | null>(null);
  const [selected, setSelected] = useState('');
  const [assigning, setAssigning] = useState(false);
  const collection = snapshot.collections.find((item) => item.id === id);
  const instances = snapshot.instances.filter((item) => item.collectionId === id);
  const available = snapshot.instances.filter((item) => item.collectionId !== id);
  const assignment = available.find((item) => item.id === selected);
  if (id && !collection && !loading && !error) return <NotFoundPage />;
  return (
    <div className="page">
      {id && (
        <Link className="back-link" to="/folders">
          <ArrowLeft size={16} />
          {t('folders.back')}
        </Link>
      )}
      <PageHeading
        title={collection?.name ?? t('folders.title')}
        description={collection?.description || t('folders.dragHint')}
        action={
          collection ? (
            <CreateInstanceButton collectionId={collection.id} />
          ) : (
            <CreateCollectionButton />
          )
        }
      />
      <LibraryStatus />
      {collection ? (
        <>
          <div className="instance-actions">
            <button className="button secondary" disabled={busy} onClick={() => setDialog('edit')}>
              <Pencil size={16} />
              {t('folders.edit')}
            </button>
            <button
              className="button secondary danger-text"
              disabled={busy}
              onClick={() => setDialog('delete')}
            >
              <Trash2 size={16} />
              {t('folders.delete')}
            </button>
          </div>
          {available.length > 0 && (
            <div className="assign-row">
              <label>
                {t('instance.assign')}
                <select
                  aria-label={t('instance.assign')}
                  value={selected}
                  onChange={(event) => setSelected(event.target.value)}
                >
                  <option value="">{t('instance.assignHint')}</option>
                  {available.map((item) => (
                    <option key={item.id} value={item.id}>
                      {item.name}
                    </option>
                  ))}
                </select>
              </label>
              <button
                className="button secondary"
                disabled={busy || !assignment}
                onClick={() => setAssigning(true)}
              >
                {t('instance.saveAssignment')}
              </button>
            </div>
          )}
          {instances.length ? (
            <div className="instance-grid">
              {instances.map((item) => (
                <InstanceCard key={item.id} instance={item} />
              ))}
            </div>
          ) : (
            <EmptyState
              icon={Folder}
              title={t('library.noInstances')}
              body={t('library.createHint')}
            />
          )}
        </>
      ) : snapshot.collections.length ? (
        <div className="collection-grid">
          {snapshot.collections.map((item) => (
            <CollectionCard key={item.id} collection={item} />
          ))}
        </div>
      ) : (
        !loading &&
        !error && (
          <EmptyState icon={Folder} title={t('folders.empty')} body={t('folders.emptyBody')} />
        )
      )}
      {collection && dialog === 'edit' && (
        <CollectionEditor collection={collection} onClose={() => setDialog(null)} />
      )}
      {collection && dialog === 'delete' && (
        <DeleteCollectionDialog
          collection={collection}
          onClose={() => setDialog(null)}
          onDeleted={() => void navigate('/folders')}
        />
      )}
      {collection && assignment && assigning && (
        <InstanceEditor
          mode="edit"
          instance={assignment}
          collectionId={collection.id}
          onClose={() => setAssigning(false)}
        />
      )}
    </div>
  );
}
