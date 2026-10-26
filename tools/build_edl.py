#!/usr/bin/env python3
"""Build Inkroot's standalone edl (PyInstaller onedir) for the host's own target.

    python tools/build_edl.py --target <darwin-aarch64|darwin-x86_64|windows-x86_64|linux-x86_64>

Output: tools/.build/<target>/edl/ (edl or edl.exe + _internal/ + LICENSE) and
tools/.build/<target>/edl.manifest.json {commit, target, files:{relpath: sha256}[, links]},
the contract tools/lib/stage.mjs verifies. Stdlib only; needs git and uv on PATH.
"""
import argparse
import hashlib
import json
import os
import platform
import shutil
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
TOOLS = REPO / "tools"
EDL_TOOLS = TOOLS / "edl"
LOCK = TOOLS / "binaries.lock.json"
SRC = TOOLS / ".build" / "src" / "edl"
TARGETS = ("darwin-aarch64", "darwin-x86_64", "windows-x86_64", "linux-x86_64")
HELP_MARKER = "edl.py r <partitionname> <filename>"


class BuildError(Exception):
    pass


def host_target():
    plat = {"Darwin": "darwin", "Windows": "windows", "Linux": "linux"}.get(platform.system())
    arch = {"arm64": "aarch64", "aarch64": "aarch64", "x86_64": "x86_64", "AMD64": "x86_64"}.get(platform.machine())
    return f"{plat}-{arch}" if plat and arch else None


def run(cmd, cwd=None, env=None, capture=False):
    print("+ " + " ".join(str(c) for c in cmd), flush=True)
    r = subprocess.run([str(c) for c in cmd], cwd=cwd, env=env, text=True,
                       stdout=subprocess.PIPE if capture else None,
                       stderr=subprocess.STDOUT if capture else None)
    if capture and r.stdout:
        print(r.stdout, end="" if r.stdout.endswith("\n") else "\n", flush=True)
    if r.returncode != 0:
        raise BuildError(f"command failed ({r.returncode}): {' '.join(str(c) for c in cmd)}")
    return r.stdout or ""


def git_out(*args, cwd):
    return subprocess.run(["git", *args], cwd=cwd, check=True, text=True, capture_output=True).stdout.strip()


def checkout(edl):
    """Clone (or reuse) the pinned edl commit and the pinned Loaders submodule."""
    if not (SRC / ".git").exists():
        SRC.parent.mkdir(parents=True, exist_ok=True)
        run(["git", "clone", edl["repo"], SRC])
    else:
        have = subprocess.run(["git", "cat-file", "-e", f"{edl['commit']}^{{commit}}"], cwd=SRC,
                              capture_output=True).returncode == 0
        if not have:
            run(["git", "fetch", "origin", edl["commit"]], cwd=SRC)
    run(["git", "checkout", "--force", "--detach", edl["commit"]], cwd=SRC)
    head = git_out("rev-parse", "HEAD", cwd=SRC)
    if head != edl["commit"]:
        raise BuildError(f"edl checkout is at {head}, expected {edl['commit']}")
    run(["git", "submodule", "update", "--init", "--depth", "1", "Loaders"], cwd=SRC)
    loaders = git_out("rev-parse", "HEAD", cwd=SRC / "Loaders")
    if loaders != edl["loadersCommit"]:
        raise BuildError(f"Loaders submodule is at {loaders}, expected pinned {edl['loadersCommit']}")


def venv_python():
    return SRC / ".venv" / ("Scripts/python.exe" if os.name == "nt" else "bin/python")


