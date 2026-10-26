import { expect, test } from 'vitest';
import { backupLine, formatBytes, formatChip, revealLabel, shortSha } from './format';
import { NOTE_MAX, NOTE_MAX_ROOTED, ROOTED_BACKUP, STOCK_BACKUP } from './mock/fixtures';

test('formatBytes is 1024-based with one decimal', () => {
  expect(formatBytes(100663296)).toBe('96.0 MB');
  expect(formatBytes(65536)).toBe('64.0 KB');
  expect(formatBytes(3 * 1024 ** 3)).toBe('3.0 GB');
  expect(formatBytes(512)).toBe('512 B');
});

test('shortSha keeps the first 4 and last 3', () => {
  expect(shortSha('2f05dc1bb3d6df0da5e5267cfc7ff1f52e568f43c9ff09d0ee04aa97b9b0c8fc')).toBe('2f05…8fc');
});

test('backupLine', () => {
  expect(backupLine(STOCK_BACKUP)).toBe('backup ✓ boot_b.img · 96.0 MB · sha 2f05…8fc');
  expect(backupLine(ROOTED_BACKUP)).toBe('backup · not original · boot_b.img · 96.0 MB · sha 8b2e…b13');
  expect(backupLine({ ...STOCK_BACKUP, verified: false })).toMatch(/^backup · not original · boot_b\.img/);
  expect(backupLine(null)).toBe("no backup yet · we'll make one first");
});

test('formatChip', () => {
  expect(formatChip({ kind: 'adb', info: NOTE_MAX, gate: null })).toBe('● NOTE MAX / FW 4.2 / SLOT_B');
  expect(formatChip({ kind: 'adb', info: NOTE_MAX_ROOTED, gate: null })).toBe('● NOTE MAX / FW 4.2 / SLOT_B / ROOT');
  expect(formatChip({ kind: 'adb', info: { ...NOTE_MAX, model: 'TabUltraCPro', profileId: null }, gate: 'unknownModel' })).toBe(
    '● TABULTRACPRO / UNSUPPORTED',
  );
  expect(formatChip({ kind: 'none' })).toBe('○ NO TABLET');
  expect(formatChip({ kind: 'unauthorized' })).toBe('◐ WAITING FOR ALLOW');
  expect(formatChip({ kind: 'edl' })).toBe('● RESCUE MODE');
});

test('revealLabel', () => {
  expect(revealLabel('macos')).toBe('Show in Finder');
  expect(revealLabel('windows')).toBe('Show in Explorer');
  expect(revealLabel('linux')).toBe('Show in folder');
});
