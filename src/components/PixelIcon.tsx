import type { CSSProperties } from 'react';

type IconName =
  | 'home'
  | 'news'
  | 'world'
  | 'library'
  | 'catalog'
  | 'instances'
  | 'folders'
  | 'downloads'
  | 'settings'
  | 'profile'
  | 'java'
  | 'skins'
  | 'play'
  | 'search'
  | 'close'
  | 'arrow'
  | 'warning'
  | 'light'
  | 'storage';

export function PixelIcon({ name }: { name: IconName }) {
  return (
    <span
      className="pixel-icon"
      aria-hidden="true"
      style={{ '--pixel-image': `url("/branding/pixel/${name}.png")` } as CSSProperties}
    />
  );
}
