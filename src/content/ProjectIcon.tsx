import { useEffect, useState } from 'react';
import { Box, Image, Layers, Puzzle, Sparkles } from 'lucide-react';
import { backend } from '../services/backend';

function projectIconUrl(value?: string | null) {
  if (!value) return null;
  try {
    const url = new URL(value);
    return url.protocol === 'https:' &&
      url.hostname === 'cdn.modrinth.com' &&
      !url.username &&
      !url.password &&
      !url.port
      ? url.href
      : null;
  } catch {
    return null;
  }
}

const icons = new Map<string, Promise<string | null>>();
export function ProjectIcon({
  url,
  title,
  kind,
  projectId,
  instanceId,
  directory,
  filename,
  sha512,
  revision = 0,
  className = '',
}: {
  url?: string | null;
  title: string;
  kind?: string;
  projectId?: string;
  instanceId?: string;
  directory?: string;
  filename?: string;
  sha512?: string;
  revision?: number;
  className?: string;
}) {
  const [failed, setFailed] = useState<string | null>(null);
  const [resolved, setResolved] = useState<{ id: string; url: string | null } | null>(null);
  const localKey =
    instanceId && directory && filename
      ? `local:${instanceId}/${directory}/${filename}/${sha512 ?? revision}`
      : null;
  const key = localKey ?? projectId;
  useEffect(() => {
    if (url || !key || !backend.isDesktop) return;
    let active = true;
    if (!icons.has(key)) {
      if (icons.size > 500) icons.clear();
      icons.set(
        key,
        (localKey && instanceId && directory && filename
          ? backend.localContentIcon(instanceId, directory, filename, sha512 ?? null)
          : backend.contentIcon(projectId!)
        ).catch(() => null),
      );
    }
    void icons.get(key)!.then((value) => {
      if (active) setResolved({ id: key, url: value });
    });
    return () => {
      active = false;
    };
  }, [url, projectId, key, localKey, instanceId, directory, filename, sha512]);
  const value = url ?? (resolved?.id === key ? resolved?.url : null);
  const source =
    localKey &&
    !url &&
    value &&
    value.length <= 174_790 &&
    /^data:image\/png;base64,[A-Za-z0-9+/]+={0,2}$/.test(value)
      ? value
      : projectIconUrl(value);
  const Icon =
    kind === 'mod'
      ? Puzzle
      : kind === 'resourcepack'
        ? Image
        : kind === 'shader'
          ? Sparkles
          : kind === 'modpack'
            ? Layers
            : Box;
  return (
    <span className={`project-icon ${className}`}>
      {source && source !== failed ? (
        <img
          src={source}
          alt={title}
          width={56}
          height={56}
          loading="lazy"
          decoding="async"
          referrerPolicy="no-referrer"
          onError={() => setFailed(source)}
        />
      ) : (
        <Icon size={28} strokeWidth={1.5} aria-hidden="true" />
      )}
    </span>
  );
}
