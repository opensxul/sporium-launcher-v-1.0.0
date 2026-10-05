import { describe, expect, it } from 'vitest';
import { BackendError, normalizeError } from './backend';

describe('IPC error boundary', () => {
  it('never exposes raw failures or private paths to UI', () => {
    for (const value of [
      new Error('C:/Users/private/token'),
      'secret-value',
      null,
      { code: 'FAKE', details: 'sensitive' },
    ]) {
      const error = normalizeError(value);
      expect(error.code).toBe('UNKNOWN');
      expect(error.message).toBe('UNKNOWN');
    }
  });

  it('retains supported actionable error codes only', () => {
    expect(normalizeError({ code: 'SCHEMA_TOO_NEW', retryable: false })).toEqual(
      new BackendError('SCHEMA_TOO_NEW', false),
    );
    expect(normalizeError({ code: 'SETTINGS_CONFLICT', retryable: true }).retryable).toBe(true);
  });
});
