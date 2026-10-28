import { test } from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { zipSync } from "fflate";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { checkStaged, commit, describeStaged, planStaging, resolveTarget, stage } from "../lib/stage.mjs";
import { findMachO, signAll, verifyAll } from "../lib/sign-macos.mjs";
import { makePythonFramework } from "./python-framework.mjs";

const sha = (b) => createHash("sha256").update(b).digest("hex");
const enc = (s) => new TextEncoder().encode(s);
const LOADER = "qualcomm/model_generic/SDM855LA/000a50e100000000_d40eee56f3194665_fhprg.bin";

function walk(d, base = d, out = []) {
  for (const e of fs.readdirSync(d, { withFileTypes: true })) {
    const p = path.join(d, e.name);
    if (e.isDirectory()) walk(p, base, out); else out.push(path.relative(base, p));
  }
  return out.sort();
}
const snapshot = (d) => (fs.existsSync(d) ? walk(d).map((f) => `${f}:${sha(fs.readFileSync(path.join(d, f)))}`) : null);

/** Builds a temp repo with fixtures and a matching lock; returns helpers. */
function fixture({ edlTargets = ["darwin-aarch64", "linux-x86_64", "windows-x86_64"], bootService = true } = {}) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "stage-"));
  const blobs = { magisk: enc("fake apk"), loader: enc("fake loader") };
  const zips = {
    darwin: zipSync({ "platform-tools/adb": enc("adb-mac"), "platform-tools/NOTICE.txt": enc("notice") }),
    linux: zipSync({ "platform-tools/adb": enc("adb-linux"), "platform-tools/NOTICE.txt": enc("notice") }),
    windows: zipSync({ "platform-tools/adb.exe": enc("adb-win"), "platform-tools/AdbWinApi.dll": enc("a"),
      "platform-tools/AdbWinUsbApi.dll": enc("b"), "platform-tools/libwinpthread-1.dll": enc("c"), "platform-tools/NOTICE.txt": enc("notice") }),
  };
  const lock = JSON.parse(fs.readFileSync(new URL("../binaries.lock.json", import.meta.url), "utf8"));
  lock.magisk.sha256 = sha(blobs.magisk);
  lock.loader.sha256 = sha(blobs.loader);
  const bodies = { [lock.magisk.url]: blobs.magisk, [lock.loader.url]: blobs.loader };
  for (const p of ["darwin", "linux", "windows"]) {
    lock.platformTools.archives[p].sha256 = sha(zips[p]);
    bodies[lock.platformTools.archives[p].url] = zips[p];
  }
  const w = (rel, data) => { const p = path.join(root, rel); fs.mkdirSync(path.dirname(p), { recursive: true }); fs.writeFileSync(p, data); };
  for (const f of lock.bootService.files) if (bootService || !f.includes("inkroot-default")) w(f, `#!/bin/sh\n# ${f}\n`);
  w("DISCLAIMER.md", "d"); w("THIRD_PARTY_NOTICES.md", "t");
  w("tools/node_modules/@ibm/plex-sans/LICENSE.txt", "ofl");
  w("tools/.build/windows-x86_64/wdi-simple.exe", "wdi");
  for (const t of edlTargets) {
    const dir = `tools/.build/${t}/edl`;
    const exe = t.startsWith("windows") ? "edl.exe" : "edl";
    const files = { [exe]: "edl-bin", "_internal/lib.so": "lib", LICENSE: "gpl" };
    const manifest = { commit: lock.edl.commit, target: t, files: {} };
    for (const [rel, c] of Object.entries(files)) { w(`${dir}/${rel}`, c); manifest.files[rel] = sha(c); }
    w(`tools/.build/${t}/edl.manifest.json`, JSON.stringify(manifest));
  }
  const calls = [];
  const fetchImpl = async (url) => {
    calls.push(url);
    const body = bodies[url];
    return body ? new Response(body, { status: 200 }) : new Response("nope", { status: 404 });
  };
  return { root, lock, bodies, fetchImpl, calls, outDir: path.join(root, "app/src-tauri/resources"), cacheDir: path.join(root, "cache") };
}
const run = (fx, target, extra = {}) => stage({ lock: fx.lock, target, repoRoot: fx.root, outDir: fx.outDir, fetchImpl: fx.fetchImpl, cacheDir: fx.cacheDir, ...extra });