def sha256(p):
    h = hashlib.sha256()
    with open(p, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def build_manifest(onedir, commit, target):
    """Every regular file with its sha256; symlinks recorded (never followed) in `links`."""
    files, links = {}, {}
    root = onedir.resolve()
    for dirpath, dirnames, filenames in os.walk(onedir):
        for name in sorted(dirnames + filenames):
            p = Path(dirpath) / name
            rel = p.relative_to(onedir).as_posix()
            if p.is_symlink():
                dest = os.readlink(p)
                resolved = (p.parent / dest).resolve()
                if os.path.isabs(dest) or root not in resolved.parents:
                    raise BuildError(f"symlink {rel} -> {dest} is absolute or escapes the edl dir")
                links[rel] = dest
            elif p.is_file():
                files[rel] = sha256(p)
            elif not p.is_dir():
                raise BuildError(f"{rel} is neither a file, directory nor symlink")
        # os.walk does not descend into directory symlinks (followlinks=False): they are links above.
    m = {"commit": commit, "target": target, "files": dict(sorted(files.items()))}
    if links:
        m["links"] = dict(sorted(links.items()))
    return m


def clean_env():
    """Environment for running the frozen edl: no Python paths leaking in from the build."""
    return {k: v for k, v in os.environ.items()
            if not k.startswith("PYTHON") and k not in ("VIRTUAL_ENV", "UV_PROJECT_ENVIRONMENT")}


def smoke_test(exe):
    work = exe.parent.parent  # run outside the onedir and the checkout
    out = run([exe, "--inkroot-selftest"], cwd=work, env=clean_env(), capture=True)
    if "libusb OK" not in out:
        raise BuildError("self-test did not report 'libusb OK'")
    r = subprocess.run([str(exe), "--help"], cwd=work, env=clean_env(), text=True, capture_output=True)
    if r.returncode != 0 or HELP_MARKER not in r.stdout + r.stderr:
        raise BuildError(f"`edl --help` failed (exit {r.returncode}) or lacks '{HELP_MARKER}':\n{r.stdout}{r.stderr}")
    print(f"edl --help OK (exit 0, contains '{HELP_MARKER}')")


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--target", required=True, choices=TARGETS)
    args = ap.parse_args(argv)
    host = host_target()
    if args.target != host:
        raise BuildError(f"--target {args.target} does not match this host ({host}); edl must be built natively on each target")

    edl = json.loads(LOCK.read_text())["edl"]
    out_root = TOOLS / ".build" / args.target
    onedir = REPO / edl["buildDir"].replace("{target}", args.target)
    manifest_path = REPO / edl["manifest"].replace("{target}", args.target)
    if onedir.parent != out_root:
        raise BuildError(f"lock edl.buildDir must be tools/.build/{{target}}/edl (PyInstaller --distpath), got {edl['buildDir']}")

    checkout(edl)
    run(["uv", "sync", "--frozen", "--no-dev", "--python", edl["python"]], cwd=SRC)
    py = venv_python()
    run(["uv", "pip", "install", "--python", py, "--require-hashes", "-r", EDL_TOOLS / "requirements-build.txt"])

    manifest_path.unlink(missing_ok=True)
    shutil.rmtree(onedir, ignore_errors=True)
    env = {**os.environ, "INKROOT_EDL_SRC": str(SRC)}
    run([py, "-m", "PyInstaller", EDL_TOOLS / "edl.spec", "--distpath", out_root,
         "--workpath", TOOLS / ".build" / f"work-{args.target}", "--noconfirm"], cwd=SRC, env=env)
    shutil.copyfile(SRC / "LICENSE", onedir / "LICENSE")

    exe = onedir / ("edl.exe" if args.target.startswith("windows") else "edl")
    if not exe.is_file():
        raise BuildError(f"PyInstaller did not produce {exe}")
    smoke_test(exe)

    m = build_manifest(onedir, edl["commit"], args.target)
    manifest_path.write_text(json.dumps(m, indent=2) + "\n")
    extra = f", {len(m['links'])} links" if "links" in m else ""
    print(f"manifest: {manifest_path.relative_to(REPO).as_posix()} ({len(m['files'])} files{extra})")


if __name__ == "__main__":
    try:
        main()
    except BuildError as e:
        print(f"build_edl: {e}", file=sys.stderr)
        sys.exit(1)
