import type { Instance, Loader } from '../bindings/core';

export const loaderNames: Record<Loader, string> = {
  vanilla: 'Vanilla',
  fabric: 'Fabric',
  forge: 'Forge',
  neo_forge: 'NeoForge',
};

export function belongsToLibrary(instance: Instance, id: string): boolean {
  const aliases: Record<string, string> = {
    studio: 'creator_studio',
    managed: 'managed_project',
    neoforge: 'neo_forge',
  };
  return instance.instanceType === (aliases[id] ?? id);
}

export function libraryLoader(id: string | undefined): Loader | undefined {
  if (id === 'neoforge') return 'neo_forge';
  if (id === 'vanilla' || id === 'fabric' || id === 'forge') return id;
  return undefined;
}
