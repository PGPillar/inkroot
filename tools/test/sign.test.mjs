import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { findMachO, signAll, verifyAll } from '../lib/sign-macos.mjs';

const tmp = () => fs.mkdtempSync(path.join(os.tmpdir(), 'inkroot-sign-'));
const put = (root, rel, bytes) => { const p = path.join(root, rel); fs.mkdirSync(path.dirname(p), { recursive: true }); fs.writeFileSync(p, Buffer.from(bytes)); return p; };
const hex = (s) => Buffer.from(s, 'hex');

test('findMachO lists thin/fat Mach-O only, deepest first, ignoring symlinks', async () => {
  const d = tmp();
  const edl = put(d, 'tools/edl/edl', Buffer.concat([hex('cffaedfe'), Buffer.alloc(16)]));
  const fat = put(d, 'tools/adb', Buffer.concat([hex('cafebabe'), hex('00000002'), Buffer.alloc(16)]));
  const dylib = put(d, 'tools/edl/_internal/lib/libusb.dylib', Buffer.concat([hex('cffaedfe'), Buffer.alloc(16)]));
  put(d, 'legal/NOTICE.txt', 'hello world, plain text');
  put(d, 'Foo.class', Buffer.concat([hex('cafebabe'), hex('00000034'), Buffer.alloc(16)]));
  fs.symlinkSync('lib/libusb.dylib', path.join(d, 'tools/edl/_internal/libusb-link.dylib'));
  fs.symlinkSync('../../..', path.join(d, 'tools/edl/_internal/dirlink'));
  const found = await findMachO(d);
  assert.deepEqual([...found].sort(), [edl, fat, dylib].sort());
  assert.equal(found[0], dylib);
});

test('signAll with no identity skips with a log line', async () => {
  const logs = [];
  await signAll(tmp(), '', { log: (m) => logs.push(m) });
  assert.match(logs[0], /skipping/);
});

test('ad-hoc signing a copy of the real edl build verifies; verifyAll rejects missing TeamIdentifier', { skip: process.platform !== 'darwin' && 'codesign needs macOS' }, async (t) => {
  const src = new URL('../.build/darwin-aarch64/edl', import.meta.url).pathname;
  const d = tmp();
  if (fs.existsSync(src)) fs.cpSync(src, path.join(d, 'tools/edl'), { recursive: true, verbatimSymlinks: true });
  else { fs.mkdirSync(path.join(d, 'tools/edl'), { recursive: true }); fs.copyFileSync('/usr/bin/true', path.join(d, 'tools/edl/edl')); }
  await signAll(d, '-', { log: () => {} });
  const files = await findMachO(d);
  assert.ok(files.length >= 1);
  for (const f of files) execFileSync('codesign', ['--verify', '--strict', f], { stdio: 'pipe' });
  if (fs.existsSync(path.join(d, 'tools/edl/_internal'))) execFileSync('codesign', ['--verify', '--deep', '--strict', path.join(d, 'tools/edl/edl')], { stdio: 'pipe' });
  const r = await verifyAll(d, 'ABCDE12345');
  assert.equal(r.ok, false);
  assert.ok(r.problems.some((p) => /TeamIdentifier/.test(p)));
  assert.ok(r.problems.some((p) => /timestamp/.test(p)));
  assert.ok(!r.problems.some((p) => /runtime/.test(p)), 'hardened runtime flag was applied');
});
