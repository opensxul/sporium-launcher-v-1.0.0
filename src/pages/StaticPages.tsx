import { ArrowLeft, MapPin } from 'lucide-react';
import { Link } from 'react-router-dom';
import { useFoundation } from '../app/context';
import { EmptyState, PageHeading } from '../components/ui';
import { GameActivity } from '../game/GameActivity';
import { ContentActivity } from '../content/ContentActivity';

export function DownloadsPage() {
  const { t } = useFoundation();
  return (
    <div className="page">
      <PageHeading title={t('nav.downloads')} description={t('download.pageHint')} />
      <ContentActivity />
      <section className="surface">
        <GameActivity />
      </section>
    </div>
  );
}

export function NotFoundPage() {
  const { t } = useFoundation();
  return (
    <div className="page">
      <EmptyState icon={MapPin} title={t('error.notFound')} body={t('error.notFoundBody')}>
        <Link className="button secondary" to="/">
          <ArrowLeft size={16} />
          {t('library.back')}
        </Link>
      </EmptyState>
    </div>
  );
}
