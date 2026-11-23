import { PixelIcon } from './PixelIcon';
import { Link } from 'react-router-dom';
import { useFoundation } from '../app/context';
import type { Library } from './libraries';
import { useLibrary } from '../library/context';
import { belongsToLibrary } from '../library/view-model';

const categoryIcon = {
  vanilla: 'instances',
  studio: 'settings',
  fabric: 'library',
  forge: 'storage',
  neoforge: 'light',
  managed: 'catalog',
} as const;

export function LibraryCard({ library }: { library: Library }) {
  const { t } = useFoundation();
  const { snapshot } = useLibrary();
  const count = snapshot.instances.filter((item) => belongsToLibrary(item, library.id)).length;
  return (
    <Link
      className={`library-card library-${library.id}`}
      to={`/instances/${library.id}`}
      aria-label={`${library.name} — ${t('home.openLibrary')}`}
    >
      <div className="library-art" aria-hidden="true">
        <div className="library-symbol">
          <PixelIcon name={categoryIcon[library.id]} />
        </div>
        <span className="card-tag">{t(`library.${library.id}.tag`)}</span>
        <span className="card-arrow">
          <PixelIcon name="arrow" />
        </span>
      </div>
      <div className="library-info">
        <h3>{library.name}</h3>
        <p>{t(`library.${library.id}.description`)}</p>
        <div className="library-count">
          <span className="count-dot" />
          {count ? `${t('library.instanceCount')}: ${count}` : t('home.emptyCount')}
        </div>
      </div>
    </Link>
  );
}
