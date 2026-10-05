import { useEffect, useRef, useState } from 'react';
import type { ReactNode } from 'react';
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
    async function tick() {
      try {
        const view = await backend.gameState();
        if (!active) return;
        setState(view);
        const key = view.job ? `${view.job.instanceId}:${view.job.phase}` : '';
        if (key !== completed.current && view.job?.phase === 'completed') void reload();
        completed.current = key;
      } catch (reason) {
        if (active) setError(normalizeError(reason));
      } finally {
        if (active) timer = setTimeout(() => void tick(), 750);
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
