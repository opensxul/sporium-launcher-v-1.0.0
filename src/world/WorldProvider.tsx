import { useEffect, useState, type ReactNode } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useFoundation } from '../app/context';
import { WorldContext, type Feed, type Status, type LoadState } from './context';

function useResource<T>(key: string, command: string, interval: number) {
  const [result, setResult] = useState<{ key: string; state: LoadState; value: T | null } | null>(
    null,
  );
  const [revision, setRevision] = useState(0);
  useEffect(() => {
    if (!key) return;
    let active = true,
      pending = false;
    let last = 0;
    const load = async () => {
      if (pending || document.hidden || Date.now() - last < interval) return;
      pending = true;
      last = Date.now();
      setResult((old) => ({ key, state: 'loading', value: old?.key === key ? old.value : null }));
      try {
        const value = await invoke<T>(command);
        if (active) setResult({ key, state: 'ready', value });
      } catch {
        if (active) setResult({ key, state: 'error', value: null });
      } finally {
        pending = false;
      }
    };
    void load();
    const timer = window.setInterval(load, interval);
    document.addEventListener('visibilitychange', load);
    return () => {
      active = false;
      clearInterval(timer);
      document.removeEventListener('visibilitychange', load);
    };
  }, [key, command, interval, revision]);
  const state: LoadState = !key ? 'unset' : result?.key === key ? result.state : 'loading';
  return {
    value: result?.key === key ? result.value : null,
    state,
    refresh: () => setRevision((v) => v + 1),
  };
}
export function WorldProvider({ children }: { children: ReactNode }) {
  const { desktop, data } = useFoundation();
  const cfg = data.settings.values.world;
  const status = useResource<Status>(desktop ? cfg.address : '', 'server_status', 60000);
  const feed = useResource<Feed>(desktop ? cfg.feed : '', 'world_feed', 300000);
  return (
    <WorldContext
      value={{
        status: status.value,
        statusState: status.state,
        refreshStatus: status.refresh,
        feed: feed.value,
        feedState: feed.state,
        refreshFeed: feed.refresh,
      }}
    >
      {children}
    </WorldContext>
  );
}
