import { describe, expect, it } from 'vitest';
import * as links from './links';
import linksSrc from './links.ts?raw';

// Repo-root files (basename -> exists), resolved at build time.
const repoFiles = import.meta.glob(['../../../*.md', '../../../boot-service/*.md'], {
  query: '?url',
  eager: false
});

describe('links', () => {
  it('every GitHub blob link points at an existing repo file', () => {
    const blobs = Object.values(links)
      .filter((v): v is string => typeof v === 'string')
      .filter((u) => u.includes('/blob/main/'));
    expect(blobs.length).toBeGreaterThanOrEqual(3);
    for (const u of blobs) {
      const path = u.split('/blob/main/')[1];
      expect(path, u).toMatch(/\.md$/);
      expect(Object.keys(repoFiles), u).toContain('../../../' + path);
    }
  });
  it('links.ts source has no extensionless blob link', () => {
    for (const m of linksSrc.matchAll(/\/blob\/main\/([^']+)'/g)) expect(m[1]).toMatch(/\.md$/);
  });
});
