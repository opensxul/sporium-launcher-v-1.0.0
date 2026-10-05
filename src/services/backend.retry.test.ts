import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
const ipc = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({ isTauri: () => true, invoke: ipc.invoke }));
import { backend } from './backend';

describe('library lock contention', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    ipc.invoke.mockReset();
  });
  afterEach(() => vi.useRealTimers());
  it('contextual catalogue reads tolerate overlapping instance reads without retrying provider failures', async () => {
    const query = {
      query: '',
      kind: 'resourcepack',
      minecraft: '',
      loader: '',
      category: '',
      environment: '',
      sort: 'relevance',
      offset: 0,
      instanceId: 'test',
    };
    ipc.invoke
      .mockRejectedValueOnce({ code: 'LIBRARY_BUSY', retryable: true })
      .mockResolvedValueOnce({ hits: [], total: 0, nextOffset: null, cached: false });
    const result = backend.contentSearch(query);
    await vi.advanceTimersByTimeAsync(250);
    await expect(result).resolves.toMatchObject({ total: 0 });
    expect(ipc.invoke).toHaveBeenCalledTimes(2);
    ipc.invoke.mockReset().mockRejectedValue({ code: 'NETWORK', retryable: true });
    await expect(backend.contentDetails('project', 'test')).rejects.toBeDefined();
    expect(ipc.invoke).toHaveBeenCalledTimes(1);
  });
  it('retries pre-mutation library contention and returns the committed result', async () => {
    ipc.invoke
      .mockRejectedValueOnce({ code: 'LIBRARY_BUSY', retryable: true })
      .mockRejectedValueOnce({ code: 'LIBRARY_BUSY', retryable: true })
      .mockResolvedValueOnce({ affectedId: 'new' });
    const result = backend.createInstance({
      name: 'Test',
      minecraftVersion: '1.21.1',
      loader: 'fabric',
      collectionId: null,
    });
    await vi.advanceTimersByTimeAsync(500);
    await expect(result).resolves.toEqual({ affectedId: 'new' });
    expect(ipc.invoke).toHaveBeenCalledTimes(3);
  });
  it('does not retry a stale revision or an instance held by a running game', async () => {
    for (const code of ['RECORD_CONFLICT', 'INSTANCE_BUSY']) {
      ipc.invoke.mockReset().mockRejectedValue({ code, retryable: true });
      await expect(
        backend.duplicateInstance({ id: 'test', name: 'Copy', expectedRevision: 1 }),
      ).rejects.toMatchObject({ code });
      expect(ipc.invoke).toHaveBeenCalledTimes(1);
    }
  });
  it('reports a persistently busy library after the bounded wait', async () => {
    ipc.invoke.mockRejectedValue({ code: 'LIBRARY_BUSY', retryable: true });
    const result = expect(backend.librarySnapshot()).rejects.toMatchObject({
      code: 'LIBRARY_BUSY',
    });
    await vi.advanceTimersByTimeAsync(3000);
    await result;
    expect(ipc.invoke).toHaveBeenCalledTimes(13);
  });
  it('launch waits for a short content read but sends no further request after a successful start', async () => {
    ipc.invoke
      .mockRejectedValueOnce({ code: 'INSTANCE_BUSY', retryable: true })
      .mockResolvedValueOnce({ job: { phase: 'resolving' }, sessions: [] });
    const result = backend.startGame({ id: 'test', action: 'local' });
    await vi.advanceTimersByTimeAsync(250);
    await expect(result).resolves.toMatchObject({ job: { phase: 'resolving' } });
    expect(ipc.invoke).toHaveBeenCalledTimes(2);
  });
  it('launch contention is bounded and incompatible game errors are not retried', async () => {
    ipc.invoke.mockRejectedValue({ code: 'INSTANCE_BUSY', retryable: true });
    const result = expect(backend.startGame({ id: 'test', action: 'local' })).rejects.toMatchObject(
      { code: 'INSTANCE_BUSY' },
    );
    await vi.advanceTimersByTimeAsync(60000);
    await result;
    expect(ipc.invoke).toHaveBeenCalledTimes(241);
    ipc.invoke.mockReset().mockRejectedValue({ code: 'CONTENT_INCOMPATIBLE', retryable: false });
    await expect(backend.startGame({ id: 'test', action: 'local' })).rejects.toMatchObject({
      code: 'CONTENT_INCOMPATIBLE',
    });
    expect(ipc.invoke).toHaveBeenCalledTimes(1);
  });
  it('large-pack verification can exceed the former three-second launch wait', async () => {
    ipc.invoke.mockRejectedValue({ code: 'INSTANCE_BUSY', retryable: true });
    const result = backend.startGame({ id: 'large-pack', action: 'local' });
    await vi.advanceTimersByTimeAsync(5000);
    ipc.invoke.mockResolvedValue({ job: { phase: 'resolving' }, sessions: [] });
    await vi.advanceTimersByTimeAsync(250);
    await expect(result).resolves.toMatchObject({ job: { phase: 'resolving' } });
    expect(ipc.invoke).toHaveBeenCalledTimes(22);
  });
});
