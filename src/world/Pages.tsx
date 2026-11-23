import { useEffect, useRef, useState, type CSSProperties } from 'react';
import { Link, useNavigate } from 'react-router-dom';
import { invoke } from '@tauri-apps/api/core';
import {
  ArrowRight,
  ArrowUpRight,
  Box,
  ChevronRight,
  Image as ImageIcon,
  Newspaper,
  Radio,
  RefreshCw,
  Signal,
  Users,
} from 'lucide-react';
import { useFoundation } from '../app/context';
import distribution from '../../config/sporium.json';
import { useWorld, type Article, type Photo } from './context';
import { useGame, jobActive } from '../game/context';
import { useLibrary } from '../library/context';
import { useContent } from '../content/context';
import { ProfilesSettings, Skin } from '../game/Profiles';
import { backend, normalizeError, BackendError } from '../services/backend';
import { ErrorNotice } from '../components/ui';
import { Modal } from '../components/Modal';
import { PixelIcon } from '../components/PixelIcon';
import { InstanceCard } from '../library/InstanceCard';
import { GameActivity } from '../game/GameActivity';

function useWords() {
  const { data } = useFoundation();
  return (ru: string, en: string) => (data.settings.values.locale === 'ru-RU' ? ru : en);
}
function WorldImage({
  src,
  title,
  small = false,
}: {
  src: string;
  title: string;
  small?: boolean;
}) {
  const [uri, setUri] = useState('');
  const ref = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    let active = true;
    const target = ref.current;
    if (!target) return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (!entries[0]?.isIntersecting) return;
        observer.disconnect();
        void invoke<string>('world_image', { path: src, small })
          .then((value) => {
            if (active) setUri(value);
          })
          .catch(() => {});
      },
      { rootMargin: '100px' },
    );
    observer.observe(target);
    return () => {
      active = false;
      observer.disconnect();
    };
  }, [src, small]);
  return (
    <span className="feed-image" ref={ref}>
      {uri ? <img src={uri} alt={title} /> : <ImageIcon aria-label={title} />}
    </span>
  );
}
function ServerPanel() {
  const { status, statusState, refreshStatus } = useWorld();
  const { data } = useFoundation();
  const w = useWords();
  const label =
    statusState === 'unset'
      ? w('Адрес не задан', 'Address not set')
      : statusState === 'loading'
        ? w('Проверка…', 'Checking…')
        : statusState === 'error'
          ? w('Не удалось проверить', 'Could not check')
          : status?.online
            ? w('Онлайн', 'Online')
            : w('Оффлайн', 'Offline');
  return (
    <aside className="server-panel">
      <div className="panel-kicker">
        <Radio size={15} />
        {w('Статус сервера', 'Server status')}
      </div>
      <h2>Sporium Survival</h2>
      <span
        role="status"
        className={'live-state ' + (statusState === 'ready' && status?.online ? 'online' : '')}
      >
        <i />
        {label}
      </span>
      <dl>
        <div>
          <dt>
            <Users />
            {w('Игроки', 'Players')}
          </dt>
          <dd>
            {status?.online && status.players !== null ? status.players : '—'}
            {status?.online && status.maximum !== null ? ' / ' + status.maximum : ''}
          </dd>
        </div>
        <div>
          <dt>
            <Box />
            {w('Версия', 'Version')}
          </dt>
          <dd>{status?.version || data.settings.values.world.version || '—'}</dd>
        </div>
        <div>
          <dt>
            <Signal />
            {w('Задержка', 'Latency')}
          </dt>
          <dd>{status?.latencyMs != null ? status.latencyMs + ' ' + w('мс', 'ms') : '—'}</dd>
        </div>
      </dl>
      {statusState === 'unset' ? (
        <Link className="server-link" to="/settings/minecraft">
          {w('Указать адрес', 'Set address')}
          <ArrowUpRight size={15} />
        </Link>
      ) : (
        <button
          className="server-link"
          onClick={refreshStatus}
          disabled={statusState === 'loading'}
        >
          <RefreshCw size={14} />
          {w('Проверить', 'Refresh')}
        </button>
      )}
    </aside>
  );
}

