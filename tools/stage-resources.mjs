#!/usr/bin/env node
// Usage: node tools/stage-resources.mjs [--target T] [--placeholders] [--check] [--out DIR]
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { checkStaged, resolveTarget, sha256File, stage } from "./lib/stage.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, "..");
const usage = "usage: node tools/stage-resources.mjs [--target T] [--placeholders] [--check] [--out DIR]";

// macOS signer: Task 8's tools/lib/sign-macos.mjs exports signAll(dir, identity) and verifyAll(dir).
async function loadSigner(target, env) {
  if (!target.startsWith("darwin") || !env.APPLE_SIGNING_IDENTITY) return undefined;
  let mod;
  try { mod = await import("./lib/sign-macos.mjs"); }
  catch (e) { throw new Error(`APPLE_SIGNING_IDENTITY is set but tools/lib/sign-macos.mjs could not be loaded (${e.message}); refusing to stage unsigned binaries`); }
  return async (dir) => { await mod.signAll(dir, env.APPLE_SIGNING_IDENTITY);
    const r = await mod.verifyAll(dir, env.APPLE_TEAM_ID);
    if (!r.ok) throw new Error(`macOS signature verification failed:\n- ${r.problems.join("\n- ")}`);
  };
}

async function main(argv) {
  const opts = { placeholders: false, check: false };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--placeholders") opts.placeholders = true;
    else if (a === "--check") opts.check = true;
    else if (a === "--target" || a === "--out") {
      if (i + 1 >= argv.length) { console.error(`${a} needs a value\n${usage}`); return 2; }
      opts[a.slice(2)] = argv[++i];
    } else { console.error(`unknown argument ${a}\n${usage}`); return 2; }
  }
  let target;
  try { target = resolveTarget(process.env, process.platform, process.arch, opts.target); }
  catch (e) { console.error(`${e.message}\n${usage}`); return 2; }
  const outDir = path.resolve(opts.out ?? path.join(here, "..", "app", "src-tauri", "resources"));
  const lock = JSON.parse(fs.readFileSync(path.join(here, "binaries.lock.json"), "utf8"));

  if (opts.check) {
    const r = checkStaged(outDir, target, lock);
    for (const p of r.problems) console.error(`FAIL ${p}`);
    if (r.ok) console.log(`ok: ${outDir} is complete for ${target}`);
    return r.ok ? 0 : 1;
  }
  try {
    const signDir = await loadSigner(target, process.env);
    const { files } = await stage({ lock, target, repoRoot, outDir, cacheDir: path.join(here, ".cache"),
      placeholders: opts.placeholders, log: (m) => console.error(m), signDir });
    for (const f of files) console.log(`${(await sha256File(path.join(outDir, f))).slice(0, 12)}  ${f}`);
    return 0;
  } catch (e) { console.error(e.message); return 1; }
}
process.exit(await main(process.argv.slice(2)));
