// Downloads, hash-verifies and stages third-party tools into the app's resources dir.
// Integrity is safety-critical: every problem is collected and reported together, and
// the output dir is only touched after everything verified.
import { createHash } from "node:crypto";
import fs from "node:fs";
import fsp from "node:fs/promises";
import path from "node:path";
import { unzipSync } from "fflate";

export const PLACEHOLDER = "INKROOT-PLACEHOLDER\n";
const TARGETS = ["darwin-aarch64", "darwin-x86_64", "windows-x86_64", "linux-x86_64"];
const PLAT = { darwin: "darwin", win32: "windows", windows: "windows", linux: "linux" };
const ARCH = { arm64: "aarch64", aarch64: "aarch64", x64: "x86_64", x86_64: "x86_64" };

/** Precedence: explicit flag > TAURI_ENV_PLATFORM/ARCH > host. */
export function resolveTarget(env, platform, arch, flag) {
  let t;
  if (flag) t = flag;
  else if (env.TAURI_ENV_PLATFORM || env.TAURI_ENV_ARCH) {
    t = `${env.TAURI_ENV_PLATFORM}-${env.TAURI_ENV_ARCH}`;
  } else t = `${PLAT[platform]}-${ARCH[arch]}`;
  if (!TARGETS.includes(t)) throw new Error(`Unsupported target: ${t}`);
  return t;
}

const platformOf = (target) => target.split("-")[0];
const isWin = (target) => platformOf(target) === "windows";
const fill = (s, target) => s.replaceAll("{target}", target);
export const edlBuildDir = (lock, target, repoRoot) => path.join(repoRoot, fill(lock.edl.buildDir, target));
export const edlManifestPath = (lock, target, repoRoot) => path.join(repoRoot, fill(lock.edl.manifest, target));
const edlBuildHint = (target) => `python tools/build_edl.py --target ${target}`;

export async function sha256File(p) {
  const h = createHash("sha256");
  for await (const chunk of fs.createReadStream(p)) h.update(chunk);
  return h.digest("hex");
}
const sha256Buf = (b) => createHash("sha256").update(b).digest("hex");

export function planStaging(lock, target, repoRoot) {
  if (!TARGETS.includes(target)) throw new Error(`Unsupported target: ${target}`);
  const plat = platformOf(target);
  const items = [];
  const root = (p) => path.join(repoRoot, p);
  items.push({ kind: "download", dest: `magisk/${lock.magisk.file}`, url: lock.magisk.url, sha256: lock.magisk.sha256, mode: 0o644 });
  items.push({ kind: "download", dest: `tools/loaders/${lock.loader.path}`, url: lock.loader.url, sha256: lock.loader.sha256, mode: 0o644 });
  const pt = lock.platformTools;
  const arc = pt.archives[plat];
  for (const m of pt.members[plat]) {
    const name = path.posix.basename(m);
    items.push({ kind: "zip-member", dest: `tools/${name}`, url: arc.url, sha256: arc.sha256, member: m, mode: /^adb(\.exe)?$/.test(name) ? 0o755 : 0o644 });
  }
  items.push({ kind: "zip-member", dest: "legal/platform-tools-NOTICE.txt", url: arc.url, sha256: arc.sha256, member: pt.members.notice, mode: 0o644 });
  items.push({ kind: "dir", dest: "tools/edl/", src: edlBuildDir(lock, target, repoRoot) });
  items.push({ kind: "copy", dest: "legal/edl-LICENSE.txt", src: path.join(edlBuildDir(lock, target, repoRoot), "LICENSE"), mode: 0o644 });
  if (plat === "windows") items.push({ kind: "copy", dest: "tools/wdi-simple.exe", src: root(lock.libwdi.build), mode: 0o644 });
  for (const f of lock.bootService.files) {
    items.push({ kind: "copy", dest: f, src: root(f), mode: f.endsWith(".sh") ? 0o755 : 0o644 });
  }
  items.push({ kind: "copy", dest: "legal/DISCLAIMER.md", src: root("DISCLAIMER.md"), mode: 0o644 });
  items.push({ kind: "copy", dest: "legal/THIRD_PARTY_NOTICES.md", src: root("THIRD_PARTY_NOTICES.md"), mode: 0o644 });
  items.push({ kind: "copy", dest: "legal/OFL.txt", src: root("tools/node_modules/@ibm/plex-sans/LICENSE.txt"), mode: 0o644 });
  return items;
}

