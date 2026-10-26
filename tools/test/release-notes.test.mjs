import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { renderReleaseNotes } from '../render-release-notes.mjs';
import { ONE_LINE_DISCLAIMER } from '../lib/constants.mjs';

const lock = JSON.parse(fs.readFileSync(new URL('../binaries.lock.json', import.meta.url), 'utf8'));

test('pre-hardware-pass notes say beta per OS and carry the disclaimer', () => {
  const md = renderReleaseNotes({ version: 'v0.1.0', verification: { macos: null, windows: null, linux: null }, lock });
  assert.match(md, /^## Inkroot v0\.1\.0/m);
  assert.ok(md.includes(ONE_LINE_DISCLAIMER));
  for (const os of ['macOS', 'Windows', 'Linux']) assert.ok(md.includes(`beta — not yet hardware-verified on ${os}`));
  for (const n of ['Inkroot-macOS-AppleSilicon.dmg','Inkroot-macOS-Intel.dmg','Inkroot-Windows-x64-setup.exe','Inkroot-Linux-x86_64.AppImage','SHA256SUMS.txt']) assert.ok(md.includes(n));
  assert.ok(md.includes('Magisk 30.7') && md.includes('2f8e89a848afaaef68997fcbcb5b178d958d497b') && md.includes('r37.0.1'));
  assert.ok(md.includes('shasum -a 256 -c --ignore-missing SHA256SUMS.txt') && md.includes('THIRD_PARTY_NOTICES.md'));
  assert.ok(!md.includes('{{'));
});
test('verified OS shows the date, others stay beta', () => {
  const md = renderReleaseNotes({ version: 'v0.2.0', verification: { macos: '2026-11-02', windows: null, linux: null }, lock });
  assert.ok(md.includes('hardware-verified on macOS (2026-11-02, Note Max, firmware TKQ1.230721.002)'));
  assert.ok(md.includes('beta — not yet hardware-verified on Windows'));
  assert.ok(!md.includes('beta — not yet hardware-verified on macOS'));
});
