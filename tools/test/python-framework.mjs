// Test helper: lays out a PyInstaller onedir's _internal/ as built by a framework Python (e.g. actions/setup-python
// on macOS), with the standard framework symlinks, some of which point at directories.
import fs from "node:fs";
import path from "node:path";

const INFO_PLIST = `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>Python</string>
<key>CFBundleIdentifier</key><string>org.python.python</string>
<key>CFBundlePackageType</key><string>FMWK</string>
<key>CFBundleShortVersionString</key><string>3.12.0</string>
<key>CFBundleVersion</key><string>3.12.0</string>
</dict></plist>
`;

/** Creates the framework under internalDir; returns {files: {rel: Buffer}, links: {rel: target}} relative to internalDir. */
export function makePythonFramework(internalDir, pythonBytes) {
  const files = {
    "Python.framework/Versions/3.12/Python": Buffer.from(pythonBytes),
    "Python.framework/Versions/3.12/Resources/Info.plist": Buffer.from(INFO_PLIST),
  };
  const links = {
    "Python.framework/Versions/Current": "3.12",
    "Python.framework/Python": "Versions/Current/Python",
    "Python.framework/Resources": "Versions/Current/Resources",
    "Python": "Python.framework/Versions/3.12/Python",
  };
  for (const [rel, b] of Object.entries(files)) {
    const p = path.join(internalDir, rel);
    fs.mkdirSync(path.dirname(p), { recursive: true });
    fs.writeFileSync(p, b);
    fs.chmodSync(p, 0o755);
  }
  for (const [rel, t] of Object.entries(links)) fs.symlinkSync(t, path.join(internalDir, rel));
  return { files, links };
}
