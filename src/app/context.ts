import { createContext, useContext } from 'react';
import type { Bootstrap, Settings, SettingsSnapshot } from '../bindings/core';
import type { Translate } from '../i18n';

export interface FoundationContextValue {
  data: Bootstrap;
  desktop: boolean;
  t: Translate;
  saveSettings: (values: Settings) => Promise<SettingsSnapshot>;
  reload: () => Promise<SettingsSnapshot>;
}

export const FoundationContext = createContext<FoundationContextValue | null>(null);

export function useFoundation() {
  const context = useContext(FoundationContext);
  if (!context) throw new Error('Foundation context missing');
  return context;
}