const COMMON = ["boot-service/bootsvc.sh", "boot-service/init.bootsvc.rc", "boot-service/inkroot-default-service.sh",
  "legal/DISCLAIMER.md", "legal/OFL.txt", "legal/THIRD_PARTY_NOTICES.md", "legal/edl-LICENSE.txt", "legal/platform-tools-NOTICE.txt",
  "magisk/Magisk-v30.7.apk", "tools/edl/", `tools/loaders/${LOADER}`];

test("planStaging produces exactly the contract layout for each target", () => {
  const lock = JSON.parse(fs.readFileSync(new URL("../binaries.lock.json", import.meta.url), "utf8"));
  const dests = (t) => planStaging(lock, t, "/r").map((i) => i.dest).sort();
  for (const t of ["darwin-aarch64", "darwin-x86_64", "linux-x86_64"]) assert.deepEqual(dests(t), [...COMMON, "tools/adb"].sort(), t);
  assert.deepEqual(dests("windows-x86_64"), [...COMMON, "tools/adb.exe", "tools/AdbWinApi.dll", "tools/AdbWinUsbApi.dll",
    "tools/libwinpthread-1.dll", "tools/wdi-simple.exe"].sort());
});

test("stage aborts on hash mismatch and leaves resources untouched", async () => {
  const fx = fixture();
  fs.mkdirSync(fx.outDir, { recursive: true });
  fs.writeFileSync(path.join(fx.outDir, "keep.txt"), "old");
  const before = snapshot(fx.outDir);
  const good = fx.fetchImpl;
  fx.fetchImpl = async (u) => (u === fx.lock.magisk.url ? new Response("evil") : good(u));
  await assert.rejects(run(fx, "linux-x86_64"), (e) => {
    assert.match(e.message, /Magisk-v30\.7\.apk/);
    assert.ok(e.message.includes(fx.lock.magisk.sha256));
    assert.ok(e.message.includes(sha(enc("evil"))));
    return true;
  });
  assert.deepEqual(snapshot(fx.outDir), before);
  assert.deepEqual(fs.readdirSync(path.dirname(fx.outDir)).filter((n) => n.includes(".tmp-")), []);
  fs.rmSync(fx.outDir, { recursive: true });
  await assert.rejects(run(fx, "linux-x86_64"));
  assert.equal(fs.existsSync(fx.outDir), false);
});

test("corrupt cache entry is re-downloaded", async () => {
  const fx = fixture();
  fs.mkdirSync(fx.cacheDir, { recursive: true });
  fs.writeFileSync(path.join(fx.cacheDir, fx.lock.magisk.sha256), "garbage");
  await run(fx, "linux-x86_64");
  assert.equal(fx.calls.filter((u) => u === fx.lock.magisk.url).length, 1);
  assert.equal(sha(fs.readFileSync(path.join(fx.cacheDir, fx.lock.magisk.sha256))), fx.lock.magisk.sha256);
  fx.calls.length = 0;
  await run(fx, "linux-x86_64");
  assert.deepEqual(fx.calls, []);
});

test("missing edl build for target fails with build hint", async () => {
  const fx = fixture({ edlTargets: ["darwin-aarch64"] });
  await assert.rejects(run(fx, "linux-x86_64"), /tools[\\/]\.build[\\/]linux-x86_64[\\/]edl.*python tools\/build_edl\.py --target linux-x86_64/);
});

test("edl manifest mismatch fails naming the file", async () => {
  const fx = fixture();
  fs.writeFileSync(path.join(fx.root, "tools/.build/linux-x86_64/edl/_internal/lib.so"), "tampered");
  await assert.rejects(run(fx, "linux-x86_64"), /_internal\/lib\.so.*hash mismatch/);
});

