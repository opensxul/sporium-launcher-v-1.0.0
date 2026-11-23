import { createContext, useContext } from 'react';
export type Article = {
  title: string;
  date: string;
  summary: string;
  body: string;
  image: string | null;
};
export type Photo = { title: string; image: string; thumbnail: string | null };
export type Feed = { news: Article[]; gallery: Photo[] };
export type Status = {
  online: boolean;
  players: number | null;
  maximum: number | null;
  version: string | null;
  latencyMs: number | null;
};
export type LoadState = 'unset' | 'loading' | 'ready' | 'error';
export const WorldContext = createContext<{
  status: Status | null;
  statusState: LoadState;
  refreshStatus: () => void;
  feed: Feed | null;
  feedState: LoadState;
  refreshFeed: () => void;
} | null>(null);
export function useWorld() {
  const context = useContext(WorldContext);
  if (!context) throw new Error('World provider missing');
  return context;
}
