// Download buttons: static links that always work, upgraded from the GitHub API when it answers.
// No DOM access at import time, so tools/test can import this module in Node.
// The build rewrites this import to ./constants.mjs and copies the file next to this one.
import { RELEASE_ASSETS } from '../../tools/lib/constants.mjs';

const REPO = 'https://github.com/n0dal/inkroot';
const API_LATEST = 'https://api.github.com/repos/n0dal/inkroot/releases/latest';
const TIMEOUT_MS = 5000;

/** The `releases/latest/download/<stable-name>` URL for each build. */
export const STATIC_LINKS = Object.freeze(Object.fromEntries(
  Object.entries(RELEASE_ASSETS).map(([key, name]) => [key, `${REPO}/releases/latest/download/${name}`]),
));

/**
 * Map a GitHub "latest release" payload to one link per build. Anything missing or malformed
 * falls back to the static link, so every key always has a usable URL.
 */
export function pickAssets(release) {
  const assets = Array.isArray(release?.assets) ? release.assets : [];
  const links = {};
  for (const [key, name] of Object.entries(RELEASE_ASSETS)) {
    const a = assets.find((x) => x && x.name === name);
    const ok = a && typeof a.browser_download_url === 'string' && a.browser_download_url.startsWith('https://');
    links[key] = ok
      ? { url: a.browser_download_url, sizeMB: Number.isFinite(a.size) ? Math.round(a.size / 104_857.6) / 10 : null }
      : { url: STATIC_LINKS[key], sizeMB: null };
  }
  const version = assets.length && typeof release.tag_name === 'string' ? release.tag_name : null;
  return { version, links };
}

/**
 * Best guess at which build fits this computer, or null (phones, tablets, ChromeOS, unknown).
 * Every Mac browser reports navigator.platform "MacIntel", and Safari can't tell Apple Silicon
 * from Intel at all, so a Mac is "mac-arm" unless `arch` (Chromium's
 * userAgentData.architecture) says "x86". Both Mac buttons are always shown.
 */
export function detectOS(ua, platform = '', arch = '') {
  ua = String(ua || '');
  if (/iPhone|iPad|iPod|Android|CrOS/i.test(ua)) return null;
  if (/Windows NT/i.test(ua)) return 'windows';
  if (/Macintosh|Mac OS X/i.test(ua)) return /^x86/i.test(arch) ? 'mac-intel' : 'mac-arm';
  if (/Linux|X11/i.test(ua) && !/aarch64|arm/i.test(`${ua} ${platform}`)) return 'linux';
  return null;
}

async function cpuArch(nav) {
  try {
    const v = await nav?.userAgentData?.getHighEntropyValues?.(['architecture']);
    return v?.architecture || '';
  } catch {
    return '';
  }
}

async function latestRelease(fetchImpl) {
  const ctrl = typeof AbortController === 'function' ? new AbortController() : null;
  const timer = ctrl && setTimeout(() => ctrl.abort(), TIMEOUT_MS);
  try {
    const res = await fetchImpl(API_LATEST, { headers: { Accept: 'application/vnd.github+json' }, signal: ctrl?.signal });
    return res && res.ok ? await res.json() : null;
  } catch {
    return null; // offline, blocked, timed out or rate-limited: the static links stay
  } finally {
    if (timer) clearTimeout(timer);
  }
}

/**
 * Wire the download buttons (`a[data-dl]`). The static hrefs in the HTML already work; this only
 * recommends the build for this computer and, if the API answers, fills in direct links, sizes
 * and the version. Any failure leaves the page exactly as it was.
 */
export async function init(doc, fetchImpl = globalThis.fetch, nav = globalThis.navigator) {
  const buttons = [...doc.querySelectorAll('a[data-dl]')];
  if (!buttons.length) return;

  // iPadOS Safari sends a Mac user agent; a touch screen gives it away.
  const iPad = nav?.platform === 'MacIntel' && nav?.maxTouchPoints > 1;
  const os = iPad ? null : detectOS(nav?.userAgent, nav?.platform, await cpuArch(nav));
  for (const a of buttons) if (os && a.getAttribute('data-dl') === os) a.setAttribute('data-primary', '');

  if (typeof fetchImpl !== 'function') return;
  const release = await latestRelease(fetchImpl);
  if (!release) return;
  const { version, links } = pickAssets(release);
  if (!version) return;

  for (const a of buttons) {
    const link = links[a.getAttribute('data-dl')];
    if (link) a.href = link.url;
  }
  for (const s of doc.querySelectorAll('[data-size]')) {
    const mb = links[s.getAttribute('data-size')]?.sizeMB;
    if (mb != null) s.textContent = `${mb} MB`;
  }
  for (const v of doc.querySelectorAll('[data-version]')) v.textContent = version;
  for (const line of doc.querySelectorAll('[data-version-line]')) line.hidden = false;
}