test("env target overrides host; flag beats env", () => {
  assert.equal(resolveTarget({ TAURI_ENV_PLATFORM: "darwin", TAURI_ENV_ARCH: "x86_64" }, "darwin", "arm64"), "darwin-x86_64");
  assert.equal(resolveTarget({}, "win32", "x64"), "windows-x86_64");
  assert.equal(resolveTarget({}, "darwin", "arm64"), "darwin-aarch64");
  assert.equal(resolveTarget({ TAURI_ENV_PLATFORM: "darwin", TAURI_ENV_ARCH: "x86_64" }, "linux", "x64", "windows-x86_64"), "windows-x86_64");
  assert.throws(() => resolveTarget({}, "linux", "arm64"), /Unsupported target/);
  assert.throws(() => resolveTarget({ TAURI_ENV_PLATFORM: "freebsd", TAURI_ENV_ARCH: "x86_64" }, "linux", "x64"), /Unsupported target/);
});

test("staged executables are 0755 on POSIX", { skip: process.platform === "win32" }, async () => {
  const fx = fixture();
  await run(fx, "linux-x86_64");
  const mode = (r) => fs.statSync(path.join(fx.outDir, r)).mode & 0o777;
  assert.equal(mode("tools/adb"), 0o755);
  assert.equal(mode("tools/edl/edl"), 0o755);
  assert.equal(mode("magisk/Magisk-v30.7.apk"), 0o644);
  assert.equal(mode(`tools/loaders/${LOADER}`), 0o644);
  assert.equal(fs.readFileSync(path.join(fx.outDir, "legal/platform-tools-NOTICE.txt"), "utf8"), "notice");
});

test("missing inkroot-default-service.sh fails with app-core hint, collected with other problems", async () => {
  const fx = fixture({ bootService: false, edlTargets: [] });
  const good = fx.fetchImpl;
  fx.fetchImpl = async (u) => (u === fx.lock.magisk.url ? new Response("evil") : good(u));
  await assert.rejects(run(fx, "linux-x86_64"), (e) => {
    assert.match(e.message, /inkroot-default-service\.sh.*app-core/);
    assert.match(e.message, /Magisk-v30\.7\.apk/);
    assert.match(e.message, /build_edl\.py --target linux-x86_64/);
    return true;
  });
});

test("check rejects placeholders; a real stage passes", async () => {
  const fx = fixture();
  await run(fx, "windows-x86_64", { placeholders: true });
  assert.equal(fs.readFileSync(path.join(fx.outDir, "tools/edl/edl.exe"), "utf8"), "INKROOT-PLACEHOLDER\n");
  assert.equal(fx.calls.length, 0);
  const r = checkStaged(fx.outDir, "windows-x86_64");
  assert.equal(r.ok, false);
  assert.ok(r.problems.some((p) => p.includes("tools/adb.exe")));
  assert.equal(r.problems.length, planStaging(fx.lock, "windows-x86_64", "/").length);
  await run(fx, "windows-x86_64");
  assert.deepEqual(checkStaged(fx.outDir, "windows-x86_64", fx.lock), { ok: true, problems: [] });
});

test("README.md is preserved, old entries replaced, signDir hook runs in temp dir", async () => {
  const fx = fixture();
  fs.mkdirSync(path.join(fx.outDir, "stale"), { recursive: true });
  fs.writeFileSync(path.join(fx.outDir, "README.md"), "stub");
  fs.writeFileSync(path.join(fx.outDir, "stale/x"), "x");
  let seen;
  await run(fx, "linux-x86_64", { signDir: async (d, t) => { seen = { d, t, adb: fs.existsSync(path.join(d, "tools/adb")) }; } });
  assert.equal(seen.t, "linux-x86_64");
  assert.equal(seen.adb, true);
  assert.notEqual(seen.d, fx.outDir);
  assert.equal(fs.readFileSync(path.join(fx.outDir, "README.md"), "utf8"), "stub");
  assert.equal(fs.existsSync(path.join(fx.outDir, "stale")), false);
  assert.equal(fs.existsSync(path.join(fx.outDir, "tools/adb")), true);
  assert.deepEqual(fs.readdirSync(path.dirname(fx.outDir)).filter((n) => /\.(tmp|old)-/.test(n)), []);
});

