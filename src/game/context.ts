import { createContext, useContext } from 'react';
import type { GameRequest, GameState } from '../bindings/core';
import type { BackendError } from '../services/backend';

export interface GameContextValue {
  state: GameState;
  busy: boolean;
  starting: boolean;
  closeWarning: boolean;
  error: BackendError | null;
  dismiss: () => void;
  start: (request: GameRequest) => Promise<boolean>;
  stop: (id: string) => Promise<boolean>;
  cancel: () => Promise<void>;
  pause: (paused: boolean) => Promise<boolean>;
}
export const GameContext = createContext<GameContextValue | null>(null);
export function useGame() {
  const context = useContext(GameContext);
  if (!context) throw new Error('Game context missing');
  return context;
}
export function jobActive(state: GameState) {
  return (
    !!state.job && !['completed', 'cancelled', 'failed', 'interrupted'].includes(state.job.phase)
  );
}