async function download(item, { fetchImpl, cacheDir, log }) {
  const cached = cacheDir ? path.join(cacheDir, item.sha256) : null;
  if (cached && fs.existsSync(cached)) {
    const buf = await fsp.readFile(cached);
    if (sha256Buf(buf) === item.sha256) return buf;
    log(`cache entry ${item.sha256} is corrupt; re-downloading`);
    await fsp.rm(cached, { force: true });
  }
  log(`downloading ${item.url}`);
  const res = await fetchImpl(item.url, { redirect: "follow" });
  if (!res.ok) throw new Error(`${item.url}: HTTP ${res.status}`);
  const buf = Buffer.from(await res.arrayBuffer());
  const actual = sha256Buf(buf);
  if (actual !== item.sha256) {
    throw new Error(`${path.posix.basename(item.dest)} (${item.url}): hash mismatch. expected ${item.sha256}, actual ${actual}`);
  }
  if (cached) {
    await fsp.mkdir(cacheDir, { recursive: true });
    const tmp = `${cached}.tmp-${process.pid}`;
    await fsp.writeFile(tmp, buf);
    await fsp.rename(tmp, cached);
  }
  return buf;
}

const safeRel = (rel) => rel && !path.isAbsolute(rel) && !rel.split(/[\\/]/).includes("..");

