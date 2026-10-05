import { useCallback, useEffect, useRef, useState } from 'react';
import { BrandMark } from '../components/BrandMark';
import {
  Check,
  HardDrive,
  Info,
  LoaderCircle,
  Palette,
  Settings2,
  ShieldCheck,
  Sparkles,
} from 'lucide-react';
import { NavLink, useBeforeUnload, useBlocker, useParams } from 'react-router-dom';
import { useFoundation } from '../app/context';
import type { Locale, Settings } from '../bindings/core';
import { EmptyState, ErrorNotice, PageHeading, Toggle } from '../components/ui';
import { normalizeError } from '../services/backend';
import type { BackendError } from '../services/backend';
import { NotFoundPage } from './StaticPages';
import { JavaSettings, VersionSettings } from '../game/GameSettings';
import { ProfilesSettings } from '../game/Profiles';
import { AutomaticSettings } from '../projects/AutomaticSettings';
import { CacheSettings, DownloadSettings } from '../game/DownloadSettings';

const sections = [
  'accounts',
  'minecraft',
  'java',
  'catalog',
  'downloads',
  'storage',
  'appearance',
  'animations',
  'advanced',
  'about',
] as const;

export function SettingsPage() {
  const { section = 'appearance' } = useParams();
  const { data, desktop, t, saveSettings, reload } = useFoundation();
  const [draft, setDraft] = useState<Settings>(data.settings.values);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<BackendError | null>(null);
  const dirty =
    section !== 'accounts' &&
    JSON.stringify({
      ...draft,
      localNickname: data.settings.values.localNickname,
      nicknameSkins: data.settings.values.nicknameSkins,
    }) !== JSON.stringify(data.settings.values);
  const blocker = useBlocker(dirty && !saving);
  const dialogRef = useRef<HTMLDialogElement>(null);
  useBeforeUnload(
    useCallback(
      (event) => {
        if (dirty) {
          event.preventDefault();
          event.returnValue = '';
        }
      },
      [dirty],
    ),
  );

  useEffect(() => {
    const dialog = dialogRef.current;
    if (blocker.state === 'blocked' && !dialog?.open) dialog?.showModal();
    else if (blocker.state !== 'blocked') dialog?.close();
  }, [blocker.state]);

  function update(patch: Partial<Settings>) {
    setDraft({ ...draft, ...patch });
    setSaved(false);
    setError(null);
  }

  async function save(leave = false) {
    setSaving(true);
    setError(null);
    try {
      const result = await saveSettings({
        ...draft,
        localNickname: data.settings.values.localNickname,
        nicknameSkins: data.settings.values.nicknameSkins,
      });
      setDraft(result.values);
      setSaved(true);
      if (leave && blocker.state === 'blocked') blocker.proceed();
    } catch (reason) {
      setError(normalizeError(reason));
    } finally {
      setSaving(false);
    }
  }

  async function loadSaved() {
    setSaving(true);
    try {
      const current = await reload();
      setDraft(current.values);
      setError(null);
      setSaved(false);
    } catch (reason) {
      setError(normalizeError(reason));
    } finally {
      setSaving(false);
    }
  }

  if (!sections.some((value) => value === section)) return <NotFoundPage />;
  const currentSection = section as (typeof sections)[number];

  return (
    <div className="page settings-page">
      <PageHeading title={t('settings.title')} description={t('settings.description')} />
      <div className="settings-layout">
        <nav className="settings-navigation" aria-label={t('settings.nav')}>
          {sections.map((value) => (
            <NavLink key={value} to={`/settings/${value}`}>
              {t(`settings.${value}`)}
            </NavLink>
          ))}
        </nav>
        <section className="settings-content surface">
          <div className="settings-content-header">
            <h2>{t(`settings.${currentSection}`)}</h2>
          </div>
          {currentSection === 'appearance' && (
            <>
              <div className="theme-block">
                <span className="setting-label">{t('settings.theme')}</span>
                <div className="theme-preview" aria-hidden="true">
                  <div className="mini-sidebar">
                    <i />
                    <i />
                    <i />
                  </div>
                  <div className="mini-content">
                    <span />
                    <div>
                      <i />
                      <i />
                      <i />
                    </div>
                  </div>
                </div>
                <div className="theme-title">
                  <span>{t('settings.themeName')}</span>
                  <span className="selected-label">
                    <Check size={13} />
                    {t('settings.selected')}
                  </span>
                </div>
                <p className="setting-hint">{t('settings.themeHint')}</p>
                <p className="setting-hint subdued">{t('settings.themeFuture')}</p>
              </div>
              <label className="setting-row">
                <span>
                  <span className="setting-label">{t('settings.language')}</span>
                  <span className="setting-hint">{t('settings.languageHint')}</span>
                </span>
                <select
                  aria-label={t('settings.language')}
                  value={draft.locale}
                  onChange={(event) => update({ locale: event.target.value as Locale })}
                  disabled={!desktop || saving}
                >
                  <option value="ru-RU">Русский</option>
                  <option value="en-US">English</option>
                </select>
              </label>
              <label className="setting-row">
                <span>
                  <span className="setting-label">{t('settings.scale')}</span>
                  <span className="setting-hint">{t('settings.scaleHint')}</span>
                </span>
                <select
                  aria-label={t('settings.scale')}
                  value={draft.uiScale}
                  onChange={(event) => update({ uiScale: Number(event.target.value) })}
                  disabled={!desktop || saving}
                >
                  {[90, 100, 110, 125].map((scale) => (
                    <option key={scale} value={scale}>
                      {scale}%
                    </option>
                  ))}
                </select>
              </label>
            </>
          )}
          {currentSection === 'animations' && (
            <>
              <div className="settings-intro">
                <Sparkles size={26} />
                <p>{t('settings.motionSystem')}</p>
              </div>
              <Toggle
                checked={draft.motion === 'reduced'}
                onChange={(checked) => update({ motion: checked ? 'reduced' : 'system' })}
                label={t('settings.motion')}
                description={t('settings.motionHint')}
                disabled={!desktop || saving}
              />
            </>
          )}
          {currentSection === 'minecraft' && (
            <VersionSettings
              value={draft.versionVisibility}
              onChange={(versionVisibility) => update({ versionVisibility })}
              disabled={!desktop || saving}
            />
          )}
          {currentSection === 'java' && (
            <JavaSettings draft={draft} update={update} disabled={!desktop || saving} />
          )}
          {currentSection === 'storage' && (
            <>
              <div className="settings-detail">
                <HardDrive size={27} />
                <h3>{t('settings.storagePath')}</h3>
                <p>{t('settings.storageHint')}</p>
                <code className="data-path">
                  {data.info.dataDirectory || t('settings.previewPath')}
                </code>
              </div>
              <CacheSettings draft={draft} update={update} disabled={!desktop || saving} />
            </>
          )}
          {currentSection === 'downloads' && (
            <DownloadSettings draft={draft} update={update} disabled={!desktop || saving} />
          )}
          {currentSection === 'about' && (
            <div className="settings-detail">
              <BrandMark />
              <h3>Sporium</h3>
              <p>{t('settings.aboutBody')}</p>
              <div className="version-line">
                <Info size={16} />
                <span>{t('settings.version')}</span>
                <strong>{data.info.version}</strong>
              </div>
              <p className="privacy-line">
                <ShieldCheck size={17} />
                {t('settings.privacy')}
              </p>
            </div>
          )}
          {currentSection === 'accounts' && <ProfilesSettings />}
          {currentSection === 'catalog' && (
            <div className="settings-detail">
              <h3>{t('content.title')}</h3>
              <p>{t('content.description')}</p>
              <NavLink className="button secondary" to="/catalog">
                {t('nav.catalog')}
              </NavLink>
              <p className="setting-hint">{t('content.otherTypes')}</p>
              <AutomaticSettings />
            </div>
          )}
          {![
            'appearance',
            'animations',
            'storage',
            'downloads',
            'about',
            'accounts',
            'minecraft',
            'java',
            'catalog',
          ].includes(currentSection) && (
            <EmptyState
              icon={currentSection === 'advanced' ? Settings2 : Palette}
              title={t('settings.futureTitle')}
              body={t('settings.futureBody')}
            />
          )}
          {error && (
            <ErrorNotice
              error={error}
              action={
                error.code === 'SETTINGS_CONFLICT' && (
                  <button
                    className="button secondary"
                    onClick={() => void loadSaved()}
                    disabled={saving}
                  >
                    {t('settings.reload')}
                  </button>
                )
              }
            />
          )}
          {['appearance', 'animations', 'minecraft', 'java', 'downloads', 'storage'].includes(
            currentSection,
          ) && (
            <div className="settings-savebar">
              <span aria-live="polite" className={saved ? 'saved-status' : 'save-status'}>
                {saved ? (
                  <>
                    <Check size={15} />
                    {t('settings.saved')}
                  </>
                ) : dirty ? (
                  t('settings.unsaved')
                ) : (
                  ''
                )}
              </span>
              <div>
                {dirty && (
                  <button
                    className="button text-button"
                    disabled={saving}
                    onClick={() => {
                      setDraft(data.settings.values);
                      setError(null);
                      setSaved(false);
                    }}
                  >
                    {t('settings.reset')}
                  </button>
                )}
                <button
                  className="button primary"
                  disabled={
                    !desktop ||
                    !dirty ||
                    saving ||
                    !/^[A-Za-z0-9_]{1,16}$/.test(draft.localNickname)
                  }
                  onClick={() => void save()}
                >
                  {saving ? <LoaderCircle className="spin" size={16} /> : <Check size={16} />}
                  {t(saving ? 'settings.saving' : 'settings.save')}
                </button>
              </div>
            </div>
          )}
        </section>
      </div>
      <dialog
        ref={dialogRef}
        className="modal"
        aria-labelledby="unsaved-title"
        onCancel={(event) => {
          event.preventDefault();
          if (blocker.state === 'blocked') blocker.reset();
        }}
      >
        <h2 id="unsaved-title">{t('settings.leaveTitle')}</h2>
        <p>{t('settings.leaveBody')}</p>
        {error && <ErrorNotice error={error} />}
        <div className="modal-actions">
          <button
            className="button text-button"
            disabled={saving}
            onClick={() => {
              if (blocker.state === 'blocked') blocker.reset();
            }}
          >
            {t('settings.stay')}
          </button>
          <button
            className="button secondary"
            disabled={saving}
            onClick={() => {
              setDraft(data.settings.values);
              if (blocker.state === 'blocked') blocker.proceed();
            }}
          >
            {t('settings.discard')}
          </button>
          <button className="button primary" disabled={saving} onClick={() => void save(true)}>
            {t('settings.save')}
          </button>
        </div>
      </dialog>
    </div>
  );
}
