import { useEffect, useRef, useState } from 'react';
import type { ReactNode } from 'react';
import type { ContentJob } from '../bindings/core';
import { useLibrary } from '../library/context';
import { backend, normalizeError } from '../services/backend';
import type { BackendError } from '../services/backend';
import { ContentContext, contentActive } from './context';
export function ContentProvider({ children }: { children: ReactNode }) {
  const [job, setJob] = useState<ContentJob | null>(null);
  const [error, setError] = useState<BackendError | null>(null);
  const [starting, setStarting] = useState(false);
  const pending = useRef(false);
  const { reload } = useLibrary();
  const observed = useRef('');
  useEffect(() => {
    if (!backend.isDesktop) return;
    let active = true;
    let observedReports = '';
    let timer: ReturnType<typeof setTimeout>;
    async function tick() {
      try {
        const reports = await backend.automaticReports();
        const signature = reports
          .filter((report) => report.status === 'project_installed')
          .map((report) => `${report.instanceId}:${report.checkedAt}`)
          .sort()
          .join('|');
        if (active && signature !== observedReports) {
          observedReports = signature;
          if (signature) {
            await reload();
            window.dispatchEvent(new Event('sporium-project-changed'));
          }
        }
      } finally {
        if (active) timer = setTimeout(() => void tick().catch(() => {}), 30000);
      }
    }
    void tick().catch(() => {});
    return () => {
      active = false;
      clearTimeout(timer);
    };
  }, [reload]);
  useEffect(() => {
    if (!backend.isDesktop) return;
    let active = true;
    let timer: ReturnType<typeof setTimeout>;
    async function tick() {
      try {
        const value = await backend.contentState();
        if (active) {
          setJob(value);
          const key = value ? value.instanceId + ':' + value.phase : '';
          if (key !== observed.current) {
            observed.current = key;
            void reload();
          }
        }
      } catch (reason) {
        if (active) setError(normalizeError(reason));
      } finally {
        if (active) timer = setTimeout(() => void tick(), 800);
      }
    }
    void tick();
    return () => {
      active = false;
      clearTimeout(timer);
    };
  }, [reload]);
  return (
    <ContentContext
      value={{
        job,
        error,
        busy: starting || contentActive(job),
        install: async (token) => {
          if (pending.current) return false;
          pending.current = true;
          setStarting(true);
          setError(null);
          try {
            setJob(await backend.contentInstall(token));
            await reload();
            return true;
          } catch (reason) {
            setError(normalizeError(reason));
            return false;
          } finally {
            pending.current = false;
            setStarting(false);
          }
        },
        cancel: async () => {
          try {
            await backend.contentCancel();
          } catch (reason) {
            setError(normalizeError(reason));
          }
        },
      }}
    >
      {children}
    </ContentContext>
  );
}
