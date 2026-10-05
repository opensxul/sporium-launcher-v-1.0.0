import { createContext, useContext } from 'react';
import type { ContentJob } from '../bindings/core';
import type { BackendError } from '../services/backend';
export function contentActive(job: ContentJob | null) {
  return !!job && ['downloading', 'applying', 'preparing_game'].includes(job.phase);
}
export const ContentContext = createContext<{
  job: ContentJob | null;
  error: BackendError | null;
  busy: boolean;
  install: (token: string) => Promise<boolean>;
  cancel: () => Promise<void>;
} | null>(null);
export function useContent() {
  const value = useContext(ContentContext);
  if (!value) throw new Error('Content context missing');
  return value;
}
