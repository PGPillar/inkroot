import { test } from 'node:test';
import assert from 'node:assert/strict';
import { STATIC_LINKS, pickAssets, detectOS, init } from '../../site/js/downloads.js';
import { RELEASE_ASSETS } from '../lib/constants.mjs';

const KEYS = ['mac-arm', 'mac-intel', 'windows', 'linux'];

// Real browser strings (user agent + navigator.platform) as of 2026.
const UA = {
  winChrome: 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36',
  winEdge: 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36 Edg/141.0.0.0',
  linuxFirefox: 'Mozilla/5.0 (X11; Linux x86_64; rv:144.0) Gecko/20100101 Firefox/144.0',
  ubuntuFirefox: 'Mozilla/5.0 (X11; Ubuntu; Linux x86_64; rv:144.0) Gecko/20100101 Firefox/144.0',
  macSafari: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/26.0 Safari/605.1.15',
  macChrome: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36',
  iPhone: 'Mozilla/5.0 (iPhone; CPU iPhone OS 18_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.6 Mobile/15E148 Safari/604.1',
  iPad: 'Mozilla/5.0 (iPad; CPU OS 18_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.6 Mobile/15E148 Safari/604.1',
  android: 'Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Mobile Safari/537.36',
  chromeOS: 'Mozilla/5.0 (X11; CrOS x86_64 14541.0.0) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36',
};

test('pickAssets maps stable names and sizes', () => {
  const r = pickAssets({ tag_name: 'v0.1.0', assets: [
    { name: 'Inkroot-macOS-AppleSilicon.dmg', browser_download_url: 'https://x/a.dmg', size: 12_582_912 }] });
  assert.equal(r.version, 'v0.1.0');
  assert.deepEqual(r.links['mac-arm'], { url: 'https://x/a.dmg', sizeMB: 12 });
  assert.equal(r.links.windows.url, STATIC_LINKS.windows); // missing asset → static fallback
  assert.equal(r.links.windows.sizeMB, null);
});

test('pickAssets ignores unrelated assets and rounds sizes to one decimal', () => {
  const r = pickAssets({ tag_name: 'v0.2.0', assets: [
    { name: 'SHA256SUMS.txt', browser_download_url: 'https://x/sums', size: 400 },
    { name: 'Inkroot-Linux-x86_64.AppImage', browser_download_url: 'https://x/l.AppImage', size: 95_000_000 }] });
  assert.deepEqual(r.links.linux, { url: 'https://x/l.AppImage', sizeMB: 90.6 });
  assert.equal(Object.keys(r.links).length, 4);
});

test('pickAssets(null) (rate-limited / no release) falls back to static links', () => {
  for (const bad of [null, undefined, {}, { tag_name: 'v1', assets: 'nope' }]) {
    const r = pickAssets(bad);
    for (const k of KEYS) assert.equal(r.links[k].url, STATIC_LINKS[k]);
  }
  assert.equal(pickAssets(null).version, null);
});

test('STATIC_LINKS use releases/latest/download and exact stable names', () => {
  assert.equal(STATIC_LINKS.linux, 'https://github.com/n0dal/inkroot/releases/latest/download/Inkroot-Linux-x86_64.AppImage');
  assert.equal(STATIC_LINKS['mac-intel'], 'https://github.com/n0dal/inkroot/releases/latest/download/Inkroot-macOS-Intel.dmg');
  for (const k of KEYS) assert.ok(STATIC_LINKS[k].endsWith('/releases/latest/download/' + RELEASE_ASSETS[k]));
});

test('detectOS: Windows and Linux desktops', () => {
  assert.equal(detectOS(UA.winChrome, 'Win32'), 'windows');
  assert.equal(detectOS(UA.winEdge, 'Win32'), 'windows');
  assert.equal(detectOS(UA.linuxFirefox, 'Linux x86_64'), 'linux');
  assert.equal(detectOS(UA.ubuntuFirefox, 'Linux x86_64'), 'linux');
});

test('detectOS: every Mac reports "MacIntel", so a Mac is mac-arm unless the CPU architecture says x86', () => {
  assert.equal(detectOS(UA.macSafari, 'MacIntel'), 'mac-arm');
  assert.equal(detectOS(UA.macChrome, 'MacIntel'), 'mac-arm');
  assert.equal(detectOS(UA.macChrome, 'MacIntel', 'arm'), 'mac-arm');
  assert.equal(detectOS(UA.macChrome, 'MacIntel', 'x86'), 'mac-intel'); // Chromium userAgentData.architecture
});

