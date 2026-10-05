import { useState } from 'react';
import { Download, Play, Square } from 'lucide-react';
import { Link } from 'react-router-dom';
import type { Instance } from '../bindings/core';
import { useFoundation } from '../app/context';
import { useGame } from './context';
import { GameActivity } from './GameActivity';
import { Modal } from '../components/Modal';
import { ErrorNotice } from '../components/ui';
import { useVersionCatalog } from './catalog';
import { useContent } from '../content/context';
import { InstanceStatus } from '../library/InstanceStatus';

export function InstanceGame({ instance }: { instance: Instance }) {
  const { t, desktop, data } = useFoundation();
  const { state, busy, start, stop, starting } = useGame();
  const content = useContent();
  const [stopping, setStopping] = useState(false);
  const session = state.sessions.find((item) => item.instanceId === instance.id);
  const { catalog, loading, error, reload } = useVersionCatalog();
  const version = catalog?.versions.find((item) => item.id === instance.minecraftVersion);
  const nickname =
    data.profiles.profiles.find((p) => p.id === data.profiles.activeProfileId)?.nickname ??
    data.settings.values.localNickname;
  return (
    <>
      <div className="instance-status-panel">
        <div>
          <InstanceStatus instance={instance} />
          <p>
            {t(
              loading ? 'game.fetching' : !version ? 'error.UNSUPPORTED_VERSION' : 'game.localHint',
            )}
          </p>
          <Link className="text-link" to="/settings/accounts">
            {t('profile.active')}: {nickname}
          </Link>
        </div>
        <div className="game-buttons">
          {session?.running ? (
            <button
              className="button secondary"
              disabled={starting}
              onClick={() => setStopping(true)}
            >
              <Square size={16} />
              {t('game.stop')}
            </button>
          ) : (
            <>
              <button
                className="button secondary"
                disabled={!desktop || !version || busy || content.busy}
                onClick={() => void start({ id: instance.id, action: 'install' })}
              >
                <Download size={17} />
                {t(instance.status === 'installed' ? 'game.repair' : 'game.install')}
              </button>
              <button
                className="button primary"
                disabled={!desktop || !version || busy || content.busy}
                onClick={() => void start({ id: instance.id, action: 'local' })}
              >
                <Play size={17} />
                {t('game.local')}
              </button>
            </>
          )}
        </div>
      </div>
      {starting && (
        <p role="status" className="setting-hint">
          {t('game.waitingReads')}
        </p>
      )}
      {error && (
        <ErrorNotice
          error={error}
          action={
            <button className="button secondary" onClick={() => void reload()}>
              {t('game.refresh')}
            </button>
          }
        />
      )}
      {state.job?.instanceId === instance.id && <GameActivity compact />}
      {session && !session.running && (
        <div className="game-exit" role="status">
          {t('game.exit')}: {session.exitCode ?? '—'}
          <details>
            <summary>{t('ui.details')}</summary>
            <code>{session.logPath}</code>
          </details>
        </div>
      )}
      {stopping && (
        <Modal title={t('game.stop')} busy={starting} onClose={() => setStopping(false)}>
          <p>{t('game.stopHint')}</p>
          <div className="modal-actions">
            <button
              className="button secondary"
              disabled={starting}
              onClick={() => setStopping(false)}
            >
              {t('common.cancel')}
            </button>
            <button
              className="button danger"
              disabled={starting}
              onClick={async () => {
                if (await stop(instance.id)) setStopping(false);
              }}
            >
              {t('game.stop')}
            </button>
          </div>
        </Modal>
      )}
    </>
  );
}
