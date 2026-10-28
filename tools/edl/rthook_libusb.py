# PyInstaller runtime hook: make pyusb's libusb-1.0 backend load the libusb bundled from the
# libusb-package wheel, on every OS, whatever library name the caller asks for.
#
# At the pinned commit edl asks for "libusb-1.0.so" on darwin and linux (absent on macOS, and
# on Linux only present with the -dev package), and on Windows passes backend=None so pyusb
# falls back to ctypes.util.find_library + PATH. None of that is reliable inside a frozen
# onedir, so every libusb1 lookup is redirected to the bundled file. (pyinstaller-hooks-contrib's
# pyi_rth_usb replaces libusb1._load_library before this hook runs; it still honours the
# find_library callback, which is what we pass.)
import glob
import os
import sys

_NAMES = {"win32": "libusb-1.0.dll", "darwin": "libusb-1.0.dylib"}


def _inkroot_bundled_libusb():
    """Absolute path of the bundled libusb-1.0, or None if it is missing from the onedir."""
    try:
        import libusb_package

        p = libusb_package.get_library_path()
        if p and os.path.isfile(p):
            return os.path.abspath(str(p))
    except Exception:
        pass
    # get_library_path() only looks in <_internal>/libusb_package; libusb-package's own
    # PyInstaller hook drops the library at the _internal root, so check both.
    name = _NAMES.get(sys.platform, "libusb-1.0.so")
    base = getattr(sys, "_MEIPASS", os.path.dirname(os.path.abspath(sys.executable)))
    for d in (os.path.join(base, "libusb_package"), base):
        for p in [os.path.join(d, name)] + sorted(glob.glob(os.path.join(d, name + ".*"))):
            if os.path.isfile(p):
                return os.path.abspath(p)
    return None


INKROOT_LIBUSB = _inkroot_bundled_libusb()

if INKROOT_LIBUSB:
    if sys.platform == "win32":
        _dir = os.path.dirname(INKROOT_LIBUSB)
        try:
            os.add_dll_directory(_dir)
        except (AttributeError, OSError):
            pass
        os.environ["PATH"] = _dir + os.pathsep + os.environ.get("PATH", "")

    import usb.backend.libusb1

    _inkroot_get_backend = usb.backend.libusb1.get_backend

    def _get_backend(find_library=None):
        # Ignore the caller's find_library (edl's "libusb-1.0.so", pyusb's PATH search).
        return _inkroot_get_backend(find_library=lambda _name: INKROOT_LIBUSB)

    usb.backend.libusb1.get_backend = _get_backend

# Read by edl_entry's self-test to report where libusb was looked for.
sys._inkroot_libusb = INKROOT_LIBUSB
