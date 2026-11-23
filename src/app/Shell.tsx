import { NavLink, Outlet, useLocation, useNavigate, useSearchParams } from 'react-router-dom';
import { PixelIcon } from '../components/PixelIcon';
import { WorldProvider } from '../world/WorldProvider';
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
  { to: '/', key: 'nav.home', icon: 'home' },
  { to: '/news', key: 'nav.news', icon: 'news' },
  { to: '/world', key: 'nav.world', icon: 'world' },
  { to: '/library', key: 'nav.library', icon: 'library' },
  { to: '/catalog', key: 'nav.catalog', icon: 'catalog' },
  { to: '/instances', key: 'nav.instances', icon: 'instances' },
  { to: '/folders', key: 'nav.folders', icon: 'folders' },
  { to: '/downloads', key: 'nav.downloads', icon: 'downloads' },
  { to: '/settings', key: 'nav.settings', icon: 'settings' },
] as const;

export function Shell() {
  const { t, desktop, data } = useFoundation();
  const { state: gameState } = useGame();
  const { job: contentJob } = useContent();
  const { pathname } = useLocation();
  const navigate = useNavigate();
  const [params] = useSearchParams();
  const searchRoute =
    ['/library', '/catalog'].includes(pathname) || pathname.startsWith('/instances')
      ? pathname
      : '/instances';
  const searchValue = params.get('q') ?? '';
  const searchLabel = searchRoute === '/catalog' ? t('content.search') : t('home.search');
  const nickname =
    data.profiles.profiles.find((p) => p.id === data.profiles.activeProfileId)?.nickname ??
    data.settings.values.localNickname;
  const current: MessageKey = pathname.startsWith('/instance/')
    ? 'nav.instances'
    : pathname === '/profile'
      ? 'nav.profile'
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
        <NavLink to="/" className="brand" aria-label="Sporych Launcher">
          <img
            className="sidebar-mark"
            src="/branding/scenery/fly-agaric.webp"
            width={64}
            height={64}
            alt=""
          />
          <span className="brand-word">
            SPORYCH<span className="brand-sub">{t('app.brandCaption')}</span>
          </span>
        </NavLink>

        <nav aria-label={t('app.nav')}>
          {navigation.map(({ to, key, icon }) => (
            <NavLink
              key={to}
              end={to === '/'}
              to={to}
              className={({ isActive }) =>
                `nav-item ${isActive || (to === '/instances' && pathname.startsWith('/instance/')) ? 'active' : ''}`
              }
              title={t(key)}
            >
              <PixelIcon name={icon} colored />
              <span>{t(key)}</span>
            </NavLink>
          ))}
        </nav>
        <div className="sidebar-bottom">
          <div className="build-label">
            <span>v{data.info.version}</span>
          </div>
          <NavLink
            to="/profile"
            className="account-widget"
            title={t('nav.profile')}
            aria-label={t('nav.profile')}
          >
            <span className="avatar">
              <Skin key={nickname} nickname={nickname} />
            </span>
            <span className="profile-label">
              <b>{nickname}</b>
              <small>{t('nav.profile')}</small>
            </span>
            <PixelIcon name="arrow" />
          </NavLink>
        </div>
      </aside>
      <div className="main-shell">
        <header className="topbar">
          <div className="breadcrumb">
            <span>Sporych</span>
            <PixelIcon name="arrow" />
            <span>{t(current)}</span>
          </div>
          <form
            key={pathname + searchValue}
            role="search"
            className="global-search search-bar"
            onSubmit={(event) => {
              event.preventDefault();
              const value = String(new FormData(event.currentTarget).get('q') ?? '').trim();
              const next =
                searchRoute === pathname ? new URLSearchParams(params) : new URLSearchParams();
              if (value) next.set('q', value);
              else next.delete('q');
              next.delete('offset');
              void navigate({ pathname: searchRoute, search: next.toString() });
            }}
          >
            <input
              defaultValue={searchValue}
              type="search"
              name="q"
              placeholder={searchLabel}
              aria-label={searchLabel}
              maxLength={120}
            />
            <button
              className="icon-button"
              type="submit"
              title={searchLabel}
              aria-label={searchLabel}
            >
              <PixelIcon name="search" />
            </button>
          </form>
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
          <WorldProvider>
            <Outlet />
          </WorldProvider>
        </main>
        {(pathname !== '/' || contentActive(contentJob) || jobActive(gameState)) && (
          <footer className="download-dock">
            <div className="dock-icon">
              <PixelIcon name="downloads" />
            </div>
            <div>
              <span>
                {contentJob && contentActive(contentJob)
                  ? t(`content.phase.${contentJob.phase}` as MessageKey)
                  : gameState.job && jobActive(gameState)
                    ? t(
                        gameState.job.paused
                          ? 'download.pausedStatus'
                          : `job.${gameState.job.phase}`,
                      )
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
              <PixelIcon name="arrow" />
            </NavLink>
          </footer>
        )}
      </div>
    </div>
  );
}
