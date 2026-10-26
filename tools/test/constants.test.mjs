import { test } from 'node:test';
import assert from 'node:assert/strict';
import { ONE_LINE_DISCLAIMER, RELEASE_ASSETS } from '../lib/constants.mjs';

test('ONE_LINE_DISCLAIMER is the plan text verbatim', () => {
  assert.equal(
    ONE_LINE_DISCLAIMER,
    "Use at your own risk: modifying your tablet's software can brick it or void its warranty. Inkroot is provided as-is, with no warranty, and is not affiliated with Onyx/BOOX. Read the full [disclaimer](https://github.com/n0dal/inkroot/blob/main/DISCLAIMER.md).",
  );
});

test('RELEASE_ASSETS are the four stable names', () => {
  assert.deepEqual(Object.values(RELEASE_ASSETS).sort(), [
    'Inkroot-Linux-x86_64.AppImage',
    'Inkroot-Windows-x64-setup.exe',
    'Inkroot-macOS-AppleSilicon.dmg',
    'Inkroot-macOS-Intel.dmg',
  ]);
  assert.ok(Object.isFrozen(RELEASE_ASSETS));
});
