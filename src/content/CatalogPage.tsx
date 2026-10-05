import { ProjectIcon } from './ProjectIcon';
import { useEffect, useState } from 'react';
import { Link, useSearchParams } from 'react-router-dom';
import { Compass, Search, LockKeyhole, Check } from 'lucide-react';
import type { CatalogPage as SearchPage, CatalogQuery, ContentTags } from '../bindings/core';
import type { BackendError } from '../services/backend';
import { backend, normalizeError } from '../services/backend';
import { useFoundation } from '../app/context';
import { useLibrary } from '../library/context';
import { EmptyState, ErrorNotice, PageHeading } from '../components/ui';
import { ProjectDialog } from './ProjectDialog';
import { ContentActivity } from './ContentActivity';
import { loaderNames } from '../library/view-model';
import { Modal } from '../components/Modal';
import { useContent } from './context';
import type { InstalledContent as InstalledRecord } from '../bindings/core';
import type { MessageKey } from '../i18n/messages';

export function CatalogPage() {
  const [params, setParams] = useSearchParams();
  return <CatalogBrowser params={params} setParams={setParams} />;
}

export function InstanceCatalogDialog({
  id,
  records,
  disabled,
  initialKind = 'mod',
  initialQuery = '',
  onClose,
}: {
  id: string;
  records: InstalledRecord[];
  disabled: boolean;
  initialKind?: string;
  initialQuery?: string;
  onClose: () => void;
}) {
  const { t } = useFoundation();
  const [params, setParams] = useState(
    () => new URLSearchParams({ instance: id, kind: initialKind, q: initialQuery }),
  );
  return (
    <Modal title={t('content.browseInstance')} className="instance-catalog-modal" onClose={onClose}>
      <CatalogBrowser
        params={params}
        setParams={setParams}
        embedded
        records={records}
        disabled={disabled}
      />
    </Modal>
  );
}