function LaunchBar() {
  const { data, desktop, t } = useFoundation();
  const { status } = useWorld();
  const library = useLibrary();
  const game = useGame();
  const content = useContent();
  const navigate = useNavigate();
  const [error, setError] = useState<BackendError | null>(null);
  const [pending, setPending] = useState(false);
  const guard = useRef(false);
  const w = useWords();
  const cfg = data.settings.values.world;
  const version =
    cfg.version ||
    status?.version?.match(/^(?:Paper\s+)?(\d+\.\d+(?:\.\d+)?)(?:\s|$)/i)?.[1] ||
    (cfg.address === distribution.address ? distribution.lastKnownVersion : '');
  const eligible = library.snapshot.instances.filter(
    (i) => i.loader === 'vanilla' && (!version || i.minecraftVersion === version),
  );
  const chosen =
    eligible.find((i) => i.name === 'Sporium Survival') ||
    eligible.find((i) => i.favorite) ||
    eligible[0];
  const nickname =
    data.profiles.profiles.find((p) => p.id === data.profiles.activeProfileId)?.nickname ??
    data.settings.values.localNickname;
  const job = game.state.job;
  const working = jobActive(game.state);
  const session = game.state.sessions.find((s) => s.instanceId === chosen?.id);
  const running = session?.running;
  async function play() {
    if (guard.current) return;
    if ((!chosen && !version) || (cfg.autoJoin && !cfg.address)) {
      navigate('/settings/minecraft');
      return;
    }
    guard.current = true;
    setPending(true);
    setError(null);
    try {
      if (!chosen) {
        const catalog = await backend.gameCatalog();
        if (!catalog.versions.some((entry) => entry.id === version))
          throw new BackendError('UNSUPPORTED_VERSION', false);
      }
      const id =
        chosen?.id ??
        (
          await library.run(() =>
            backend.createInstance({
              name: 'Sporium Survival',
              minecraftVersion: version,
              loader: 'vanilla',
              collectionId: null,
            }),
          )
        ).affectedId;
      await game.start({ id, action: 'world' });
    } catch (reason) {
      setError(normalizeError(reason));
    } finally {
      setPending(false);
      guard.current = false;
    }
  }
  const installing = chosen?.status !== 'installed';
  const action = running
    ? w('Minecraft запущен', 'Minecraft running')
    : working && job
      ? job.phase === 'downloading'
        ? installing
          ? w('Установка Minecraft…', 'Installing Minecraft…')
          : w('Обновление клиента…', 'Updating client…')
        : t(('job.' + job.phase) as Parameters<typeof t>[0])
      : pending || game.starting
        ? w('Проверка файлов…', 'Checking files…')
        : w('ИГРАТЬ', 'PLAY');
  return (
    <>
      <div className="launch-bar">
        <button
          className="play-button"
          disabled={
            !desktop ||
            working ||
            running ||
            game.starting ||
            content.busy ||
            pending ||
            library.loading
          }
          onClick={() => void play()}
        >
          <PixelIcon name="play" />
          <strong>{action}</strong>
        </button>
        <div className="launch-target">
          <span>{w('Игровая сборка', 'Game instance')}</span>
          <Link to={chosen ? '/instance/' + chosen.id : '/settings/minecraft'}>
            {chosen?.name ?? 'Sporium Survival'} <ChevronRight size={13} />
          </Link>
          <small>
            {chosen?.minecraftVersion || version || w('Выбрать версию', 'Select a version')}
          </small>
        </div>
        <Link to="/profile" className="launch-profile">
          <span className="avatar">
            <Skin nickname={nickname} />
          </span>
          <span>
            <small>
              {data.profiles.profiles.find((p) => p.id === data.profiles.activeProfileId)?.microsoft
                ? 'Microsoft'
                : w('Локальный профиль', 'Local profile')}
            </small>
            <b>{nickname}</b>
          </span>
          <ChevronRight size={15} />
        </Link>
      </div>
      {job && (working || job.phase === 'failed') && <GameActivity compact />}
      {!working && !pending && session && !session.running && !!session.exitCode && (
        <p className="game-exit" role="alert">
          {t('game.exit')}: {session.exitCode}{' '}
          <Link className="text-link" to={'/instance/' + session.instanceId}>
            {t('ui.details')}
          </Link>
        </p>
      )}
      {error && <ErrorNotice error={error} />}
    </>
  );
}