test('detectOS: phones, tablets and ChromeOS get no recommendation', () => {
  assert.equal(detectOS(UA.iPhone, 'iPhone'), null);
  assert.equal(detectOS(UA.iPad, 'MacIntel'), null);
  assert.equal(detectOS(UA.android, 'Linux armv81'), null);
  assert.equal(detectOS(UA.chromeOS, 'Linux x86_64'), null);
  assert.equal(detectOS('', ''), null);
});

// Minimal DOM stand-in: just the bits init() touches.
function fakeDoc() {
  const el = (attrs) => ({
    attrs: { ...attrs }, textContent: '', hidden: 'hidden' in attrs,
    getAttribute(n) { return n in this.attrs ? this.attrs[n] : null; },
    setAttribute(n, v) { this.attrs[n] = String(v); },
    hasAttribute(n) { return n in this.attrs; },
    get href() { return this.attrs.href; },
    set href(v) { this.attrs.href = v; },
  });
  const links = KEYS.map((k) => el({ 'data-dl': k, href: STATIC_LINKS[k] }));
  const sizes = KEYS.map((k) => el({ 'data-size': k }));
  const version = el({ 'data-version': '' });
  const versionLine = el({ 'data-version-line': '', hidden: '' });
  const all = { 'a[data-dl]': links, '[data-size]': sizes, '[data-version]': [version], '[data-version-line]': [versionLine] };
  return { links, sizes, version, versionLine, querySelectorAll: (s) => all[s] ?? [] };
}
const nav = (userAgent, platform) => ({ userAgent, platform });
const hrefs = (doc) => Object.fromEntries(doc.links.map((a) => [a.getAttribute('data-dl'), a.href]));

test('init survives a failed fetch (403 rate limit) and leaves static hrefs', async () => {
  const doc = fakeDoc();
  let calls = 0;
  const fetch403 = async () => { calls++; return { ok: false, status: 403, json: async () => ({ message: 'API rate limit exceeded' }) }; };
  await init(doc, fetch403, nav(UA.winChrome, 'Win32'));
  assert.equal(calls, 1);
  assert.deepEqual(hrefs(doc), { ...STATIC_LINKS });
  assert.ok(doc.sizes.every((s) => s.textContent === ''));
  assert.equal(doc.versionLine.hidden, true);
  // The OS recommendation doesn't need the API, so it still happens.
  assert.deepEqual(doc.links.map((a) => a.hasAttribute('data-primary')), [false, false, true, false]);
});

test('init survives a rejected fetch (offline) and leaves static hrefs', async () => {
  const doc = fakeDoc();
  await init(doc, async () => { throw new TypeError('Failed to fetch'); }, nav(UA.linuxFirefox, 'Linux x86_64'));
  assert.deepEqual(hrefs(doc), { ...STATIC_LINKS });
  assert.equal(doc.versionLine.hidden, true);
});

test('init on success sets hrefs, sizes, version and marks the detected OS', async () => {
  const doc = fakeDoc();
  const release = { tag_name: 'v0.1.0', assets: [
    { name: 'Inkroot-Windows-x64-setup.exe', browser_download_url: 'https://github.com/n0dal/inkroot/releases/download/v0.1.0/Inkroot-Windows-x64-setup.exe', size: 31_457_280 }] };
  let url;
  await init(doc, async (u) => { url = u; return { ok: true, status: 200, json: async () => release }; }, nav(UA.winChrome, 'Win32'));
  assert.equal(url, 'https://api.github.com/repos/n0dal/inkroot/releases/latest');
  assert.equal(doc.links[2].href, release.assets[0].browser_download_url);
  assert.equal(doc.links[0].href, STATIC_LINKS['mac-arm']);
  assert.equal(doc.sizes[2].textContent, '30 MB');
  assert.equal(doc.sizes[0].textContent, '');
  assert.equal(doc.version.textContent, 'v0.1.0');
  assert.equal(doc.versionLine.hidden, false);
  assert.deepEqual(doc.links.map((a) => a.hasAttribute('data-primary')), [false, false, true, false]);
});
