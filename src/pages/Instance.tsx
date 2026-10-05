import { InstanceIcon } from '../library/InstanceIcon';
import { InstanceAppearance } from '../library/InstanceAppearance';
import { InstanceMetrics } from '../library/InstanceMetrics';
import { useState } from 'react';
import { ArrowLeft, Copy, Pencil, Trash2 } from 'lucide-react';
import { Link, useParams, useSearchParams } from 'react-router-dom';
import { useFoundation } from '../app/context';
import { useLibrary } from '../library/context';
import { LibraryStatus } from '../library/LibraryStatus';
import { InstanceEditor, DeleteInstanceDialog } from '../library/InstanceForms';
import { OpenFolderButton } from '../library/OpenFolderButton';
import { FavoriteButton } from '../library/FavoriteButton';
import { loaderNames } from '../library/view-model';
import { PageHeading } from '../components/ui';
import { NotFoundPage } from './StaticPages';
import { InstanceGame } from '../game/InstanceGame';
import { InstanceLaunchSettings } from '../game/InstanceLaunchSettings';
import { useGame, jobActive } from '../game/context';
import { InstalledContent } from '../content/InstalledContent';
import { ProjectPanel } from '../projects/ProjectPanel';
import { useContent } from '../content/context';
import { ExportPackButton } from '../packs/PackExport';

