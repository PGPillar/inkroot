# -*- mode: python -*-
# PyInstaller onedir spec for Inkroot's bundled edl. Driven by tools/build_edl.py, which
# sets INKROOT_EDL_SRC to the pinned bkerler/edl checkout and runs this from edl's venv.
import os
import sys

from PyInstaller.utils.hooks import collect_data_files, collect_dynamic_libs, collect_submodules

checkout = os.environ["INKROOT_EDL_SRC"]
here = SPECPATH  # noqa: F821 (provided by PyInstaller)
is_win = sys.platform == "win32"

# libusb-1.0 goes in as a binary at _internal/libusb_package/, where
# libusb_package.get_library_path() looks (collect_data_files skips *.so on Linux, and the
# package's own hook only drops a copy at the _internal root). rthook_libusb.py points pyusb at it.
_libusb = ["*.dll", "*.dylib", "*.so", "*.so.*"]
binaries = collect_dynamic_libs("libusb_package", destdir="libusb_package", search_patterns=_libusb)
if not binaries:
    raise SystemExit("edl.spec: libusb_package ships no libusb-1.0 library for this platform")
datas = collect_data_files("libusb_package", excludes=_libusb)
if is_win:
    datas.append((os.path.join(checkout, "edlclient", "Windows"), "edlclient/Windows"))

a = Analysis(  # noqa: F821
    [os.path.join(here, "edl_entry.py")],
    pathex=[checkout],
    hiddenimports=collect_submodules("edlclient"),
    binaries=binaries,
    datas=datas,
    runtime_hooks=[os.path.join(here, "rthook_libusb.py")],
)
pyz = PYZ(a.pure)  # noqa: F821
exe = EXE(  # noqa: F821
    pyz,
    a.scripts,
    [],
    exclude_binaries=True,
    name="edl",
    console=True,
    upx=False,
    target_arch=None,
)
coll = COLLECT(exe, a.binaries, a.datas, upx=False, name="edl")  # noqa: F821
