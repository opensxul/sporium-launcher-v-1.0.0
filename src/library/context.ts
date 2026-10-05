import { createContext, useContext } from 'react';
import type { LibraryChange, LibrarySnapshot } from '../bindings/core';
import type { BackendError } from '../services/backend';

export interface LibraryContextValue {
  snapshot: LibrarySnapshot;
  loading: boolean;
  busy: boolean;
  error: BackendError | null;
  backup: { id: string; directory: string } | null;
  dismissBackup: () => void;
  reload: () => Promise<void>;
  run: (operation: () => Promise<LibraryChange>) => Promise<LibraryChange>;
}

export const LibraryContext = createContext<LibraryContextValue | null>(null);
export function useLibrary() {
  const context = useContext(LibraryContext);
  if (!context) throw new Error('Library context missing');
  return context;
}
