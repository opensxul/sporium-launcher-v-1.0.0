import { useCallback, useEffect, useRef, useState } from 'react';
import type { ReactNode } from 'react';
import type { LibraryChange, LibrarySnapshot } from '../bindings/core';
import { backend, BackendError, normalizeError } from '../services/backend';
import { LibraryContext } from './context';

export function LibraryProvider({ children }: { children: ReactNode }) {
  const [snapshot, setSnapshot] = useState<LibrarySnapshot>({ instances: [], collections: [] });
  const [loading, setLoading] = useState(backend.isDesktop);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<BackendError | null>(null);
  const [backup, setBackup] = useState<{ id: string; directory: string } | null>(null);
  const pending = useRef(false);
  const sequence = useRef(0);
  const request = useRef<Promise<LibrarySnapshot> | null>(null);

  const reload = useCallback(async () => {
    if (!backend.isDesktop || pending.current) return;
    const current = ++sequence.current;
    setLoading(true);
    const inflight = request.current ?? backend.librarySnapshot();
    request.current = inflight;
    try {
      const result = await inflight;
      if (current === sequence.current) {
        setSnapshot(result);
        setError(null);
      }
    } catch (reason) {
      if (current === sequence.current) setError(normalizeError(reason));
    } finally {
      if (request.current === inflight) request.current = null;
      if (current === sequence.current) {
        setLoading(false);
      }
    }
  }, []);

  const invalidate = useCallback(() => {
    ++sequence.current;
  }, []);
  useEffect(() => {
    void reload();
    return invalidate;
  }, [reload, invalidate]);

  const run = useCallback(async (operation: () => Promise<LibraryChange>) => {
    if (pending.current) throw new BackendError('LIBRARY_BUSY', true);
    pending.current = true;
    ++sequence.current;
    setLoading(false);
    setBusy(true);
    try {
      // Finish an existing read before the mutation. pending prevents fresh background reads.
      const reading = request.current;
      if (reading) {
        await reading.catch(() => {});
        if (request.current === reading) request.current = null;
      }
      const result = await operation();
      setSnapshot(result.snapshot);
      setError(null);
      if (result.preservedDirectory)
        setBackup({ id: result.affectedId, directory: result.preservedDirectory });
      return result;
    } catch (reason) {
      throw normalizeError(reason);
    } finally {
      pending.current = false;
      setBusy(false);
    }
  }, []);

  return (
    <LibraryContext
      value={{
        snapshot,
        loading,
        busy,
        error,
        backup,
        dismissBackup: () => setBackup(null),
        reload,
        run,
      }}
    >
      {children}
    </LibraryContext>
  );
}
