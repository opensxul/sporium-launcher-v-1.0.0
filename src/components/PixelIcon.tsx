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

export function PixelIcon({ name, colored = false }: { name: IconName; colored?: boolean }) {
  return (
    <span
      className={colored ? 'pixel-icon pixel-icon-color' : 'pixel-icon'}
      aria-hidden="true"
      style={
        {
          '--pixel-image': `url("/branding/pixel/${colored ? 'color-' : ''}${name}.png")`,
        } as CSSProperties
      }
    />
  );
}
