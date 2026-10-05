import { useCallback, useEffect, useState } from 'react';
import type { GameVersion, VersionCatalog, VersionVisibility } from '../bindings/core';
import { backend, normalizeError } from '../services/backend';
import type { BackendError } from '../services/backend';

let cache: { value: VersionCatalog; time: number } | null = null;
let request: Promise<VersionCatalog> | null = null;
function fetchCatalog(refresh: boolean) {
  if (!refresh && cache && Date.now() - cache.time < 300_000) return Promise.resolve(cache.value);
  request ??= backend
    .gameCatalog(refresh)
    .then((value) => {
      cache = { value, time: Date.now() };
      return value;
    })
    .finally(() => {
      request = null;
    });
  return request;
}
export function useVersionCatalog() {
  const [catalog, setCatalog] = useState<VersionCatalog | null>(cache?.value ?? null);
  const [loading, setLoading] = useState(backend.isDesktop && !cache);
  const [error, setError] = useState<BackendError | null>(null);
  const reload = useCallback(async () => {
    if (!backend.isDesktop) return;
    setLoading(true);
    try {
      setCatalog(await fetchCatalog(true));
      setError(null);
    } catch (reason) {
      setError(normalizeError(reason));
    } finally {
      setLoading(false);
    }
  }, []);
  useEffect(() => {
    if (!backend.isDesktop) return;
    let active = true;
    void fetchCatalog(false)
      .then(
        (value) => {
          if (active) {
            setCatalog(value);
            setError(null);
          }
        },
        (reason) => {
          if (active) setError(normalizeError(reason));
        },
      )
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, []);
  return { catalog, loading, error, reload };
}
export function visibleVersions(versions: GameVersion[], visibility: VersionVisibility) {
  return versions.filter(
    (version) =>
      ({
        release: visibility.releases,
        snapshot: visibility.snapshots,
        old_beta: visibility.beta,
        old_alpha: visibility.alpha,
        other: visibility.other,
      })[version.kind],
  );
}
