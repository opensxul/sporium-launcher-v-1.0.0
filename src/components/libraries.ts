import { Anvil, Clapperboard, Flame, Layers3, Leaf, Radio } from 'lucide-react';

export const libraries = [
  { id: 'vanilla', name: 'Vanilla', icon: Leaf },
  { id: 'studio', name: 'Creator Studio', icon: Clapperboard },
  { id: 'fabric', name: 'Fabric', icon: Layers3 },
  { id: 'forge', name: 'Forge', icon: Anvil },
  { id: 'neoforge', name: 'NeoForge', icon: Flame },
  { id: 'managed', name: 'Zero Signal', icon: Radio },
] as const;

export type Library = (typeof libraries)[number];
