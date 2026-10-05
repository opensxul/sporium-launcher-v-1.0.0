import { ProjectIcon } from './ProjectIcon';
import { useEffect, useRef, useState } from 'react';
import { LocalMods } from './LocalMods';
import { Search, Plus, Power, Trash2, History, RefreshCw } from 'lucide-react';
import { UntrackedFiles } from './UntrackedFiles';
import { ContentUpdates } from './ContentUpdates';
import { ContentRestore } from './ContentRestore';
import { ContentDiagnostics } from './ContentDiagnostics';
import { WorldContent } from './WorldContent';
import { LocalDependencies } from './LocalDependencies';
import { InstanceCatalogDialog } from './CatalogPage';
import type {
  InstalledContent as Record,
  ContentAction,
  ContentHistory,
  UntrackedContent,
  ContentSelection,
  ContentWorld,
} from '../bindings/core';
import { Modal } from '../components/Modal';
import { useFoundation } from '../app/context';
import { backend, normalizeError } from '../services/backend';
import type { BackendError } from '../services/backend';
import { ErrorNotice } from '../components/ui';
import { useContent } from './context';
import { ContentActivity } from './ContentActivity';
import type { MessageKey } from '../i18n/messages';

export function InstalledContent({
  id,
  disabled,
  section = '',
}: {
  id: string;
  disabled: boolean;
  section?: string;
}) {
  const { t, desktop, data } = useFoundation();
  const { job, busy } = useContent();
  const [records, setRecords] = useState<Record[]>([]);
  const [worlds, setWorlds] = useState<ContentWorld[]>([]);
  const [untracked, setUntracked] = useState<UntrackedContent[]>([]);
  const area = useRef<HTMLElement>(null);
  const [revision, setRevision] = useState(0);
  useEffect(() => {
    const changed = () => setRevision((value) => value + 1);
    window.addEventListener('sporium-project-changed', changed);
    return () => window.removeEventListener('sporium-project-changed', changed);
  }, []);
  const [error, setError] = useState<BackendError | null>(null);
  const [open, setOpen] = useState(false);
  const [diagnosticOpen, setDiagnosticOpen] = useState(false);
  const [restoreOpen, setRestoreOpen] = useState(false);
  const [dependencyFiles, setDependencyFiles] = useState<ContentSelection[] | null>(null);
  const [catalogQuery, setCatalogQuery] = useState('');
  const [updateSelection, setUpdateSelection] = useState<string[] | null | undefined>(undefined);
  const [query, setQuery] = useState('');
  const [selectedKind, setKind] = useState('');
  const kind = ['mod', 'resourcepack', 'shader', 'datapack'].includes(section)
    ? section
    : selectedKind;
  const [sort, setSort] = useState('name');
  const [selected, setSelected] = useState<string[]>([]);
  const [working, setWorking] = useState(false);
  const [history, setHistory] = useState<ContentHistory[]>([]);
  const [confirmation, setConfirmation] = useState<{
    action: ContentAction;
    records: Record[];
  } | null>(null);
  const locked = disabled || busy || working || !desktop;
  const key = (row: Record) => `${row.record.directory}/${row.record.file.filename}`;
  const visible = records
    .filter(
      ({ record }) =>
        (!kind ||
          (kind === 'datapack' && record.kind === 'datapack') ||
          (kind === 'mod' && record.directory === 'mods_disabled') ||
          record.directory ===
            { mod: 'mods', resourcepack: 'resourcepacks', shader: 'shaderpacks' }[kind]) &&
        `${record.title} ${record.file.filename}`
          .toLocaleLowerCase()
          .includes(query.trim().toLocaleLowerCase()),
    )
    .sort((a, b) =>
      sort === 'name'
        ? a.record.title.localeCompare(b.record.title, data.settings.values.locale)
        : b.record.version.date_published.localeCompare(a.record.version.date_published),
    );
  const phase = job?.instanceId === id ? job.phase : null;
  useEffect(() => {
    if (!desktop || disabled || busy) return;
    let active = true;
    void Promise.all([
      backend.installedContent(id),
      backend.contentHistory(id),
      backend.untrackedContent(id),
      backend.contentWorlds(id),
    ])
      .then(([value, events, external, destinations]) => {
        if (active) {
          setRecords(value);
          setWorlds(destinations);
          setUntracked(external);
          setHistory(events);
          setSelected((old) => old.filter((item) => value.some((row) => key(row) === item)));
          setError(null);
        }
      })
      .catch((reason) => {
        if (active) setError(normalizeError(reason));
      });
    return () => {
      active = false;
    };
  }, [id, desktop, disabled, busy, phase, revision]);
  function choose(action: ContentAction, rows: Record[]) {
    const eligible = rows.filter((row) =>
      action === 'delete'
        ? ['installed', 'disabled'].includes(row.status)
        : action === 'enable'
          ? row.status === 'disabled'
          : row.status === 'installed' && row.record.directory === 'mods',
    );
    if (eligible.length) setConfirmation({ action, records: eligible });
  }
  async function applyChange() {
    if (!confirmation) return;
    setWorking(true);
    setError(null);
    try {
      await backend.changeContent({
        instanceId: id,
        action: confirmation.action,
        files: confirmation.records.map(({ record }) => ({
          directory: record.directory,
          filename: record.file.filename,
          sha512: record.file.hashes.sha512,
        })),
      });
      setConfirmation(null);
      setSelected([]);
      const [value, events] = await Promise.all([
        backend.installedContent(id),
        backend.contentHistory(id),
      ]);
      setRecords(value);
      setHistory(events);
    } catch (reason) {
      setError(normalizeError(reason));
    } finally {
      setWorking(false);
    }
  }
  const selection = records.filter((row) => selected.includes(key(row)));
  const hasUntrackedMatches = untracked.some(
    (file) =>
      (!kind || file.kind === kind) &&
      `${file.title} ${file.filename}`
        .toLocaleLowerCase()
        .includes(query.trim().toLocaleLowerCase()),
  );
  return (
    <section className="surface content-installed" data-view={section} ref={area}>
      <div className="content-section-heading">
        <h2>{t('content.installed')}</h2>
        <button
          className="button secondary"
          disabled={locked}
          onClick={() => setDiagnosticOpen(true)}
        >
          {t('diagnostics.title')}
        </button>
        <button
          className="button secondary"
          disabled={locked || records.length === 0}
          onClick={() => setUpdateSelection(null)}
        >
          {t('updates.check')}
        </button>
        <button
          className="icon-button"
          aria-label={t('local.refresh')}
          disabled={locked}
          onClick={() => setRevision((value) => value + 1)}
        >
          <RefreshCw size={17} />
        </button>
        <button
          className="button primary"
          disabled={!desktop}
          onClick={() => {
            setCatalogQuery('');
            setOpen(true);
          }}
        >
          <Plus size={17} />
          {t('content.add')}
        </button>
      </div>
      <div className="content-kind-tabs" role="group" aria-label={t('content.kind')}>
        {['', 'mod', 'resourcepack', 'shader', 'datapack'].map((value) => (
          <button
            key={value}
            className="button secondary"
            aria-pressed={kind === value}
            onClick={() => setKind(value)}
          >
            {t(value ? (`content.${value}` as MessageKey) : 'content.all')}
          </button>
        ))}
      </div>
      {(!section || section === 'mod') && (!kind || kind === 'mod') && (
        <LocalMods
          key={id}
          id={id}
          area={area}
          disabled={locked || open || diagnosticOpen || !!confirmation}
          onImported={() => setRevision((value) => value + 1)}
        />
      )}
      {['', 'worlds', 'datapack'].includes(section) && (
        <WorldContent
          key={id}
          id={id}
          revision={revision}
          disabled={locked || open || diagnosticOpen || !!confirmation || !!dependencyFiles}
          onChanged={() => setRevision((value) => value + 1)}
        />
      )}
      <div className="instance-content-toolbar">
        <label className="instance-content-search">
          <Search size={17} />
          <input
            type="search"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder={t('content.searchInstalled')}
            aria-label={t('content.searchInstalled')}
          />
        </label>
        <select
          aria-label={t('content.sortInstalled')}
          value={sort}
          onChange={(event) => setSort(event.target.value)}
        >
          <option value="name">{t('content.nameSort')}</option>
          <option value="date">{t('content.versionDateSort')}</option>
        </select>
        <span className="setting-hint">
          {visible.length} / {records.length}
        </span>
      </div>
      {visible.length > 0 && (
        <div className="content-bulk-actions">
          <label>
            <input
              type="checkbox"
              disabled={locked}
              checked={visible.every((row) => selected.includes(key(row)))}
              onChange={(event) =>
                setSelected(
                  event.target.checked
                    ? [...new Set([...selected, ...visible.map(key)])]
                    : selected.filter((item) => !visible.some((row) => key(row) === item)),
                )
              }
            />{' '}
            {t('content.selectVisible')}
          </label>
          <span>
            {t('content.selected')}: {selection.length}
          </span>
          <button
            className="button secondary"
            disabled={locked || !selection.some((r) => r.record.provider === 'modrinth')}
            onClick={() => setUpdateSelection(selection.map((r) => r.record.projectId))}
          >
            {t('updates.selected')}
          </button>
          {(['enable', 'disable', 'delete'] as const).map((action) => (
            <button
              key={action}
              className="button secondary"
              disabled={
                locked ||
                !selection.some((row) =>
                  action === 'enable'
                    ? row.status === 'disabled'
                    : action === 'disable'
                      ? row.status === 'installed' && row.record.directory === 'mods'
                      : ['installed', 'disabled'].includes(row.status),
                )
              }
              onClick={() => choose(action, selection)}
            >
              {t(`content.action.${action}`)}
            </button>
          ))}
        </div>
      )}
      {job?.instanceId === id && job.phase !== 'completed' && <ContentActivity />}
      {error && <ErrorNotice error={error} />}
      {visible.length === 0 && !hasUntrackedMatches ? (
        <p className="setting-hint">
          {t(
            records.length === 0 && untracked.length === 0
              ? 'content.noInstalled'
              : 'content.noLocalMatches',
          )}
        </p>
      ) : (
        <ul className="content-records">
          {visible.map((row) => {
            const { record, status } = row;
            return (
              <li key={`${record.directory}/${record.file.filename}`}>
                <input
                  type="checkbox"
                  aria-label={`${t('content.select')}: ${record.title}`}
                  disabled={locked}
                  checked={selected.includes(key(row))}
                  onChange={(event) =>
                    setSelected(
                      event.target.checked
                        ? [...selected, key(row)]
                        : selected.filter((item) => item !== key(row)),
                    )
                  }
                />
                <ProjectIcon
                  url={record.iconUrl}
                  projectId={record.provider === 'modrinth' ? record.projectId : undefined}
                  title={record.title}
                  kind={record.kind}
                  instanceId={record.provider === 'local' ? id : undefined}
                  directory={record.directory}
                  filename={record.file.filename}
                  sha512={record.file.hashes.sha512}
                />
                <div>
                  <strong>{record.title}</strong> · {record.version.version_number}
                  {record.kind === 'datapack' && (
                    <small>
                      {t('worlds.target')}:{' '}
                      {worlds.find((item) => item.id === record.directory.split('/')[1])?.title ??
                        record.directory.split('/')[1]}
                    </small>
                  )}
                  <small>
                    {record.provider === 'local' ? t('local.source') : 'Modrinth'}
                    {record.dependency ? ` · ${t('content.required')}` : ''}
                  </small>
                  <details>
                    <summary>{t('content.fileDetails')}</summary>
                    <small>
                      {record.directory}/{record.file.filename}
                    </small>
                    {record.local && (
                      <>
                        <small>
                          {record.local.loader || t('local.unknown')} ·{' '}
                          {record.local.modIds.join(', ')}
                        </small>
                        {!!record.local.required.length && (
                          <small>
                            {t('local.required')}: {record.local.required.join(', ')}
                          </small>
                        )}
                        {record.local.warnings.map((code) => (
                          <small key={code}>{t(`local.warning.${code}` as MessageKey)}</small>
                        ))}
                      </>
                    )}
                  </details>
                </div>
                <span className={status === 'installed' ? 'setting-hint' : 'content-warning'}>
                  {t(`content.status.${status}` as MessageKey)}
                </span>
                {['mods', 'mods_disabled'].includes(record.directory) && (
                  <button
                    className="icon-button"
                    role="switch"
                    aria-checked={record.directory === 'mods'}
                    aria-label={`${t(status === 'disabled' ? 'content.action.enable' : 'content.action.disable')}: ${record.title}`}
                    disabled={locked || !['installed', 'disabled'].includes(status)}
                    onClick={() => choose(status === 'disabled' ? 'enable' : 'disable', [row])}
                  >
                    <Power size={18} />
                  </button>
                )}
                <button
                  className="icon-button"
                  aria-label={`${t('content.action.delete')}: ${record.title}`}
                  disabled={locked || !['installed', 'disabled'].includes(status)}
                  onClick={() => choose('delete', [row])}
                >
                  <Trash2 size={18} />
                </button>
                {record.directory === 'mods' && (
                  <button
                    className="button secondary"
                    disabled={locked || status !== 'installed'}
                    onClick={() =>
                      setDependencyFiles([
                        {
                          directory: record.directory,
                          filename: record.file.filename,
                          sha512: record.file.hashes.sha512,
                        },
                      ])
                    }
                  >
                    {t('dependencies.title')}
                  </button>
                )}
                {record.provider === 'modrinth' && (
                  <button
                    className="icon-button"
                    aria-label={`${t('updates.check')}: ${record.title}`}
                    disabled={locked || !['installed', 'disabled'].includes(status)}
                    onClick={() => setUpdateSelection([record.projectId])}
                  >
                    <RefreshCw size={18} />
                  </button>
                )}
                {record.provider === 'modrinth' && (
                  <button
                    className="button secondary"
                    onClick={() =>
                      void backend
                        .openContentProject(record.projectId)
                        .catch((reason) => setError(normalizeError(reason)))
                    }
                  >
                    Modrinth
                  </button>
                )}
              </li>
            );
          })}
        </ul>
      )}
      <UntrackedFiles
        id={id}
        files={untracked}
        query={query}
        kind={kind}
        disabled={locked}
        revision={revision}
        onAdopted={() => setRevision((value) => value + 1)}
      />
      <details className="content-history" open={section === 'history' ? true : undefined}>
        <summary>
          <History size={16} /> {t('content.history')}
        </summary>
        <button className="button secondary" disabled={locked} onClick={() => setRestoreOpen(true)}>
          {t('restore.title')}
        </button>
        {history.length === 0 ? (
          <p className="setting-hint">{t('content.noHistory')}</p>
        ) : (
          <ol>
            {[...history].reverse().map((event) => (
              <li key={event.id}>
                <time>{new Date(event.timestamp).toLocaleString(data.settings.values.locale)}</time>
                <strong>{t(`content.action.${event.action}` as MessageKey)}</strong>
                <span>{event.titles.join(', ')}</span>
              </li>
            ))}
          </ol>
        )}
      </details>
      {confirmation && (
        <Modal
          title={t(`content.action.${confirmation.action}`)}
          onClose={() => {
            setConfirmation(null);
            setError(null);
          }}
          busy={working}
        >
          <p>{t(confirmation.action === 'delete' ? 'content.deleteHint' : 'content.toggleHint')}</p>
          <ul>
            {confirmation.records.map((row) => (
              <li key={key(row)}>
                {row.record.title} · {row.record.version.version_number}
              </li>
            ))}
          </ul>
          {error && <ErrorNotice error={error} />}
          <div className="modal-actions">
            <button
              className="button secondary"
              disabled={working}
              onClick={() => {
                setConfirmation(null);
                setError(null);
              }}
            >
              {t('common.cancel')}
            </button>
            <button className="button primary" disabled={locked} onClick={() => void applyChange()}>
              {t('content.confirmChange')}
            </button>
          </div>
        </Modal>
      )}
      {open && (
        <InstanceCatalogDialog
          id={id}
          records={records}
          disabled={locked}
          initialKind={kind || 'mod'}
          initialQuery={catalogQuery}
          onClose={() => setOpen(false)}
        />
      )}
      {diagnosticOpen && (
        <ContentDiagnostics
          id={id}
          onClose={() => setDiagnosticOpen(false)}
          onSearch={(query) => {
            setDiagnosticOpen(false);
            setCatalogQuery(query);
            setKind('mod');
            setOpen(true);
          }}
        />
      )}
      {restoreOpen && (
        <ContentRestore
          id={id}
          onClose={() => setRestoreOpen(false)}
          onRestored={() => setRevision((value) => value + 1)}
        />
      )}
      {dependencyFiles && (
        <LocalDependencies
          id={id}
          files={dependencyFiles}
          onClose={() => setDependencyFiles(null)}
        />
      )}
      {updateSelection !== undefined && (
        <ContentUpdates
          id={id}
          records={records}
          initial={updateSelection}
          onClose={() => {
            setUpdateSelection(undefined);
            setRevision((v) => v + 1);
          }}
        />
      )}
    </section>
  );
}
