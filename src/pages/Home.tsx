import { useState } from 'react';
import { ArrowUpRight, Compass, Folder, Search, ShieldCheck, Play } from 'lucide-react';
import { ImportPackButton } from '../packs/PackImports';
import { Link } from 'react-router-dom';
import { useFoundation } from '../app/context';
import { EmptyState, PageHeading, SearchBar } from '../components/ui';
import { LibraryCard } from '../components/LibraryCard';
import { libraries } from '../components/libraries';
import { useLibrary } from '../library/context';
import { CreateInstanceButton } from '../library/InstanceForms';
import { CreateCollectionButton } from '../library/CollectionForms';
import { CollectionCard } from '../library/CollectionCard';
import { InstanceCard } from '../library/InstanceCard';
import { OpenFolderButton } from '../library/OpenFolderButton';
import { LibraryStatus } from '../library/LibraryStatus';
import { useGame } from '../game/context';
import { useContent } from '../content/context';

export function HomePage() {
  const { t, desktop, data } = useFoundation();
  const game = useGame();
  const content = useContent();
  const { snapshot } = useLibrary();
  const [query, setQuery] = useState('');
  const recent = snapshot.instances
    .filter((item) => item.lastPlayedAt !== null)
    .sort((a, b) => (b.lastPlayedAt ?? 0) - (a.lastPlayedAt ?? 0))
    .slice(0, 3);
  const matchingInstances = snapshot.instances.filter((item) =>
    query.trim()
      ? item.name.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase())
      : item.favorite,
  );
  const filtered = libraries.filter((library) =>
    `${library.name} ${t(`library.${library.id}.description`)}`
      .toLocaleLowerCase()
      .includes(query.trim().toLocaleLowerCase()),
  );
  return (
    <div className="page home-page">
      <PageHeading
        eyebrow={t('home.eyebrow')}
        title={t('home.title')}
        description={t('home.description')}
        action={<CreateInstanceButton />}
      />
      <LibraryStatus />
      {!query.trim() && recent.length > 0 && (
        <section className="recent-section" aria-label={t('ui.continue')}>
          <div className="section-toolbar">
            <div>
              <h2>{t('ui.continue')}</h2>
              <span>{t('ui.recentHint')}</span>
            </div>
            <Link className="text-link" to="/instances">
              {t('library.all')}
            </Link>
          </div>
          <div className="recent-grid">
            {recent.map((instance) => (
              <article className="recent-item" key={instance.id}>
                <InstanceCard instance={instance} />
                <div className="recent-launch">
                  <time dateTime={new Date(instance.lastPlayedAt!).toISOString()}>
                    {new Date(instance.lastPlayedAt!).toLocaleDateString(
                      data.settings.values.locale,
                    )}
                  </time>
                  <button
                    className="button primary"
                    aria-label={`${t('game.local')}: ${instance.name}`}
                    disabled={
                      !desktop ||
                      game.busy ||
                      content.busy ||
                      game.state.sessions.some(
                        (session) => session.instanceId === instance.id && session.running,
                      )
                    }
                    onClick={() => void game.start({ id: instance.id, action: 'local' })}
                  >
                    <Play size={16} />
                    {t('game.local')}
                  </button>
                </div>
              </article>
            ))}
          </div>
        </section>
      )}
      <div className="dashboard">
        <section className="library-section">
          <div className="section-toolbar">
            <div>
              <h2>{t('home.libraries')}</h2>
              <span>{t('home.libraryHint')}</span>
            </div>
            <SearchBar
              value={query}
              onChange={setQuery}
              label={t('home.search')}
              clearLabel={t('home.clear')}
            />
          </div>
          {filtered.length ? (
            <div className="library-grid">
              {filtered.map((library) => (
                <LibraryCard key={library.id} library={library} />
              ))}
            </div>
          ) : matchingInstances.length === 0 ? (
            <EmptyState icon={Search} title={t('home.noResults')} body={t('home.noResultsBody')}>
              <button className="button secondary" onClick={() => setQuery('')}>
                {t('home.clear')}
              </button>
            </EmptyState>
          ) : null}
          <p className="library-note">
            <ShieldCheck size={16} />
            {t('home.note')}
          </p>
          {matchingInstances.length > 0 && (
            <section className="library-results">
              <h2>{t(query.trim() ? 'library.matchingInstances' : 'instance.favorites')}</h2>
              <div className="instance-grid">
                {matchingInstances.map((item) => (
                  <InstanceCard key={item.id} instance={item} />
                ))}
              </div>
            </section>
          )}
        </section>
        <aside className="home-rail">
          <section className="rail-panel">
            <div className="rail-heading">
              <h2>{t('folders.title')}</h2>
              <Link to="/folders" className="icon-button" aria-label={t('nav.folders')}>
                <ArrowUpRight size={17} />
              </Link>
            </div>
            <p className="rail-subtitle">{t('folders.subtitle')}</p>
            {snapshot.collections.length ? (
              <div className="rail-collections">
                {snapshot.collections.map((item) => (
                  <CollectionCard key={item.id} collection={item} compact />
                ))}
              </div>
            ) : (
              <div className="folder-preview">
                <Folder size={35} strokeWidth={1.1} />
                <h3>{t('folders.empty')}</h3>
                <p>{t('folders.emptyBody')}</p>
              </div>
            )}
            <CreateCollectionButton />
          </section>
          <section className="rail-panel quick-actions">
            <h2>{t('quick.title')}</h2>
            <CreateInstanceButton secondary />
            <ImportPackButton />
            <Link to="/catalog">
              <Compass size={17} />
              {t('quick.find')}
              <ArrowUpRight size={14} />
            </Link>
            <OpenFolderButton target="data" label={t('quick.folder')} />
          </section>
          <div className="forest-note">
            <span className="tiny-spark" aria-hidden="true">
              ✦
            </span>
            <span>{t('app.subtitle')}</span>
          </div>
        </aside>
      </div>
    </div>
  );
}