function CatalogBrowser({
  params,
  setParams,
  embedded = false,
  records = [],
  disabled = false,
}: {
  params: URLSearchParams;
  setParams: (params: URLSearchParams) => void;
  embedded?: boolean;
  records?: InstalledRecord[];
  disabled?: boolean;
}) {
  const { t, desktop, data } = useFoundation();
  const { snapshot } = useLibrary();
  const serialized = params.toString();
  const instanceId = params.get('instance');
  const instance = snapshot.instances.find((i) => i.id === instanceId);
  const [result, setResult] = useState<SearchPage | null>(null);
  const [tags, setTags] = useState<ContentTags | null>(null);
  const [error, setError] = useState<BackendError | null>(null);
  const [loading, setLoading] = useState(false);
  const [attempt, setAttempt] = useState(0);
  const [project, setProject] = useState<string | null>(null);
  const [hideInstalled, setHideInstalled] = useState(false);
  const content = useContent();
  const installed = new Set(
    records
      .filter(
        (r) =>
          ['installed', 'disabled'].includes(r.status) &&
          records
            .filter((other) => other.record.projectId === r.record.projectId)
            .every((other) => ['installed', 'disabled'].includes(other.status)),
      )
      .map((r) => r.record.projectId),
  );
  const hits = result?.hits.filter((hit) => !hideInstalled || !installed.has(hit.id)) ?? [];
  const kind = params.get('kind') ?? 'mod';
  function selectKind(value: string) {
    if (value === kind) return;
    const next = new URLSearchParams(params);
    next.set('kind', value);
    next.delete('category');
    next.delete('offset');
    setLoading(true);
    setParams(next);
  }
  useEffect(() => {
    if (!desktop) return;
    let active = true;
    void backend
      .contentTags()
      .then((value) => {
        if (active) setTags(value);
      })
      .catch(() => {});
    return () => {
      active = false;
    };
  }, [desktop]);
  useEffect(() => {
    if (!desktop) return;
    let active = true;
    const values = new URLSearchParams(serialized);
    const query: CatalogQuery = {
      query: values.get('q') ?? '',
      kind: values.get('kind') ?? 'mod',
      minecraft: values.get('minecraft') ?? '',
      loader: values.get('loader') ?? '',
      category: values.get('category') ?? '',
      environment: values.get('environment') ?? '',
      sort: values.get('sort') ?? 'relevance',
      offset: Number(values.get('offset') ?? 0),
      instanceId: values.get('instance'),
    };
    // State is changed by request events; scheduling keeps effects as external synchronization.
    const timer = setTimeout(() => {
      setLoading(true);
      setError(null);
      void backend
        .contentSearch(query)
        .then((value) => {
          if (active) setResult(value);
        })
        .catch((reason) => {
          if (active) setError(normalizeError(reason));
        })
        .finally(() => {
          if (active) setLoading(false);
        });
    }, 100);
    return () => {
      active = false;
      clearTimeout(timer);
    };
  }, [desktop, serialized, attempt]);
  const offset = Number(params.get('offset') ?? 0);
  function paginate(value: number) {
    const next = new URLSearchParams(params);
    next.set('offset', String(value));
    setParams(next);
  }
  return (
    <div className={embedded ? 'catalog-page embedded-catalog' : 'page catalog-page'}>
      {embedded && instance ? (
        <div className="catalog-instance-context">
          <ProjectIcon url={instance.iconRef} title={instance.name} kind="modpack" />
          <div>
            <h3>{instance.name}</h3>
            <p>
              <LockKeyhole size={14} /> Minecraft {instance.minecraftVersion} ·{' '}
              {kind === 'datapack' ? t('content.datapack') : loaderNames[instance.loader]}
            </p>
            <small>{t('content.contextLocked')}</small>
          </div>
        </div>
      ) : (
        <PageHeading
          eyebrow={t('content.source')}
          title={instance ? `${t('content.context')} «${instance.name}»` : t('content.title')}
          description={
            instance
              ? `${loaderNames[instance.loader]} · Minecraft ${instance.minecraftVersion}`
              : t('content.description')
          }
          action={
            instanceId ? (
              <Link className="button secondary" to="/catalog">
                {t('content.global')}
              </Link>
            ) : undefined
          }
        />
      )}
      {(!embedded ||
        (content.job?.instanceId === instanceId && content.job.phase !== 'completed')) && (
        <ContentActivity />
      )}
      {!desktop ? (
        <section className="surface">
          <EmptyState
            icon={Compass}
            title={t('content.title')}
            body={t('error.DESKTOP_REQUIRED')}
          />
        </section>
      ) : (
        <>
          {embedded && (
            <div className="content-kind-tabs" role="group" aria-label={t('content.kind')}>
              {['mod', 'resourcepack', 'shader', 'datapack'].map((value) => (
                <button
                  key={value}
                  className="button secondary"
                  aria-pressed={kind === value}
                  onClick={() => selectKind(value)}
                >
                  {t(('content.' + value) as MessageKey)}
                </button>
              ))}
            </div>
          )}
          <form
            key={serialized}
            className="surface catalog-filters"
            onSubmit={(event) => {
              event.preventDefault();
              const form = new FormData(event.currentTarget);
              const next = new URLSearchParams();
              next.set('kind', String(form.get('kind') ?? 'mod'));
              for (const [key, value] of form.entries())
                if (typeof value === 'string' && value) next.set(key, value);
              if (instanceId) next.set('instance', instanceId);
              setLoading(true);
              setParams(next);
              if (next.toString() === serialized) setAttempt(attempt + 1);
            }}
          >
            <div className="catalog-search">
              <Search size={19} />
              <input
                name="q"
                type="search"
                maxLength={160}
                defaultValue={params.get('q') ?? ''}
                placeholder={t('content.query')}
                aria-label={t('content.query')}
              />
              <button className="button primary" type="submit" disabled={loading}>
                {t('content.search')}
              </button>
            </div>
            <div className="catalog-filter-grid">
              {embedded ? (
                <input type="hidden" name="kind" value={kind} />
              ) : (
                <>
                  <label className="field">
                    <span>{t('content.kind')}</span>
                    <select name="kind" defaultValue={params.get('kind') ?? 'mod'}>
                      {[
                        '',
                        'mod',
                        'resourcepack',
                        'shader',
                        'datapack',
                        ...(!instanceId ? ['modpack'] : []),
                      ].map((value) => (
                        <option key={value} value={value}>
                          {t(value ? (`content.${value}` as MessageKey) : 'content.all')}
                        </option>
                      ))}
                    </select>
                  </label>
                  <label className="field">
                    <span>{t('content.minecraft')}</span>
                    <input
                      name="minecraft"
                      list="content-game-versions"
                      maxLength={80}
                      disabled={!!instanceId}
                      defaultValue={instance?.minecraftVersion ?? params.get('minecraft') ?? ''}
                      placeholder={t('content.all')}
                    />
                    <datalist id="content-game-versions">
                      {tags?.gameVersions.map((v) => (
                        <option key={v} value={v} />
                      ))}
                    </datalist>
                  </label>
                  <label className="field">
                    <span>{t('content.loader')}</span>
                    <select
                      name="loader"
                      disabled={!!instanceId || kind === 'datapack'}
                      defaultValue={
                        kind === 'datapack'
                          ? 'datapack'
                          : instance
                            ? instance.loader === 'neo_forge'
                              ? 'neoforge'
                              : instance.loader === 'vanilla'
                                ? 'minecraft'
                                : instance.loader
                            : (params.get('loader') ?? '')
                      }
                    >
                      <option value="">{t('content.all')}</option>
                      {(
                        tags?.loaders.map((l) => l.name) ?? [
                          'fabric',
                          'forge',
                          'neoforge',
                          'minecraft',
                          'iris',
                          'optifine',
                          'datapack',
                        ]
                      ).map((l) => (
                        <option key={l} value={l}>
                          {l}
                        </option>
                      ))}
                    </select>
                  </label>
                </>
              )}
              <label className="field">
                <span>{t('content.category')}</span>
                <select name="category" defaultValue={params.get('category') ?? ''}>
                  <option value="">{t('content.all')}</option>
                  {[
                    ...new Set(
                      tags?.categories
                        .filter(
                          (c) =>
                            !kind ||
                            c.project_type === kind ||
                            (kind === 'datapack' && c.project_type === 'mod'),
                        )
                        .map((c) => c.name),
                    ),
                  ]
                    .sort()
                    .map((c) => (
                      <option key={c} value={c}>
                        {c}
                      </option>
                    ))}
                </select>
              </label>
              {!embedded && (
                <label className="field">
                  <span>{t('content.environment')}</span>
                  <select
                    name="environment"
                    disabled={!!instanceId || kind === 'datapack'}
                    defaultValue={
                      kind === 'datapack'
                        ? ''
                        : instanceId
                          ? 'client'
                          : (params.get('environment') ?? '')
                    }
                  >
                    {['', 'client', 'both', 'server'].map((value) => (
                      <option key={value} value={value}>
                        {t(value ? (`content.${value}` as MessageKey) : 'content.all')}
                      </option>
                    ))}
                  </select>
                </label>
              )}
              <label className="field">
                <span>{t('content.sort')}</span>
                <select name="sort" defaultValue={params.get('sort') ?? 'relevance'}>
                  {['relevance', 'downloads', 'follows', 'updated', 'newest'].map((value) => (
                    <option key={value} value={value}>
                      {t(`content.${value}` as MessageKey)}
                    </option>
                  ))}
                </select>
              </label>
            </div>
          </form>
          {embedded && (
            <label className="content-hide-installed">
              <input
                type="checkbox"
                checked={hideInstalled}
                onChange={(event) => setHideInstalled(event.target.checked)}
              />
              {t('content.hideInstalled')}
            </label>
          )}
          {embedded && disabled && <p className="setting-hint">{t('content.instanceBusy')}</p>}
          {error && (
            <ErrorNotice
              error={error}
              action={
                <button className="button secondary" onClick={() => setAttempt(attempt + 1)}>
                  {t('error.retry')}
                </button>
              }
            />
          )}
          {loading ? (
            <p role="status">{t('content.loading')}</p>
          ) : (
            result && (
              <>
                {result.cached && <p className="setting-hint">{t('content.cached')}</p>}
                {hits.length === 0 ? (
                  <EmptyState
                    icon={Search}
                    title={t('content.empty')}
                    body={t(hideInstalled ? 'content.hiddenPageHint' : 'content.emptyHint')}
                  />
                ) : (
                  <div className="catalog-results">
                    {hits.map((hit) => (
                      <article className="surface catalog-result" key={hit.id}>
                        <ProjectIcon url={hit.iconUrl} title={hit.title} kind={hit.kind} />
                        <div className="catalog-result-main">
                          <h2>{hit.title}</h2>
                          <p>{hit.description}</p>
                          <div className="catalog-result-meta">
                            <span>{hit.author}</span>
                            <span>
                              {new Intl.NumberFormat(data.settings.values.locale, {
                                notation: 'compact',
                              }).format(hit.downloads)}{' '}
                              {t('content.downloads').toLowerCase()}
                            </span>
                            <span>{hit.categories.slice(0, 4).join(' · ')}</span>
                          </div>
                        </div>
                        <button
                          className={embedded ? 'button primary' : 'button secondary'}
                          disabled={
                            embedded &&
                            ((kind !== 'datapack' && installed.has(hit.id)) ||
                              disabled ||
                              content.busy)
                          }
                          onClick={() => setProject(hit.id)}
                        >
                          {embedded && installed.has(hit.id) && <Check size={16} />}
                          {t(
                            embedded
                              ? installed.has(hit.id)
                                ? kind === 'datapack'
                                  ? 'worlds.target'
                                  : 'content.alreadyPresent'
                                : 'content.addProject'
                              : 'content.details',
                          )}
                        </button>
                      </article>
                    ))}
                  </div>
                )}
                <div className="catalog-pagination">
                  <button
                    className="button secondary"
                    disabled={offset === 0}
                    onClick={() => paginate(Math.max(0, offset - 12))}
                  >
                    {t('content.previous')}
                  </button>
                  <span>{Math.floor(offset / 12) + 1}</span>
                  <button
                    className="button secondary"
                    disabled={result.nextOffset === null}
                    onClick={() => {
                      if (result.nextOffset !== null) paginate(result.nextOffset);
                    }}
                  >
                    {t('content.next')}
                  </button>
                </div>
              </>
            )
          )}
          {!embedded && <p className="setting-hint">{t('content.otherTypes')}</p>}
        </>
      )}
      {project && (
        <ProjectDialog
          projectId={project}
          projectKind={kind}
          instanceId={instanceId}
          stayInContext={embedded}
          disabled={disabled}
          onClose={() => setProject(null)}
        />
      )}
    </div>
  );
}
