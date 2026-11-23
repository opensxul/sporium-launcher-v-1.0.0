import { useEffect, useRef, useState } from 'react';
import type { ReactNode } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { GameRequest, GameState } from '../bindings/core';
import { backend, normalizeError } from '../services/backend';
import type { BackendError } from '../services/backend';
import { useLibrary } from '../library/context';
import { GameContext, jobActive } from './context';

export function GameProvider({ children }: { children: ReactNode }) {
  const [state, setState] = useState<GameState>({ job: null, sessions: [] });
  const [starting, setStarting] = useState(false);
  const [error, setError] = useState<BackendError | null>(null);
  const [closeWarning, setCloseWarning] = useState(false);
  const completed = useRef('');
  const pending = useRef(false);
  const { reload } = useLibrary();
  useEffect(() => {
    if (!backend.isDesktop) return;
    let active = true;
    let timer: ReturnType<typeof setTimeout>;
    let running = new Set<string>();
    let wait = 3500;
    async function tick() {
      try {
        const view = await backend.gameState();
        if (!active) return;
        wait = jobActive(view) ? 500 : view.sessions.some((s) => s.running) ? 1500 : 3500;
        setState(view);
        const next = new Set(view.sessions.filter((s) => s.running).map((s) => s.instanceId));
        if ([...next].some((id) => !running.has(id)))
          void invoke('window_after_launch').catch((reason) => setError(normalizeError(reason)));
        running = next;
        const key = view.job ? `${view.job.instanceId}:${view.job.phase}` : '';
        if (key !== completed.current && view.job?.phase === 'completed') void reload();
        completed.current = key;
      } catch (reason) {
        if (active) setError(normalizeError(reason));
      } finally {
        if (active) timer = setTimeout(() => void tick(), wait);
      }
    }
    void tick();
    const unlisten = listen('game-close-blocked', () => {
      if (active) setCloseWarning(true);
    });
    return () => {
      active = false;
      clearTimeout(timer);
      void unlisten.then((stop) => stop());
    };
  }, [reload]);
  async function operation(action: () => Promise<GameState>) {
    if (pending.current) return false;
    pending.current = true;
    setStarting(true);
    setError(null);
    setCloseWarning(false);
    try {
      const view = await action();
      setState(view);
      completed.current = '';
      return true;
    } catch (reason) {
      setError(normalizeError(reason));
      return false;
    } finally {
      pending.current = false;
      setStarting(false);
    }
  }
  return (
    <GameContext
      value={{
        state,
        starting,
        busy: starting || jobActive(state),
        error,
        closeWarning,
        dismiss: () => {
          setError(null);
          setCloseWarning(false);
        },
        start: (request: GameRequest) => operation(() => backend.startGame(request)),
        stop: (id) => operation(() => backend.stopGame(id)),
        pause: (paused) => operation(() => backend.pauseDownloads(paused)),
        cancel: async () => {
          try {
            await backend.cancelGame();
          } catch (reason) {
            setError(normalizeError(reason));
          }
        },
      }}
    >
      {children}
    </GameContext>
  );
}
