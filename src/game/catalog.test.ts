import { describe, expect, it } from 'vitest';
import type { GameVersion } from '../bindings/core';
import { visibleVersions } from './catalog';

describe('official version visibility', () => {
  const versions: GameVersion[] = [
    { id: '26.3', kind: 'release', releasedAt: '' },
    { id: '26.4-snapshot-1', kind: 'snapshot', releasedAt: '' },
    { id: 'b1.7.3', kind: 'old_beta', releasedAt: '' },
    { id: 'a1.2.6', kind: 'old_alpha', releasedAt: '' },
    { id: 'future-format', kind: 'other', releasedAt: '' },
  ];
  it('hides experimental and historical versions by default without dropping them from the catalog', () => {
    const defaults = { releases: true, snapshots: false, beta: false, alpha: false, other: false };
    expect(visibleVersions(versions, defaults).map((version) => version.id)).toEqual(['26.3']);
    expect(
      visibleVersions(versions, { ...defaults, releases: false, beta: true, alpha: true }).map(
        (version) => version.id,
      ),
    ).toEqual(['b1.7.3', 'a1.2.6']);
    expect(versions).toHaveLength(5);
  });
});
