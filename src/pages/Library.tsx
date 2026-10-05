import { useState } from 'react';
import { Box } from 'lucide-react';
import { useParams } from 'react-router-dom';
import { useFoundation } from '../app/context';
import { useLibrary } from '../library/context';
import { LibraryCard } from '../components/LibraryCard';
import { libraries } from '../components/libraries';
import { EmptyState, PageHeading, SearchBar } from '../components/ui';
import { CreateInstanceButton } from '../library/InstanceForms';
import { InstanceCard } from '../library/InstanceCard';
import { LibraryStatus } from '../library/LibraryStatus';
import { belongsToLibrary, libraryLoader } from '../library/view-model';
import { NotFoundPage } from './StaticPages';
import { ImportPackButton } from '../packs/PackImports';

export function LibraryPage() {
  const { kind } = useParams();
  const { t } = useFoundation();
  const { snapshot, loading, error } = useLibrary();
  const [query, setQuery] = useState('');
  const [favorites, setFavorites] = useState(false);
  const [sort, setSort] = useState('recent');
  const library = libraries.find((item) => item.id === kind);
  if (kind && !library) return <NotFoundPage />;
  const managed = kind === 'studio' || kind === 'managed';
  const instances = snapshot.instances
    .filter(
      (item) =>
        (!kind || belongsToLibrary(item, kind)) &&
        (!favorites || item.favorite) &&
        `${item.name} ${item.minecraftVersion}`
          .toLocaleLowerCase()
          .includes(query.trim().toLocaleLowerCase()),
    )
    .sort((a, b) =>
      sort === 'name'
        ? a.name.localeCompare(b.name)
        : sort === 'created'
          ? b.createdAt - a.createdAt
          : (b.lastPlayedAt ?? 0) - (a.lastPlayedAt ?? 0) || b.createdAt - a.createdAt,
    );
  return (
    <div className="page">
      <PageHeading
        title={library?.name ?? t('nav.instances')}
        description={library ? t(`library.${library.id}.description`) : t('library.description')}
        action={
          !managed && (
            <div className="filter-buttons">
              <ImportPackButton />
              <CreateInstanceButton loader={libraryLoader(kind)} />
            </div>
          )
        }
      />
      <LibraryStatus />
      {!kind && (
        <details className="library-categories">
          <summary>{t('home.libraries')}</summary>
          <div className="library-grid library-grid-full">
            {libraries.map((item) => (
              <LibraryCard key={item.id} library={item} />
            ))}
          </div>
        </details>
      )}
      <section className="library-results">
        <div className="section-toolbar">
          <div className="filter-buttons">
            <button
              className={`button secondary ${!favorites ? 'active' : ''}`}
              aria-pressed={!favorites}
              onClick={() => setFavorites(false)}
            >
              {t('instance.all')}
            </button>
            <button
              className={`button secondary ${favorites ? 'active' : ''}`}
              aria-pressed={favorites}
              onClick={() => setFavorites(true)}
            >
              {t('instance.favorites')}
            </button>
          </div>
          <SearchBar
            value={query}
            onChange={setQuery}
            label={t('library.searchInstances')}
            clearLabel={t('home.clear')}
          />
          <select
            className="library-sort"
            aria-label={t('ui.sort')}
            value={sort}
            onChange={(event) => setSort(event.target.value)}
          >
            <option value="recent">{t('ui.sortRecent')}</option>
            <option value="name">{t('ui.sortName')}</option>
            <option value="created">{t('ui.sortCreated')}</option>
          </select>
        </div>
        {instances.length ? (
          <div className="instance-grid">
            {instances.map((item) => (
              <InstanceCard key={item.id} instance={item} />
            ))}
          </div>
        ) : (
          !loading &&
          !error && (
            <EmptyState
              icon={library?.icon ?? Box}
              title={query || favorites ? t('home.noResults') : t('library.emptyTitle')}
              body={managed ? t('library.managedHint') : t('library.createHint')}
            />
          )
        )}
      </section>
    </div>
  );
}