function addLink(fx, target, rel, linkTarget, { record = true } = {}) {
  const dir = path.join(fx.root, `tools/.build/${target}/edl`);
  fs.mkdirSync(path.dirname(path.join(dir, rel)), { recursive: true });
  fs.symlinkSync(linkTarget, path.join(dir, rel));
  if (record) {
    const mp = path.join(fx.root, `tools/.build/${target}/edl.manifest.json`);
    const m = JSON.parse(fs.readFileSync(mp, "utf8"));
    m.links = { ...m.links, [rel]: linkTarget };
    fs.writeFileSync(mp, JSON.stringify(m));
  }
}

test("source tampered after verification is caught by the post-copy check", async () => {
  const fx = fixture();
  const lib = path.join(fx.root, "tools/.build/linux-x86_64/edl/_internal/lib.so");
  await assert.rejects(run(fx, "linux-x86_64", { log: (m) => { if (m === "edl verified") fs.writeFileSync(lib, "tampered"); } }),
    /staged edl file _internal\/lib\.so.*hash mismatch/);
  assert.equal(fs.existsSync(fx.outDir), false);
});

test("symlinks in the onedir are recorded, verified and recreated as symlinks", async () => {
  const fx = fixture();
  addLink(fx, "linux-x86_64", "_internal/lib.so.1", "lib.so");
  addLink(fx, "linux-x86_64", "_internal/libdir", ".");
  await run(fx, "linux-x86_64");
  const l = path.join(fx.outDir, "tools/edl/_internal/lib.so.1");
  assert.equal(fs.lstatSync(l).isSymbolicLink(), true);
  assert.equal(fs.readlinkSync(l), "lib.so");
  assert.equal(fs.lstatSync(path.join(fx.outDir, "tools/edl/_internal/libdir")).isSymbolicLink(), true);
});

/** Adds a framework-build Python.framework (with directory symlinks) to the fixture's edl onedir and manifest. */
function addPythonFramework(fx, target, pythonBytes) {
  const dir = path.join(fx.root, `tools/.build/${target}/edl`);
  const { files, links } = makePythonFramework(path.join(dir, "_internal"), pythonBytes);
  const mp = path.join(fx.root, `tools/.build/${target}/edl.manifest.json`);
  const m = JSON.parse(fs.readFileSync(mp, "utf8"));
  for (const [rel, b] of Object.entries(files)) m.files[`_internal/${rel}`] = sha(b);
  for (const [rel, t] of Object.entries(links)) m.links = { ...m.links, [`_internal/${rel}`]: t };
  fs.writeFileSync(mp, JSON.stringify(m));
}

test("framework-build Python.framework links (some to directories) are staged as links and listed without reading them", { skip: process.platform === "win32" && "symlinks" }, async () => {
  const fx = fixture();
  addPythonFramework(fx, "darwin-aarch64", "fake python");
  const { files } = await run(fx, "darwin-aarch64");
  const fw = path.join(fx.outDir, "tools/edl/_internal/Python.framework");
  for (const [rel, t] of [["Versions/Current", "3.12"], ["Python", "Versions/Current/Python"], ["Resources", "Versions/Current/Resources"], ["../Python", "Python.framework/Versions/3.12/Python"]]) {
    assert.equal(fs.lstatSync(path.join(fw, rel)).isSymbolicLink(), true, rel);
    assert.equal(fs.readlinkSync(path.join(fw, rel)), t);
  }
  assert.equal(fs.readFileSync(path.join(fw, "Resources/Info.plist"), "utf8").includes("CFBundleExecutable"), true);
  const lines = await describeStaged(fx.outDir, files); // used to hit EISDIR hashing Resources -> dir
  assert.ok(lines.includes("link          tools/edl/_internal/Python.framework/Resources -> Versions/Current/Resources"));
  assert.ok(lines.some((l) => l === `${sha("fake python").slice(0, 12)}  tools/edl/_internal/Python.framework/Versions/3.12/Python`));
});

