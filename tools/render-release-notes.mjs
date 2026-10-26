#!/usr/bin/env node
// Usage: node tools/render-release-notes.mjs v0.1.0 > notes.md
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { ONE_LINE_DISCLAIMER, RELEASE_ASSETS } from "./lib/constants.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, "..");
const OS_NAMES = { macos: "macOS", windows: "Windows", linux: "Linux" };

export function renderReleaseNotes({ version, verification, lock, template }) {
  template ??= fs.readFileSync(path.join(repoRoot, ".github/release-notes.md.tmpl"), "utf8");
  const status = Object.entries(OS_NAMES).map(([k, name]) => verification[k]
    ? `- ${name}: hardware-verified on ${name} (${verification[k]}, Note Max, firmware TKQ1.230721.002)`
    : `- ${name}: beta — not yet hardware-verified on ${name}`).join("\n");
  const vars = {
    version, disclaimer: ONE_LINE_DISCLAIMER, status, ...RELEASE_ASSETS,
    magiskVersion: lock.magisk.version, edlCommit: lock.edl.commit, edlVersion: lock.edl.version,
    platformTools: lock.platformTools.version,
  };
  const out = template.replace(/\{\{([\w-]+)\}\}/g, (_, k) => {
    if (!(k in vars)) throw new Error(`unknown template variable {{${k}}}`);
    return vars[k];
  });
  return out.endsWith("\n") ? out : out + "\n";
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const version = process.argv[2];
  if (!version) { console.error("usage: node tools/render-release-notes.mjs v0.1.0 > notes.md"); process.exit(2); }
  const lock = JSON.parse(fs.readFileSync(path.join(here, "binaries.lock.json"), "utf8"));
  const verification = JSON.parse(fs.readFileSync(path.join(repoRoot, ".github/hardware-verification.json"), "utf8"));
  process.stdout.write(renderReleaseNotes({ version, verification, lock }));
}
