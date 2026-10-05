import type { Locale } from '../bindings/core';
import { en, ru } from './messages';
import type { MessageKey } from './messages';

export function translate(locale: Locale, key: MessageKey): string {
  return locale === 'en-US' ? en[key] : ru[key];
}

export type Translate = (key: MessageKey) => string;
