// Single source for strings shared by the README, site and release notes.

export const ONE_LINE_DISCLAIMER =
  "Use at your own risk: modifying your tablet's software can brick it or void its warranty. Inkroot is provided as-is, with no warranty, and is not affiliated with Onyx/BOOX. Read the full [disclaimer](https://github.com/n0dal/inkroot/blob/main/DISCLAIMER.md).";

// Stable release asset names (contract between the README/site and the release workflow).
export const RELEASE_ASSETS = Object.freeze({
  'mac-arm': 'Inkroot-macOS-AppleSilicon.dmg',
  'mac-intel': 'Inkroot-macOS-Intel.dmg',
  windows: 'Inkroot-Windows-x64-setup.exe',
  linux: 'Inkroot-Linux-x86_64.AppImage',
});
