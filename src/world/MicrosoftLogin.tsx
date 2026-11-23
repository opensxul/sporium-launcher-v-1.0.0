import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useFoundation } from '../app/context';
import { Modal } from '../components/Modal';
import { ErrorNotice } from '../components/ui';
import { normalizeError, type BackendError } from '../services/backend';
type Challenge = { ticket: string; userCode: string; interval: number; verification: string };

export function MicrosoftLogin({
  id,
  nickname,
  linked,
}: {
  id: string;
  nickname: string;
  linked: boolean;
}) {
  const { desktop, data, reload } = useFoundation();
  const ru = data.settings.values.locale === 'ru-RU';
  const [available, setAvailable] = useState(false);
  const [challenge, setChallenge] = useState<Challenge | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<BackendError | null>(null);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    if (desktop)
      void invoke<boolean>('microsoft_available')
        .then((v) => {
          if (mounted.current) setAvailable(v);
        })
        .catch(() => {});
    return () => {
      mounted.current = false;
    };
  }, [desktop]);
  useEffect(() => {
    if (!challenge) return;
    let active = true;
    let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      try {
        const ok = await invoke<boolean>('microsoft_poll', { ticket: challenge.ticket });
        if (!active) return;
        if (ok) {
          setChallenge(null);
          await reload();
        } else timer = setTimeout(() => void poll(), challenge.interval * 1000);
      } catch (reason) {
        if (active) {
          setChallenge(null);
          setError(normalizeError(reason));
        }
      }
    };
    timer = setTimeout(() => void poll(), challenge.interval * 1000);
    return () => {
      active = false;
      clearTimeout(timer);
      void invoke('microsoft_cancel', { ticket: challenge.ticket }).catch(() => {});
    };
  }, [challenge, reload]);
  async function begin() {
    setBusy(true);
    setError(null);
    try {
      const c = await invoke<Challenge>('microsoft_begin', { profile: id });
      if (mounted.current) setChallenge(c);
      else await invoke('microsoft_cancel', { ticket: c.ticket });
    } catch (e) {
      if (mounted.current) setError(normalizeError(e));
    } finally {
      if (mounted.current) setBusy(false);
    }
  }
  return (
    <div className="ms-login">
      {available ? (
        <button
          className="button secondary"
          disabled={busy || !!challenge}
          onClick={() => void begin()}
        >
          {linked
            ? ru
              ? 'Повторить вход Microsoft'
              : 'Sign in again'
            : ru
              ? 'Войти через Microsoft'
              : 'Sign in with Microsoft'}
        </button>
      ) : (
        <small>
          {ru
            ? 'Вход Microsoft требует регистрации приложения Sporium.'
            : 'Microsoft sign-in requires a registered Sporium app.'}
        </small>
      )}
      {error && <ErrorNotice error={error} />}
      {challenge && (
        <Modal
          title={ru ? 'Вход Microsoft' : 'Microsoft sign-in'}
          onClose={() => setChallenge(null)}
        >
          <p>
            {ru
              ? 'На странице Microsoft введите код и выберите аккаунт Minecraft с ником'
              : 'Enter this code on the Microsoft page and choose the Minecraft account named'}{' '}
            <b>{nickname}</b>.
          </p>
          <strong className="device-code">{challenge.userCode}</strong>
          <p>{challenge.verification}</p>
        </Modal>
      )}
    </div>
  );
}