test("framework-build Python.framework retargeted Current link is rejected", { skip: process.platform === "win32" && "symlinks" }, async () => {
  const fx = fixture();
  addPythonFramework(fx, "darwin-aarch64", "fake python");
  const cur = path.join(fx.root, "tools/.build/darwin-aarch64/edl/_internal/Python.framework/Versions/Current");
  fs.rmSync(cur); fs.symlinkSync("../..", cur);
  await assert.rejects(run(fx, "darwin-aarch64"), /link _internal\/Python\.framework\/Versions\/Current: expected -> 3\.12/);
});

test("staging + ad-hoc signing a framework-build onedir signs the framework bundle and passes strict verification", { skip: process.platform !== "darwin" && "codesign needs macOS" }, async () => {
  const fx = fixture();
  const edlBin = path.join(fx.root, "tools/.build/darwin-aarch64/edl/edl");
  fs.copyFileSync("/usr/bin/true", edlBin);
  const mp = path.join(fx.root, "tools/.build/darwin-aarch64/edl.manifest.json");
  const m = JSON.parse(fs.readFileSync(mp, "utf8"));
  m.files.edl = sha(fs.readFileSync(edlBin)); fs.writeFileSync(mp, JSON.stringify(m));
  addPythonFramework(fx, "darwin-aarch64", fs.readFileSync("/usr/bin/true"));
  const logs = [];
  let verify;
  await run(fx, "darwin-aarch64", { signDir: async (d) => { await signAll(d, "-", { log: (l) => logs.push(l) }); verify = await verifyAll(d, "ABCDE12345"); } });
  assert.match(logs.join("\n"), /1 framework bundle/);
  assert.ok(!verify.problems.some((p) => /not signed|--verify failed|runtime|EISDIR/.test(p)), verify.problems.join("\n"));
  assert.ok(verify.problems.some((p) => /_internal\/Python\.framework\/Versions\/3\.12: TeamIdentifier/.test(p)), "framework bundle is checked");
  const fw = path.join(fx.outDir, "tools/edl/_internal/Python.framework");
  assert.equal(fs.existsSync(path.join(fw, "Versions/3.12/_CodeSignature/CodeResources")), true);
  assert.equal(fs.lstatSync(path.join(fw, "Versions/Current")).isSymbolicLink(), true);
  const cs = (...a) => spawnSync("codesign", a, { encoding: "utf8" });
  for (const p of [fw, path.join(fw, "Versions/3.12"), path.join(fx.outDir, "tools/edl/edl")]) {
    const r = cs("--verify", "--strict", p);
    assert.equal(r.status, 0, `${p}: ${r.stderr}`);
  }
  assert.equal(cs("--verify", "--deep", "--strict", path.join(fx.outDir, "tools/edl/edl")).status, 0);
  assert.ok((await findMachO(fx.outDir)).every((f) => fs.lstatSync(f).isFile()));
});

test("symlink problems are collected, not thrown: unlisted, escaping, retargeted", async () => {
  let fx = fixture();
  addLink(fx, "linux-x86_64", "_internal/dirlink", ".", { record: false });
  await assert.rejects(run(fx, "linux-x86_64"), /link _internal\/dirlink.*not in manifest/);
  fx = fixture();
  addLink(fx, "linux-x86_64", "_internal/evil", "../../../../../etc");
  await assert.rejects(run(fx, "linux-x86_64"), /link _internal\/evil.*escapes/);
  fx = fixture();
  addLink(fx, "linux-x86_64", "_internal/l", "lib.so");
  const p = path.join(fx.root, "tools/.build/linux-x86_64/edl/_internal/l");
  fs.rmSync(p); fs.symlinkSync("elsewhere", p);
  await assert.rejects(run(fx, "linux-x86_64"), /link _internal\/l: expected -> lib\.so/);
});

