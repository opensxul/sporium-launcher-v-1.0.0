import { AlertTriangle, RotateCw } from 'lucide-react';
import { useFoundation } from './context';

export function RouteFailure() {
  const { t } = useFoundation();
  return (
    <div className="startup-screen">
      <div className="startup-card" role="alert">
        <AlertTriangle size={30} />
        <h1>{t('error.title')}</h1>
        <p>{t('error.UNKNOWN')}</p>
        <button className="button primary" onClick={() => window.location.reload()}>
          <RotateCw size={16} />
          {t('error.retry')}
        </button>
      </div>
    </div>
  );
}
