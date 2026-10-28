// Signs every nested Mach-O under a staged dir (Developer ID + hardened runtime + timestamp) so notarization passes.
// Symlinks are never followed or signed; their targets are regular files signed once.
// Versioned frameworks (e.g. a framework-build Python's Python.framework) are signed as bundles via
// Foo.framework/Versions/<ver> after their inner Mach-Os, as Apple recommends; the Current/top-level links are left alone.
import { execFile } from "node:child_process";
import fsp from "node:fs/promises";
import path from "node:path";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";

const run = promisify(execFile);
const here = path.dirname(fileURLToPath(import.meta.url));
export const EDL_ENTITLEMENTS = path.join(here, "..", "macos", "entitlements.edl.plist");

const MACHO = new Set(["feedface", "feedfacf", "cefaedfe", "cffaedfe"]);

async function isMachO(p) {
  if (!(await fsp.lstat(p)).isFile()) return false; // never read through a symlink or a directory
  const fh = await fsp.open(p, "r");
  try {
    const b = Buffer.alloc(8);
    const { bytesRead } = await fh.read(b, 0, 8, 0);
    if (bytesRead < 8) return false;
    const magic = b.subarray(0, 4).toString("hex");
    if (MACHO.has(magic)) return true;
    // Fat binaries and Java classes share cafebabe; fat has a small arch count, Java has a major version >= 45.
    return magic === "cafebabe" && b.readUInt32BE(4) < 45;
  } finally { await fh.close(); }
}

async function walk(dir, out) {
  for (const e of await fsp.readdir(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isSymbolicLink()) continue;
    if (e.isDirectory()) { if (e.name.endsWith(".framework")) out.frameworks.push(p); await walk(p, out); }
    else if (e.isFile() && (await isMachO(p))) out.files.push(p);
  }
  return out;
}

const depth = (p) => p.split(path.sep).length;
const deepestFirst = (a, b) => depth(b) - depth(a) || (a < b ? -1 : 1);

/** Regular Mach-O files under dir (symlinks skipped), deepest first. */
export async function findMachO(dir) {
  return (await walk(dir, { files: [], frameworks: [] })).files.sort(deepestFirst);
}

/** Real (non-symlink) version dirs under fw/Versions, or [fw] for an unversioned (shallow) framework. */
async function frameworkBundles(fw) {
  const versions = path.join(fw, "Versions");
  const st = await fsp.lstat(versions).catch(() => null);
  if (!st?.isDirectory()) return [fw];
  const out = [];
  for (const e of await fsp.readdir(versions, { withFileTypes: true })) if (e.isDirectory()) out.push(path.join(versions, e.name));
  return out;
}

/** Signable bundle paths of every framework under dir (Versions/<ver> or the shallow .framework), deepest first. */
export async function findFrameworkBundles(dir) {
  const { frameworks } = await walk(dir, { files: [], frameworks: [] });
  const out = [];
  for (const fw of frameworks) out.push(...(await frameworkBundles(fw)));
  return out.sort(deepestFirst);
}

/** Only edl's main executable gets entitlements (PyInstaller bootloader loads its own unsigned-team libs). */
export const defaultEntitlementsFor = (root) => (p) =>
  path.relative(root, p).split(path.sep).join("/") === "tools/edl/edl" ? EDL_ENTITLEMENTS : null;

export async function signAll(dir, identity, { entitlementsFor = defaultEntitlementsFor(dir), log = (m) => console.error(m) } = {}) {
  if (!identity) { log("APPLE_SIGNING_IDENTITY not set; skipping macOS signing"); return; }
  const stamp = identity === "-" ? "--timestamp=none" : "--timestamp"; // ad-hoc is local-only (tests)
  const files = await findMachO(dir);
  const bundles = await findFrameworkBundles(dir);
  const sign = (p, ent) => run("codesign", ["--force", stamp, "--options", "runtime", "--sign", identity, ...(ent ? ["--entitlements", ent] : []), p]);
  for (const f of files) await sign(f, entitlementsFor(f));
  for (const b of bundles) await sign(b, null);
  log(`signed ${files.length} Mach-O file(s) and ${bundles.length} framework bundle(s)`);
}

/** Returns {ok, problems}. teamId defaults to env APPLE_TEAM_ID. */
export async function verifyAll(dir, teamId = process.env.APPLE_TEAM_ID) {
  const problems = [];
  if (!teamId) problems.push("APPLE_TEAM_ID is not set; cannot check TeamIdentifier");
  for (const f of [...(await findMachO(dir)), ...(await findFrameworkBundles(dir))]) {
    const rel = path.relative(dir, f);
    let info;
    try { info = (await run("codesign", ["-dv", "--verbose=4", f])).stderr; }
    catch (e) { problems.push(`${rel}: not signed (${(e.stderr || e.message).trim().split("\n")[0]})`); continue; }
    if (teamId && !new RegExp(`^TeamIdentifier=${teamId}$`, "m").test(info)) problems.push(`${rel}: TeamIdentifier is not ${teamId}`);
    if (!/^Timestamp=/m.test(info)) problems.push(`${rel}: no secure timestamp`);
    if (!/flags=\S*\([^)]*\bruntime\b/.test(info)) problems.push(`${rel}: hardened runtime flag missing`);
    try { await run("codesign", ["--verify", "--strict", f]); }
    catch (e) { problems.push(`${rel}: codesign --verify failed (${(e.stderr || e.message).trim().split("\n")[0]})`); }
  }
  return { ok: problems.length === 0, problems };
}
