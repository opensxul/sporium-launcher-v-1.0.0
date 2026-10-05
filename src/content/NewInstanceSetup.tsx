import { useEffect, useState } from 'react';
import type {
  ContentCreateRequest,
  ContentDetails,
  ContentVersion,
  Loader,
} from '../bindings/core';
import { useFoundation } from '../app/context';
import { loaderNames } from '../library/view-model';

function contentLoaders(kind: string, version: ContentVersion): Loader[] {
  if (kind === 'resourcepack' && version.loaders.includes('minecraft')) return ['vanilla'];
  if (kind === 'shader') return []; // An engine must already be installed in the destination.
  return (['fabric', 'forge', 'neo_forge'] as const).filter((loader) =>
    version.loaders.includes(loader === 'neo_forge' ? 'neoforge' : loader),
  );
}

export function NewInstanceSetup({
  details,
  disabled,
  onChange,
  initial,
}: {
  details: ContentDetails;
  disabled: boolean;
  onChange: (request: ContentCreateRequest | null) => void;
  initial?: ContentCreateRequest | null;
}) {
  const { t } = useFoundation();
  const versions = details.versions.filter(
    (v) =>
      contentLoaders(details.project.project_type, v).length > 0 &&
      v.environment !== 'dedicated_server_only' &&
      !(
        (!v.environment || v.environment === 'unknown') &&
        details.project.client_side === 'unsupported'
      ),
  );
  const [selected, setSelected] = useState(initial?.versionId ?? '');
  const version =
    versions.find((v) => v.id === selected) ??
    versions.find((v) => v.version_type === 'release') ??
    versions[0];
  const [name, setName] = useState(initial?.instance.name ?? details.project.title.slice(0, 80));
  const [minecraft, setMinecraft] = useState(initial?.instance.minecraftVersion ?? '');
  const [loader, setLoader] = useState<Loader>(initial?.instance.loader ?? 'fabric');
  const games = [...(version?.game_versions ?? [])].sort((a, b) =>
    b.localeCompare(a, undefined, { numeric: true }),
  );
  const game = games.includes(minecraft) ? minecraft : (games[0] ?? '');
  const loaders = version ? contentLoaders(details.project.project_type, version) : [];
  const chosenLoader = loaders.includes(loader) ? loader : loaders[0];
  useEffect(() => {
    onChange(
      version && game && chosenLoader && name.trim()
        ? {
            projectId: details.project.id,
            versionId: version.id,
            instance: {
              name: name.trim(),
              minecraftVersion: game,
              loader: chosenLoader,
              collectionId: null,
            },
          }
        : null,
    );
  }, [details.project.id, version, game, chosenLoader, name, onChange]);
  if (!version) return <p>{t('content.noneCompatible')}</p>;
  return (
    <fieldset className="content-new-instance" disabled={disabled}>
      <p className="setting-hint">{t('content.createHint')}</p>
      <label className="field">
        <span>{t('instance.name')}</span>
        <input
          value={name}
          onChange={(event) => setName(event.target.value)}
          maxLength={80}
          required
        />
      </label>
      <label className="field">
        <span>{t('content.version')}</span>
        <select
          value={version.id}
          onChange={(event) => {
            setSelected(event.target.value);
            setMinecraft('');
          }}
        >
          {versions.map((v) => (
            <option value={v.id} key={v.id}>
              {v.version_number} · {v.version_type}
            </option>
          ))}
        </select>
      </label>
      <div className="content-create-grid">
        <label className="field">
          <span>{t('content.minecraft')}</span>
          <select value={game} onChange={(event) => setMinecraft(event.target.value)}>
            {games.map((v) => (
              <option key={v}>{v}</option>
            ))}
          </select>
        </label>
        <label className="field">
          <span>{t('instance.loader')}</span>
          <select
            value={chosenLoader}
            onChange={(event) => setLoader(event.target.value as Loader)}
          >
            {loaders.map((v) => (
              <option key={v} value={v}>
                {loaderNames[v]}
              </option>
            ))}
          </select>
        </label>
      </div>
    </fieldset>
  );
}