/** Lists regular files and symlinks (never following links) under dir. */
async function scan(dir, base = dir, out = { files: [], links: {} }) {
  for (const e of await fsp.readdir(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    const rel = path.relative(base, p).split(path.sep).join("/");
    if (e.isSymbolicLink()) out.links[rel] = await fsp.readlink(p);
    else if (e.isDirectory()) await scan(p, base, out);
    else out.files.push(rel);
  }
  return out;
}

const linkEscapes = (dir, rel, target) =>
  path.isAbsolute(target) || !path.resolve(dir, path.dirname(rel), target).startsWith(path.resolve(dir) + path.sep);

async function hasSymlinkAncestor(dir, rel) {
  const parts = rel.split("/");
  let cur = dir;
  for (const part of parts.slice(0, -1)) {
    cur = path.join(cur, part);
    const st = await fsp.lstat(cur).catch(() => null);
    if (st?.isSymbolicLink()) return true;
  }
  return false;
}

/** Verify a directory (source onedir or staged copy) against the manifest. Returns problems[]. */
export async function verifyEdlTree(dir, manifest, target, label = "edl") {
  const hint = `rebuild with: ${edlBuildHint(target)}`;
  const problems = [];
  const files = manifest.files ?? {};
  const links = manifest.links ?? {};
  for (const [rel, expected] of Object.entries(files)) {
    if (!safeRel(rel)) { problems.push(`${label} manifest has unsafe path ${rel}`); continue; }
    const p = path.join(dir, rel);
    const st = await fsp.lstat(p).catch(() => null);
    if (!st) { problems.push(`${label} file ${rel}: missing (expected ${expected}); ${hint}`); continue; }
    if (!st.isFile() || (await hasSymlinkAncestor(dir, rel))) { problems.push(`${label} file ${rel}: is not a regular file (symlink or directory); ${hint}`); continue; }
    const actual = await sha256File(p);
    if (actual !== expected) problems.push(`${label} file ${rel}: hash mismatch. expected ${expected}, actual ${actual}; ${hint}`);
  }
  for (const [rel, target_] of Object.entries(links)) {
    if (!safeRel(rel) || typeof target_ !== "string") { problems.push(`${label} manifest has unsafe link ${rel}`); continue; }
    if (linkEscapes(dir, rel, target_)) { problems.push(`${label} link ${rel} -> ${target_}: escapes the edl dir`); continue; }
    const actual = await fsp.readlink(path.join(dir, rel)).catch(() => null);
    if (actual !== target_) problems.push(`${label} link ${rel}: expected -> ${target_}, actual ${actual === null ? "missing or not a link" : "-> " + actual}; ${hint}`);
  }
  const found = await scan(dir);
  for (const rel of found.files) if (!(rel in files)) problems.push(`${label} file ${rel}: present but not in manifest; ${hint}`);
  for (const [rel, t] of Object.entries(found.links)) {
    if (!(rel in links)) problems.push(`${label} link ${rel} -> ${t}: present but not in manifest; ${hint}`);
  }
  return problems;
}

/** Verify the PyInstaller onedir against its manifest. Returns {problems, manifest}. */
async function verifyEdl(lock, target, repoRoot) {
  const dir = edlBuildDir(lock, target, repoRoot);
  const mf = edlManifestPath(lock, target, repoRoot);
  const hint = `edl build missing or incomplete for ${target}: expected ${path.relative(repoRoot, dir)} and ${path.relative(repoRoot, mf)}; build it with: ${edlBuildHint(target)}`;
  if (!fs.existsSync(dir) || !fs.existsSync(mf)) return { problems: [hint] };
  let m;
  try { m = JSON.parse(await fsp.readFile(mf, "utf8")); } catch (e) { return { problems: [`${path.relative(repoRoot, mf)} is unreadable (${e.message}); rebuild with: ${edlBuildHint(target)}`] }; }
  const problems = [];
  if (m.commit !== lock.edl.commit) problems.push(`edl manifest commit ${m.commit} != pinned ${lock.edl.commit}; rebuild with: ${edlBuildHint(target)}`);
  if (m.target !== target) problems.push(`edl manifest target ${m.target} != ${target}; rebuild with: ${edlBuildHint(target)}`);
  const exe = isWin(target) ? "edl.exe" : "edl";
  if (!(exe in (m.files ?? {}))) problems.push(`edl manifest does not list ${exe}; rebuild with: ${edlBuildHint(target)}`);
  problems.push(...(await verifyEdlTree(dir, m, target)));
  return { problems, manifest: m };
}

/** Copies files and recreates symlinks as symlinks (never dereferences). */
async function copyTree(src, dest, exe) {
  const { files, links } = await scan(src);
  for (const rel of files) {
    const to = path.join(dest, rel);
    await fsp.mkdir(path.dirname(to), { recursive: true });
    await fsp.copyFile(path.join(src, rel), to);
    const srcMode = (await fsp.stat(path.join(src, rel))).mode & 0o777;
    await fsp.chmod(to, rel === exe ? 0o755 : srcMode | 0o644);
  }
  for (const [rel, target] of Object.entries(links)) {
    const to = path.join(dest, rel);
    await fsp.mkdir(path.dirname(to), { recursive: true });
    await fsp.symlink(target, to);
  }
}

async function writeFile(root, dest, data, mode) {
  const to = path.join(root, dest);
  await fsp.mkdir(path.dirname(to), { recursive: true });
  await fsp.writeFile(to, data);
  await fsp.chmod(to, mode);
}

/** Replace outDir's contents with tmp's, preserving README.md. Rolls back on failure; a failed rollback keeps the backup. */
export async function commit(tmp, outDir, rename = fsp.rename) {
  if (!fs.existsSync(outDir)) { await rename(tmp, outDir); return; }
  const backup = `${outDir}.old-${process.pid}`;
  await fsp.mkdir(backup, { recursive: true });
  const moved = [];
  const added = [];
  try {
    for (const e of await fsp.readdir(outDir)) {
      if (e === "README.md") continue;
      await rename(path.join(outDir, e), path.join(backup, e));
      moved.push(e);
    }
    for (const e of await fsp.readdir(tmp)) {
      await rename(path.join(tmp, e), path.join(outDir, e));
      added.push(e);
    }
  } catch (err) {
    try {
      for (const e of added) await fsp.rm(path.join(outDir, e), { recursive: true, force: true });
      for (const e of moved) await rename(path.join(backup, e), path.join(outDir, e));
    } catch (rollbackErr) {
      await fsp.rm(tmp, { recursive: true, force: true });
      throw new Error(`Commit failed (${err.message}) and rollback failed (${rollbackErr.message}). Previous contents are preserved in ${backup}; restore them manually.`);
    }
    await fsp.rm(backup, { recursive: true, force: true });
    throw err;
  }
  await fsp.rm(backup, { recursive: true, force: true });
  await fsp.rm(tmp, { recursive: true, force: true });
}

/**
 * signDir(tempDir, target): optional hook, awaited inside the temp dir before the move
 * (Task 8 wires the macOS signer here).
 */
export async function stage({ lock, target, repoRoot, outDir, fetchImpl = globalThis.fetch, cacheDir, placeholders = false, log = () => {}, signDir }) {
  const items = planStaging(lock, target, repoRoot);
  const tmp = `${outDir}.tmp-${process.pid}`;
  await fsp.mkdir(path.dirname(outDir), { recursive: true });
  await fsp.rm(tmp, { recursive: true, force: true });
  await fsp.mkdir(tmp);
  try {
    const files = [];
    if (placeholders) {
      for (const it of items) {
        if (it.kind === "dir") {
          await writeFile(tmp, `tools/edl/${isWin(target) ? "edl.exe" : "edl"}`, PLACEHOLDER, 0o755);
          files.push(`tools/edl/${isWin(target) ? "edl.exe" : "edl"}`);
        } else {
          await writeFile(tmp, it.dest, PLACEHOLDER, it.mode);
          files.push(it.dest);
        }
      }
    } else {
      const problems = [];
      const zips = new Map();
      const ctx = { fetchImpl, cacheDir, log };
      // 1. downloads and zip members
      for (const it of items.filter((i) => i.kind === "download" || i.kind === "zip-member")) {
        try {
          if (it.kind === "download") {
            await writeFile(tmp, it.dest, await download(it, ctx), it.mode);
            files.push(it.dest);
          } else {
            if (!zips.has(it.url)) zips.set(it.url, download({ ...it, dest: path.posix.basename(it.url) }, ctx).then((b) => b, (e) => e));
            const z = await zips.get(it.url);
            if (z instanceof Error) {
              if (!z.reported) { z.reported = true; problems.push(z.message); }
              continue;
            }
            const wanted = it.member;
            const entries = unzipSync(new Uint8Array(z), { filter: (f) => f.name === wanted });
            if (!entries[wanted]) { problems.push(`${wanted} not found in ${it.url}`); continue; }
            await writeFile(tmp, it.dest, entries[wanted], it.mode);
            files.push(it.dest);
          }
        } catch (e) { problems.push(e.message); }
      }
      // 2. edl build
      let edl;
      try { edl = await verifyEdl(lock, target, repoRoot); } catch (e) { edl = { problems: [`edl verification failed: ${e.message}`] }; }
      const edlProblems = edl.problems;
      problems.push(...edlProblems);
      log("edl verified");
      // 3. repo files (boot-service, legal, wdi)
      for (const it of items.filter((i) => i.kind === "copy")) {
        if (it.dest === "legal/edl-LICENSE.txt" && edlProblems.length) continue;
        if (!fs.existsSync(it.src)) {
          const hint = it.dest.endsWith("inkroot-default-service.sh") ? " (created by the app-core track; it must exist before staging)"
            : it.dest === "tools/wdi-simple.exe" ? " (build it with the Task 7 Windows build)" : "";
          problems.push(`${it.dest}: source file missing at ${path.relative(repoRoot, it.src)}${hint}`);
        } else {
          try {
            await writeFile(tmp, it.dest, await fsp.readFile(it.src), it.mode);
            files.push(it.dest);
          } catch (e) { problems.push(`${it.dest}: could not copy from ${path.relative(repoRoot, it.src)} (${e.message})`); }
        }
      }
      if (!problems.length) {
        const stagedEdl = path.join(tmp, "tools/edl");
        try {
          await copyTree(edlBuildDir(lock, target, repoRoot), stagedEdl, isWin(target) ? "edl.exe" : "edl");
          // re-verify the COPY, closing the gap between checking the source and copying it
          problems.push(...(await verifyEdlTree(stagedEdl, edl.manifest, target, "staged edl")));
          const found = await scan(stagedEdl);
          for (const rel of [...found.files, ...Object.keys(found.links)]) files.push(`tools/edl/${rel}`);
        } catch (e) { problems.push(`edl copy failed: ${e.message}`); }
      }
      if (problems.length) throw new Error(`Staging failed for ${target} with ${problems.length} problem(s):\n- ${problems.join("\n- ")}`);
    }
    if (signDir) await signDir(tmp, target);
    await commit(tmp, outDir);
    return { files: files.sort() };
  } catch (e) {
    await fsp.rm(tmp, { recursive: true, force: true });
    throw e;
  }
}

/** Verifies a staged tree: all contract paths exist, none are placeholders; with lock, pinned hashes match. */
export function checkStaged(outDir, target, lock) {
  const problems = [];
  const exe = isWin(target) ? "edl.exe" : "edl";
  const planLock = lock ?? JSON.parse(fs.readFileSync(new URL("../binaries.lock.json", import.meta.url), "utf8"));
  const expected = planStaging(planLock, target, "/").map((i) => (i.kind === "dir" ? `tools/edl/${exe}` : i.dest));
  for (const rel of expected) {
    const p = path.join(outDir, rel);
    if (!fs.existsSync(p)) { problems.push(`${rel}: missing`); continue; }
    const head = fs.readFileSync(p).subarray(0, PLACEHOLDER.length).toString("latin1");
    if (head === PLACEHOLDER) problems.push(`${rel}: is a placeholder`);
  }
  if (lock) {
    for (const [rel, want] of [[`magisk/${lock.magisk.file}`, lock.magisk.sha256], [`tools/loaders/${lock.loader.path}`, lock.loader.sha256]]) {
      const p = path.join(outDir, rel);
      if (fs.existsSync(p) && sha256Buf(fs.readFileSync(p)) !== want) problems.push(`${rel}: hash mismatch (expected ${want})`);
    }
  }
  return { ok: problems.length === 0, problems };
}
