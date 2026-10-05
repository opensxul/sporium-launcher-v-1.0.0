import {
  BookOpen,
  ChevronRight,
  Compass,
  Download,
  Folder,
  Home,
  Leaf,
  Settings,
} from 'lucide-react';
import { NavLink, Outlet, useLocation } from 'react-router-dom';
import { BrandMark } from '../components/BrandMark';
import { AppUpdates } from './AppUpdates';
import { BackupNotice } from '../library/LibraryStatus';
import { GameAlerts } from '../game/GameActivity';
import { useGame, jobActive } from '../game/context';
import { useFoundation } from './context';
import type { MessageKey } from '../i18n/messages';
import { Skin } from '../game/Profiles';
import { useContent, contentActive } from '../content/context';
import { PackImports } from '../packs/PackImports';
import '../styles/packs.css';

const navigation = [
  { to: '/', key: 'nav.home', icon: Home },
  { to: '/catalog', key: 'nav.catalog', icon: Compass },
  { to: '/instances', key: 'nav.instances', icon: BookOpen },
  { to: '/folders', key: 'nav.folders', icon: Folder },
  { to: '/downloads', key: 'nav.downloads', icon: Download },
  { to: '/settings', key: 'nav.settings', icon: Settings },
] as const;

export function Shell() {
  const { t, desktop, data } = useFoundation();
  const { state: gameState } = useGame();
  const { job: contentJob } = useContent();
  const { pathname } = useLocation();
  const nickname =
    data.profiles.profiles.find((p) => p.id === data.profiles.activeProfileId)?.nickname ??
    data.settings.values.localNickname;
  const current: MessageKey = pathname.startsWith('/instance/')
    ? 'nav.instances'
    : (navigation.find((item) => item.to !== '/' && pathname.startsWith(item.to))?.key ??
      'nav.home');
  return (
    <div className="app-shell">
      <AppUpdates />
      <a
        className="skip-link"
        href="#main-content"
        onClick={(event) => {
          event.preventDefault();
          document.getElementById('main-content')?.focus();
        }}
      >
        {t('app.skip')}
      </a>
      <aside className="sidebar">
        <NavLink to="/" className="brand" aria-label="Sporium">
          <BrandMark />
          <span className="brand-word">
            sporium<span className="brand-sub">{t('app.brandCaption')}</span>
          </span>
        </NavLink>
        <div className="nav-caption">{t('app.library')}</div>
        <nav aria-label={t('app.nav')}>
          {navigation.map(({ to, key, icon: Icon }) => (
            <NavLink
              key={to}
              end={to === '/'}
              to={to}
              className={({ isActive }) =>
                `nav-item ${isActive || (to === '/instances' && pathname.startsWith('/instance/')) ? 'active' : ''}`
              }
              title={t(key)}
            >
              <Icon size={19} strokeWidth={1.7} />
              <span>{t(key)}</span>
            </NavLink>
          ))}
        </nav>
        <div className="sidebar-bottom">
          <div className="sidebar-decoration" aria-hidden="true">
            <Leaf size={88} strokeWidth={0.5} />
            <span />
          </div>
          <div className="build-label">
            <span className="status-dot" />
            <span>Sporium {data.info.version}</span>
          </div>
          <span className="early-label">{t('app.early')}</span>
        </div>
      </aside>
      <div className="main-shell">
        <header className="topbar">
          <div className="breadcrumb">
            <span>Sporium</span>
            <ChevronRight size={13} />
            <span>{t(current)}</span>
          </div>
          <NavLink to="/settings/accounts" className="account-widget" title={t('account.guest')}>
            <span className="avatar">
              <Skin key={nickname} nickname={nickname} />
            </span>
            <span>{nickname}</span>
            <ChevronRight size={14} />
          </NavLink>
        </header>
        {!desktop && (
          <div className="preview-notice" role="note">
            {t('app.preview')}
          </div>
        )}
        <main id="main-content" tabIndex={-1}>
          <BackupNotice />
          <GameAlerts />
          <PackImports />
          <Outlet />
        </main>
        <footer className="download-dock">
          <div className="dock-icon">
            <Download size={17} />
          </div>
          <div>
            <span>
              {contentJob && contentActive(contentJob)
                ? t(`content.phase.${contentJob.phase}` as MessageKey)
                : gameState.job && jobActive(gameState)
                  ? t(gameState.job.paused ? 'download.pausedStatus' : `job.${gameState.job.phase}`)
                  : t('downloads.queue')}
            </span>
            <small>
              {contentJob && contentActive(contentJob)
                ? `${contentJob.completedFiles} / ${contentJob.totalFiles}`
                : gameState.job && jobActive(gameState)
                  ? `${gameState.job.completedFiles} / ${gameState.job.totalFiles || '…'}`
                  : t('downloads.footer')}
            </small>
          </div>
          <NavLink to="/downloads" className="icon-button" aria-label={t('nav.downloads')}>
            <ChevronRight size={19} />
          </NavLink>
        </footer>
      </div>
    </div>
  );
}