export function InstancePage() {
  const { id } = useParams();
  const [search, setSearch] = useSearchParams();
  const tabs = [
    'overview',
    'mod',
    'resourcepack',
    'shader',
    'worlds',
    'datapack',
    'settings',
    'history',
    'logs',
  ] as const;
  const tab = tabs.find((value) => value === search.get('tab')) ?? 'overview';
  const { t, data } = useFoundation();
  const { snapshot, busy, loading, error } = useLibrary();
  const { state: gameState } = useGame();
  const content = useContent();
  const filesBusy =
    (content.busy && content.job?.instanceId === id) ||
    gameState.sessions.some((session) => session.instanceId === id && session.running) ||
    (gameState.job?.instanceId === id && jobActive(gameState));
  const [dialog, setDialog] = useState<'edit' | 'duplicate' | 'delete' | null>(null);
  const instance = snapshot.instances.find((item) => item.id === id);
  // A background snapshot refresh must not unmount the open content browser.
  if ((loading || error) && !instance)
    return (
      <div className="page">
        <LibraryStatus />
      </div>
    );
  if (!instance) return <NotFoundPage />;
  const collection = snapshot.collections.find((item) => item.id === instance.collectionId);
  return (
    <div className="page instance-page">
      {error && <LibraryStatus />}
      <Link className="back-link" to="/instances">
        <ArrowLeft size={16} />
        {t('library.all')}
      </Link>
      <div className="instance-heading">
        <div className="instance-title-icon">
          <InstanceIcon instance={instance} />
        </div>
        <PageHeading
          title={instance.name}
          description={`${loaderNames[instance.loader]} · ${instance.minecraftVersion}`}
          action={<FavoriteButton instance={instance} />}
        />
      </div>
      <InstanceGame instance={instance} />
      <div
        className="instance-tabs"
        role="tablist"
        aria-label={t('ui.tabs')}
        onKeyDown={(event) => {
          const offset = event.key === 'ArrowRight' ? 1 : event.key === 'ArrowLeft' ? -1 : 0;
          if (!offset && !['Home', 'End'].includes(event.key)) return;
          event.preventDefault();
          const next =
            event.key === 'Home'
              ? 0
              : event.key === 'End'
                ? tabs.length - 1
                : (tabs.indexOf(tab) + offset + tabs.length) % tabs.length;
          setSearch({ tab: tabs[next] ?? 'overview' }, { replace: true });
          (event.currentTarget.children[next] as HTMLButtonElement).focus();
        }}
      >
        {tabs.map((value) => (
          <button
            key={value}
            id={`tab-${value}`}
            role="tab"
            aria-selected={tab === value}
            aria-controls="instance-panel"
            tabIndex={tab === value ? 0 : -1}
            onClick={() => setSearch({ tab: value }, { replace: true })}
          >
            {t(`ui.${value}`)}
          </button>
        ))}
      </div>
      <div id="instance-panel" role="tabpanel" aria-labelledby={`tab-${tab}`}>
        {!['overview', 'settings', 'logs'].includes(tab) && (
          <InstalledContent
            key={`${instance.id}-${tab}`}
            id={instance.id}
            disabled={filesBusy}
            section={tab}
          />
        )}
        {tab === 'settings' && (
          <>
            <InstanceAppearance instance={instance} />
            <ProjectPanel key={`project-${instance.id}`} id={instance.id} disabled={filesBusy} />
            <InstanceLaunchSettings
              key={`${instance.id}-${instance.loaderVersion ?? ''}`}
              instance={instance}
              disabled={filesBusy}
            />
            <section className="surface appearance-panel">
              <h2>{t('ui.quickLaunch')}</h2>
              <button className="button secondary" disabled aria-describedby="shortcut-hint">
                {t('ui.shortcut')}
              </button>
              <p className="setting-hint" id="shortcut-hint">
                {t('ui.shortcutLater')}
              </p>
            </section>
          </>
        )}
        {tab === 'logs' && (
          <section className="surface instance-overview">
            <h2>{t('ui.logs')}</h2>
            <p>{t(instance.lastPlayedAt ? 'ui.logsHint' : 'ui.noLogs')}</p>
            <OpenFolderButton target="logs" id={id} label={t('ui.logFolder')} />
            {gameState.sessions
              .filter((session) => session.instanceId === id)
              .map((session) => (
                <details key={session.instanceId}>
                  <summary>{t('ui.details')}</summary>
                  <code>{session.logPath}</code>
                </details>
              ))}
          </section>
        )}
        {tab === 'overview' && (
          <>
            <div className="instance-actions">
              <ExportPackButton id={instance.id} disabled={busy || filesBusy} />
              <button
                className="button secondary"
                disabled={busy}
                onClick={() => setDialog('edit')}
              >
                <Pencil size={16} />
                {t('instance.edit')}
              </button>
              <button
                className="button secondary"
                disabled={busy || filesBusy}
                onClick={() => setDialog('duplicate')}
              >
                <Copy size={16} />
                {t('instance.duplicate')}
              </button>
              <OpenFolderButton target="instance" id={id} label={t('common.openFolder')} />
              <OpenFolderButton target="mods" id={id} label={t('instance.openMods')} />
              <button
                className="button secondary danger-text"
                disabled={busy || filesBusy}
                onClick={() => setDialog('delete')}
              >
                <Trash2 size={16} />
                {t('common.delete')}
              </button>
            </div>
            <section className="surface instance-overview">
              <h2>{t('instance.overview')}</h2>
              <p>{t('instance.isolation')}</p>
              <InstanceMetrics key={instance.id} id={instance.id} busy={!!filesBusy} />
              <dl>
                <div>
                  <dt>{t('ui.lastPlayed')}</dt>
                  <dd>
                    {instance.lastPlayedAt
                      ? new Date(instance.lastPlayedAt).toLocaleString(data.settings.values.locale)
                      : t('ui.never')}
                  </dd>
                </div>
                <div>
                  <dt>{t('instance.requestedVersion')}</dt>
                  <dd>{instance.minecraftVersion}</dd>
                </div>
                <div>
                  <dt>{t('instance.loader')}</dt>
                  <dd>{loaderNames[instance.loader]}</dd>
                </div>
                <div>
                  <dt>{t('instance.collection')}</dt>
                  <dd>
                    {collection ? (
                      <Link to={`/folders/${collection.id}`}>{collection.name}</Link>
                    ) : (
                      t('instance.noCollection')
                    )}
                  </dd>
                </div>
                <div>
                  <dt>{t('instance.java')}</dt>
                  <dd>
                    {data.settings.values.customJavaPath ? (
                      <Link to="/settings/java">{t('java.custom')}</Link>
                    ) : (
                      t('instance.auto')
                    )}
                  </dd>
                </div>
                <div>
                  <dt>{t('instance.memory')}</dt>
                  <dd>{t('instance.auto')}</dd>
                </div>
                <div>
                  <dt>{t('instance.created')}</dt>
                  <dd>
                    {new Date(instance.createdAt).toLocaleDateString(data.settings.values.locale)}
                  </dd>
                </div>
              </dl>
              <details>
                <summary>{t('ui.details')}</summary>
                <code className="instance-id">{instance.id}</code>
              </details>
            </section>
          </>
        )}
      </div>
      {dialog === 'delete' ? (
        <DeleteInstanceDialog instance={instance} onClose={() => setDialog(null)} />
      ) : (
        dialog && (
          <InstanceEditor mode={dialog} instance={instance} onClose={() => setDialog(null)} />
        )
      )}
    </div>
  );
}
