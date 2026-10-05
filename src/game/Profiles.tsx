import { useEffect, useRef, useState } from 'react';
import { UserRound, Plus, Pencil, Trash2, Check, RefreshCw } from 'lucide-react';
import type { EditProfile, LocalProfile, SkinView } from '../bindings/core';
import { useFoundation } from '../app/context';
import { useLibrary } from '../library/context';
import { backend, normalizeError, type BackendError } from '../services/backend';
import { ErrorNotice, Toggle } from '../components/ui';
import { Modal } from '../components/Modal';

const skins = new Map<string, Promise<SkinView>>();
export function Skin({ nickname, full = false }: { nickname: string; full?: boolean }) {
  const { desktop, t } = useFoundation();
  const [view, setView] = useState<SkinView | null>(null);
  const [attempt, setAttempt] = useState(0);
  const canvas = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    let active = true;
    if (!desktop) return;
    let operation = skins.get(nickname.toLowerCase());
    if (!operation || attempt) {
      operation = backend.profileSkin(nickname, attempt > 0);
      skins.set(nickname.toLowerCase(), operation);
    }
    operation
      .then((value) => {
        if (active) setView(value);
      })
      .catch(() => {
        if (active) setView(null);
        skins.delete(nickname.toLowerCase());
      });
    return () => {
      active = false;
    };
  }, [nickname, desktop, attempt]);
  useEffect(() => {
    const node = canvas.current;
    if (!node) return;
    const context = node.getContext('2d');
    if (!context) return;
    context.clearRect(0, 0, node.width, node.height);
    if (!view?.png) return;
    let active = true;
    const img = new Image();
    img.onload = () => {
      if (!active) return;
      context.imageSmoothingEnabled = false;
      // Old 64×32 skins may fill the unused hat area with opaque black. Minecraft
      // treats an entirely opaque legacy overlay as absent rather than a black head.
      let hasHat = true;
      if (img.height === 32) {
        const source = document.createElement('canvas');
        source.width = 64;
        source.height = 32;
        const pixels = source.getContext('2d');
        if (pixels) {
          pixels.drawImage(img, 0, 0);
          const data = pixels.getImageData(32, 0, 32, 32).data;
          hasHat = data.some((value, index) => index % 4 === 3 && value < 128);
        }
      }
      if (!full) {
        context.drawImage(img, 8, 8, 8, 8, 0, 0, 32, 32);
        if (hasHat) context.drawImage(img, 40, 8, 8, 8, 0, 0, 32, 32);
        return;
      }
      const draw = (x: number, y: number, w: number, h: number, dx: number, dy: number) =>
        context.drawImage(img, x, y, w, h, dx * 6, dy * 6, w * 6, h * 6);
      draw(8, 8, 8, 8, 4, 0);
      draw(20, 20, 8, 12, 4, 8);
      draw(4, 20, 4, 12, 4, 20);
      const arm = view.slim ? 3 : 4;
      draw(44, 20, arm, 12, 4 - arm, 8);
      if (img.height === 64) {
        draw(20, 52, 4, 12, 8, 20);
        draw(36, 52, arm, 12, 12, 8);
        draw(20, 36, 8, 12, 4, 8);
        draw(4, 36, 4, 12, 4, 20);
        draw(4, 52, 4, 12, 8, 20);
        draw(44, 36, arm, 12, 4 - arm, 8);
        draw(52, 52, arm, 12, 12, 8);
      } else {
        draw(4, 20, 4, 12, 8, 20);
        draw(44, 20, 4, 12, 12, 8);
      }
      if (hasHat) draw(40, 8, 8, 8, 4, 0);
    };
    img.src = view.png;
    return () => {
      active = false;
    };
  }, [view, full]);
  return (
    <div className={full ? 'skin-preview' : 'skin-avatar'}>
      {view?.png ? (
        <canvas
          ref={canvas}
          width={full ? 96 : 32}
          height={full ? 192 : 32}
          aria-label={`${t('profile.skin')}: ${nickname}`}
          role="img"
        />
      ) : (
        <UserRound size={full ? 64 : 24} />
      )}
      {full && (
        <>
          <small>
            {t(
              view?.status === 'found'
                ? 'profile.skinFound'
                : view?.status === 'cached'
                  ? 'profile.skinCached'
                  : view?.status === 'unavailable'
                    ? 'profile.skinUnavailable'
                    : 'profile.skinMissing',
            )}
          </small>
          <button
            className="button text-button"
            disabled={!desktop}
            onClick={() => setAttempt((v) => v + 1)}
          >
            <RefreshCw size={14} />
            {t('profile.skinRefresh')}
          </button>
        </>
      )}
    </div>
  );
}