function FeedNotice({ gallery = false }: { gallery?: boolean }) {
  const { feedState, refreshFeed } = useWorld();
  const w = useWords();
  if (feedState === 'loading') return <p role="status">{w('Загрузка…', 'Loading…')}</p>;
  if (feedState === 'error')
    return (
      <div>
        <p>{w('Источник недоступен.', 'The source is unavailable.')}</p>
        <button className="quiet-action" onClick={refreshFeed}>
          {w('Повторить', 'Retry')}
        </button>
      </div>
    );
  return (
    <p>
      {feedState === 'unset'
        ? w('Источник ещё не подключён.', 'No source connected yet.')
        : gallery
          ? w('В галерее пока нет снимков.', 'No photos in the gallery yet.')
          : w('Публикаций пока нет.', 'No posts yet.')}
    </p>
  );
}
export function WorldHome() {
  const { data } = useFoundation();
  const { feed } = useWorld();
  const { snapshot, loading } = useLibrary();
  const w = useWords();
  const cfg = data.settings.values.world;
  const hero = useRef<HTMLElement>(null);
  const lead = feed?.news[0];
  const instances = snapshot.instances
    .filter((item) => item.status === 'installed')
    .sort((a, b) => (b.lastPlayedAt ?? 0) - (a.lastPlayedAt ?? 0))
    .slice(0, 3);
  return (
    <div className="world-home">
      <div className="world-head">
        <span className="world-symbol">
          <PixelIcon name="home" />
        </span>
        <h1>{w('Главная', 'Home')}</h1>
      </div>
      <div className="world-feature">
        <section
          className="forest-hero"
          ref={hero}
          style={{ '--effect': cfg.intensity / 100 } as CSSProperties}
          onPointerMove={(e) => {
            if (!cfg.parallax) return;
            if (
              data.settings.values.motion === 'reduced' ||
              matchMedia('(prefers-reduced-motion: reduce)').matches
            )
              return;
            const r = e.currentTarget.getBoundingClientRect();
            e.currentTarget.style.setProperty(
              '--px',
              ((((e.clientX - r.left) / r.width) * 6 - 3) * cfg.intensity) / 100 + 'px',
            );
            e.currentTarget.style.setProperty(
              '--py',
              ((((e.clientY - r.top) / r.height) * 6 - 3) * cfg.intensity) / 100 + 'px',
            );
          }}
          onPointerLeave={() => {
            hero.current?.style.removeProperty('--px');
            hero.current?.style.removeProperty('--py');
          }}
        >
          <div className="forest-picture" />
          <div className="forest-shade" />
          <div className="forest-haze" />
          {cfg.particles && (
            <div className="spores" aria-hidden="true">
              {Array.from({ length: 8 }, (_, i) => (
                <i
                  key={i}
                  style={{
                    left: 13 + i * 11 + '%',
                    animationDelay: -i * 2.3 + 's',
                    top: 26 + ((i * 13) % 58) + '%',
                  }}
                />
              ))}
            </div>
          )}
          <div className="forest-title">
            <div className="hero-edition">
              <b />
              MINECRAFT JAVA EDITION
            </div>
            <h2>
              SPORYCH
              <br />
              <em>SURVIVAL</em>
            </h2>
            <p>
              {w(
                'Уютный Vanilla+ Survival в мире Sporium.',
                'A cozy Vanilla+ survival world in Sporium.',
              )}
            </p>
          </div>
        </section>
        <ServerPanel />
      </div>
      <LaunchBar />
      <section
        className="home-instances"
        aria-labelledby="home-instances-title"
        aria-busy={loading}
      >
        <header className="section-toolbar">
          <h2 id="home-instances-title">{w('Установленные сборки', 'Installed instances')}</h2>
          <Link className="text-link" to="/instances">
            {w('Все сборки', 'All instances')} <PixelIcon name="arrow" />
          </Link>
        </header>
        {instances.length ? (
          <div className="instance-grid">
            {instances.map((instance) => (
              <InstanceCard instance={instance} key={instance.id} />
            ))}
          </div>
        ) : (
          <p className="home-instances-empty">
            {loading
              ? w('Загрузка…', 'Loading…')
              : w('Установленных сборок пока нет.', 'No installed instances yet.')}
          </p>
        )}
      </section>
      <div className="world-tiles">
        <article className="news-tile">
          <div className="tile-top">
            <div className="world-symbol">
              <PixelIcon name="news" />
            </div>
            <Link to="/news" aria-label={w('Все новости', 'All news')}>
              <ArrowUpRight size={19} />
            </Link>
          </div>
          <h2>{lead?.title ?? w('Новости сервера', 'Server news')}</h2>
          {lead ? <p>{lead.summary}</p> : <FeedNotice />}
        </article>
        <Link className="gallery-tile" to="/world">
          <div className="tile-top">
            <div className="world-symbol">
              <PixelIcon name="world" />
            </div>
            <ArrowUpRight size={19} />
          </div>
          <h2>{w('Мир Sporium', 'Sporium World')}</h2>
          <p>{w('Галерея игрового мира', 'World photo gallery')}</p>
        </Link>
      </div>
    </div>
  );
}
export function NewsPage() {
  const { feed } = useWorld();
  const [selected, setSelected] = useState<Article | null>(null);
  const w = useWords();
  return (
    <div className="page world-page">
      <h1>{w('Новости', 'News')}</h1>
      {feed?.news.length ? (
        <div className="feed-grid">
          {feed.news.map((a, i) => (
            <button className="feed-card" key={i} onClick={() => setSelected(a)}>
              {a.image && <WorldImage src={a.image} title="" small />}
              <time>{a.date}</time>
              <h2>{a.title}</h2>
              <p>{a.summary}</p>
              <ArrowRight size={17} />
            </button>
          ))}
        </div>
      ) : (
        <div className="world-empty">
          <Newspaper />
          <h2>{w('Новости сервера', 'Server news')}</h2>
          <FeedNotice />
          <Link to="/settings/advanced">
            {w('Источник публикаций', 'Publication source')} <ArrowRight size={15} />
          </Link>
        </div>
      )}
      {selected && (
        <Modal title={selected.title} onClose={() => setSelected(null)}>
          <time>{selected.date}</time>
          {selected.image && <WorldImage src={selected.image} title="" />}
          <p className="article-body">{selected.body || selected.summary}</p>
        </Modal>
      )}
    </div>
  );
}
export function GalleryPage() {
  const { feed } = useWorld();
  const [selected, setSelected] = useState<Photo | null>(null);
  const w = useWords();
  return (
    <div className="page world-page">
      <h1>{w('Мир Sporium', 'Sporium World')}</h1>
      {feed?.gallery.length ? (
        <div className="feed-grid">
          {feed.gallery.map((p, i) => (
            <button className="feed-card photo" key={i} onClick={() => setSelected(p)}>
              <WorldImage src={p.thumbnail || p.image} title={p.title} small />
              <h2>{p.title}</h2>
            </button>
          ))}
        </div>
      ) : (
        <div className="world-empty">
          <ImageIcon />
          <h2>{w('Галерея мира', 'World gallery')}</h2>
          <FeedNotice gallery />
          <Link to="/settings/advanced">
            {w('Источник фотографий', 'Photo source')} <ArrowRight size={15} />
          </Link>
        </div>
      )}
      {selected && (
        <Modal title={selected.title} className="photo-modal" onClose={() => setSelected(null)}>
          <WorldImage src={selected.image} title={selected.title} />
        </Modal>
      )}
    </div>
  );
}
export function ProfilePage() {
  const w = useWords();
  return (
    <div className="page world-page">
      <div className="world-head">
        <span className="world-symbol">
          <PixelIcon name="profile" />
        </span>
        <h1>{w('Профиль', 'Profile')}</h1>
        <Link className="button secondary profile-settings-link" to="/settings/accounts">
          <PixelIcon name="settings" />
          {w('Настройки аккаунта', 'Account settings')}
        </Link>
      </div>
      <section className="surface world-profiles">
        <ProfilesSettings />
      </section>
    </div>
  );
}
