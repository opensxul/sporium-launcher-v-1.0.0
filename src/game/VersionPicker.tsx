import { Link } from 'react-router-dom';
import { useFoundation } from '../app/context';
import { ErrorNotice } from '../components/ui';
import { useVersionCatalog, visibleVersions } from './catalog';

export function VersionPicker({
  value,
  onChange,
}: {
  value: string;
  onChange: (value: string) => void;
}) {
  const { t, data } = useFoundation();
  const { catalog, loading, error, reload } = useVersionCatalog();
  const versions = visibleVersions(catalog?.versions ?? [], data.settings.values.versionVisibility);
  return (
    <div className="version-picker">
      <label>
        {t('instance.minecraftVersion')}
        <select
          required
          value={value}
          aria-label={t('instance.minecraftVersion')}
          disabled={loading || !!error}
          onChange={(event) => onChange(event.target.value)}
        >
          <option value="">{t(loading ? 'game.fetching' : 'game.chooseVersion')}</option>
          {versions.map((version) => (
            <option key={version.id} value={version.id}>
              {version.id}
            </option>
          ))}
        </select>
      </label>
      {error && (
        <ErrorNotice
          error={error}
          action={
            <button className="button secondary" type="button" onClick={() => void reload()}>
              {t('library.reload')}
            </button>
          }
        />
      )}
      {catalog?.cached && <p className="form-hint">{t('game.cached')}</p>}
      <Link className="text-link" to="/settings/minecraft">
        {t('game.openSettings')}
      </Link>
    </div>
  );
}