export function ProfilesSettings() {
  const { data, t, desktop, reload } = useFoundation();
  const library = useLibrary();
  const [dialog, setDialog] = useState<{
    action: 'create' | 'rename' | 'delete';
    profile?: LocalProfile;
  } | null>(null);
  const [nickname, setNickname] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<BackendError | null>(null);
  async function edit(request: Omit<EditProfile, 'expectedRevision'>) {
    setBusy(true);
    setError(null);
    try {
      await backend.editProfile({ ...request, expectedRevision: data.profiles.revision });
      await reload();
      await library.reload();
      setDialog(null);
    } catch (reason) {
      setError(normalizeError(reason));
      await reload();
    } finally {
      setBusy(false);
    }
  }
  return (
    <div className="profiles-settings">
      <p className="setting-hint">{t('game.localHint')}</p>
      <p className="setting-hint">{t('profile.skinScope')}</p>
      <Toggle
        label={t('profile.inGameSkins')}
        description={t('profile.inGameSkinsHint')}
        checked={data.settings.values.nicknameSkins}
        disabled={!desktop || busy}
        onChange={(nicknameSkins) => {
          setBusy(true);
          setError(null);
          void backend
            .saveSettings({
              values: { ...data.settings.values, nicknameSkins },
              expectedRevision: data.settings.revision,
            })
            .then(() => reload())
            .catch((reason) => {
              setError(normalizeError(reason));
              void reload();
            })
            .finally(() => setBusy(false));
        }}
      />
      <button
        className="button primary"
        disabled={!desktop || busy}
        onClick={() => {
          setNickname('');
          setDialog({ action: 'create' });
          setError(null);
        }}
      >
        <Plus size={16} />
        {t('profile.add')}
      </button>
      <div className="profile-list">
        {data.profiles.profiles.map((profile) => (
          <article
            className="profile-card"
            data-active={profile.id === data.profiles.activeProfileId}
            key={profile.id}
          >
            <Skin key={profile.nickname} nickname={profile.nickname} full />
            <div>
              <h3>{profile.nickname}</h3>
              <p>{t('account.guest')}</p>
              <div className="instance-actions">
                <button
                  className="button secondary"
                  disabled={!desktop || busy || profile.id === data.profiles.activeProfileId}
                  onClick={() => void edit({ action: 'select', id: profile.id, nickname: null })}
                >
                  <Check size={15} />
                  {t(
                    profile.id === data.profiles.activeProfileId
                      ? 'profile.active'
                      : 'profile.select',
                  )}
                </button>
                <button
                  className="button secondary"
                  disabled={!desktop || busy}
                  onClick={() => {
                    setNickname(profile.nickname);
                    setDialog({ action: 'rename', profile });
                    setError(null);
                  }}
                >
                  <Pencil size={15} />
                  {t('profile.rename')}
                </button>
                <button
                  className="button secondary danger-text"
                  disabled={!desktop || busy || data.profiles.profiles.length === 1}
                  onClick={() => {
                    setDialog({ action: 'delete', profile });
                    setError(null);
                  }}
                >
                  <Trash2 size={15} />
                  {t('common.delete')}
                </button>
              </div>
            </div>
          </article>
        ))}
      </div>
      {error && !dialog && <ErrorNotice error={error} />}
      {dialog && (
        <Modal
          title={t(
            dialog.action === 'create'
              ? 'profile.add'
              : dialog.action === 'rename'
                ? 'profile.rename'
                : 'profile.delete',
          )}
          busy={busy}
          onClose={() => setDialog(null)}
        >
          {dialog.action === 'delete' ? (
            <p>
              {t('profile.deleteHint')} <strong>{dialog.profile?.nickname}</strong>
            </p>
          ) : (
            <div className="form-fields">
              <label>
                {t('game.nickname')}
                <input
                  value={nickname}
                  onChange={(e) => setNickname(e.target.value)}
                  maxLength={16}
                  autoFocus
                  disabled={busy}
                />
              </label>
              <p className="form-hint">{t('game.nicknameHint')}</p>
            </div>
          )}
          {error && <ErrorNotice error={error} />}
          <div className="modal-actions">
            <button className="button secondary" disabled={busy} onClick={() => setDialog(null)}>
              {t('common.cancel')}
            </button>
            <button
              className={`button ${dialog.action === 'delete' ? 'danger' : 'primary'}`}
              disabled={
                busy || (dialog.action !== 'delete' && !/^[A-Za-z0-9_]{1,16}$/.test(nickname))
              }
              onClick={() =>
                void edit({
                  action: dialog.action,
                  id: dialog.profile?.id ?? null,
                  nickname: dialog.action === 'delete' ? null : nickname,
                })
              }
            >
              {t(dialog.action === 'delete' ? 'common.delete' : 'common.save')}
            </button>
          </div>
        </Modal>
      )}
    </div>
  );
}