test("copy-phase errors and edl verification exceptions are collected with the other problems", async () => {
  const fx = fixture({ edlTargets: [] });
  fs.mkdirSync(path.join(fx.root, "tools/.build/linux-x86_64/edl"), { recursive: true });
  fs.writeFileSync(path.join(fx.root, "tools/.build/linux-x86_64/edl.manifest.json"), JSON.stringify({ commit: fx.lock.edl.commit, target: "linux-x86_64", files: { edl: "x" }, links: { l: 5 } }));
  fs.rmSync(path.join(fx.root, "DISCLAIMER.md"));
  fs.mkdirSync(path.join(fx.root, "DISCLAIMER.md")); // reading a directory throws EISDIR
  await assert.rejects(run(fx, "linux-x86_64"), (e) => {
    assert.match(e.message, /legal\/DISCLAIMER\.md: could not copy/);
    assert.match(e.message, /unsafe link l/);
    return true;
  });
});

test("failed rollback keeps the backup and reports its path", async () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "commit-"));
  const out = path.join(root, "res"), tmp = path.join(root, "tmp");
  fs.mkdirSync(path.join(out, "old"), { recursive: true });
  fs.writeFileSync(path.join(out, "old/f"), "precious");
  fs.mkdirSync(tmp); fs.writeFileSync(path.join(tmp, "new"), "n");
  let n = 0;
  const flaky = async (a, b) => { n++; if (n >= 2) throw new Error("boom"); return fs.promises.rename(a, b); };
  // call 1 moves old -> backup; call 2 (tmp -> out) fails; rollback call 3 fails too
  await assert.rejects(commit(tmp, out, flaky), (e) => {
    assert.match(e.message, /rollback failed/);
    assert.ok(e.message.includes(`${out}.old-${process.pid}`));
    return true;
  });
  assert.equal(fs.readFileSync(path.join(`${out}.old-${process.pid}`, "old/f"), "utf8"), "precious");
});

test("platform-tools zip hash mismatch and missing member are reported", async () => {
  const fx = fixture();
  const good = fx.fetchImpl;
  const url = fx.lock.platformTools.archives.linux.url;
  fx.fetchImpl = async (u) => (u === url ? new Response("bad zip") : good(u));
  await assert.rejects(run(fx, "linux-x86_64"), (e) => {
    assert.ok(e.message.includes(fx.lock.platformTools.archives.linux.sha256));
    assert.ok(e.message.includes(sha(enc("bad zip"))));
    return true;
  });
  const fx2 = fixture();
  const z = zipSync({ "platform-tools/other": enc("x") });
  fx2.lock.platformTools.archives.linux.sha256 = sha(z);
  fx2.fetchImpl = async (u) => (u === url ? new Response(z) : good(u));
  await assert.rejects(run(fx2, "linux-x86_64"), /platform-tools\/adb not found/);
});

test("checkStaged with lock detects hash mismatch", async () => {
  const fx = fixture();
  await run(fx, "linux-x86_64");
  fs.writeFileSync(path.join(fx.outDir, "magisk/Magisk-v30.7.apk"), "swapped");
  const r = checkStaged(fx.outDir, "linux-x86_64", fx.lock);
  assert.equal(r.ok, false);
  assert.match(r.problems.join("\n"), /Magisk-v30\.7\.apk: hash mismatch/);
});

test("CLI exit codes: 2 for usage, 1 for failed check", () => {
  // fileURLToPath, not URL#pathname: on Windows the latter is "/D:/..." and
  // node fails with "Cannot find module" (exit 1) before the CLI ever runs.
  const cli = fileURLToPath(new URL("../stage-resources.mjs", import.meta.url));
  const runCli = (...args) => spawnSync(process.execPath, [cli, ...args], { encoding: "utf8" });
  for (const args of [["--bogus"], ["--target", "freebsd-x86_64"]]) {
    const r = runCli(...args);
    assert.equal(r.status, 2, r.stderr);
    assert.match(r.stderr, /usage: node tools\/stage-resources\.mjs/);
  }
  const empty = fs.mkdtempSync(path.join(os.tmpdir(), "cli-"));
  const r = runCli("--check", "--target", "linux-x86_64", "--out", empty);
  assert.equal(r.status, 1, r.stderr);
  assert.doesNotMatch(r.stderr, /usage:|Cannot find module/);
});
