# -*- mode: python -*-
# PyInstaller onedir spec for Inkroot's bundled edl. Driven by tools/build_edl.py, which
# sets INKROOT_EDL_SRC to the pinned bkerler/edl checkout and runs this from edl's venv.
import os
import sys

from PyInstaller.utils.hooks import collect_data_files, collect_submodules

checkout = os.environ["INKROOT_EDL_SRC"]
here = SPECPATH  # noqa: F821 (provided by PyInstaller)
is_win = sys.platform == "win32"

datas = collect_data_files("libusb_package")
if is_win:
    datas.append((os.path.join(checkout, "edlclient", "Windows"), "edlclient/Windows"))

a = Analysis(  # noqa: F821
    [os.path.join(here, "edl_entry.py")],
    pathex=[checkout],
    hiddenimports=collect_submodules("edlclient"),
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
