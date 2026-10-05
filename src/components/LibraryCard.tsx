import { ArrowUpRight } from 'lucide-react';
import { Link } from 'react-router-dom';
import { useFoundation } from '../app/context';
import type { Library } from './libraries';
import { useLibrary } from '../library/context';
import { belongsToLibrary } from '../library/view-model';

export function LibraryCard({ library }: { library: Library }) {
  const { t } = useFoundation();
  const { snapshot } = useLibrary();
  const count = snapshot.instances.filter((item) => belongsToLibrary(item, library.id)).length;
  const Icon = library.icon;
  return (
    <Link
      className={`library-card library-${library.id}`}
      to={`/instances/${library.id}`}
      aria-label={`${library.name} — ${t('home.openLibrary')}`}
    >
      <div className="library-art" aria-hidden="true">
        <span className="art-orbit orbit-one" />
        <span className="art-orbit orbit-two" />
        <span className="art-dot dot-one" />
        <span className="art-dot dot-two" />
        <div className="library-symbol">
          <Icon size={48} strokeWidth={1.15} />
        </div>
        <span className="card-tag">{t(`library.${library.id}.tag`)}</span>
        <ArrowUpRight className="card-arrow" size={19} />
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
