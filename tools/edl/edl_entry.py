"""Entry point for Inkroot's frozen edl (PyInstaller onedir).

`edl --inkroot-selftest` is a hidden CI check that libusb loads inside the frozen app;
the app never calls it. Everything else is handed to edl unchanged.
"""
import sys

# edl's progress bar prints block characters (U+2588). On Windows a redirected stdout uses the
# locale code page (cp1252), which can't encode them, so edl crashed mid-read. PYTHONUTF8 /
# PYTHONIOENCODING are ignored by the frozen app, so force UTF-8 here, before edl prints anything.
for _stream in (sys.stdout, sys.stderr):
    if _stream is not None and hasattr(_stream, "reconfigure"):
        _stream.reconfigure(encoding="utf-8", errors="replace")


def selftest():
    import io
    import logging

    # pyusb reports why a backend failed only through logging; keep it for the failure message.
    log = io.StringIO()
    handler = logging.StreamHandler(log)
    usb_log = logging.getLogger("usb")
    usb_log.addHandler(handler)
    usb_log.setLevel(logging.DEBUG)
    usb_log.propagate = False
    try:
        import usb.backend.libusb1
        import usb.core
        from edlclient.Library.Connection.usblib import usb_class

        # Same libusb setup as edl's usb_class.__init__: the DLL-path tweak (Windows only), then
        # libusb1.get_backend(find_library=<"libusb-1.0.so">) on darwin/linux, or backend=None on
        # Windows so pyusb's default lookup (libusb1.get_backend()) picks the backend.
        usb_class.load_windows_dll(None)
        if sys.platform == "win32":
            backend = usb.backend.libusb1.get_backend()
            find_backend = None
        else:
            backend = usb.backend.libusb1.get_backend(find_library=lambda x: "libusb-1.0.so")
            find_backend = backend
        if backend is None:
            raise RuntimeError("pyusb found no libusb-1.0 backend")
        # Enumerate the bus exactly as edl's usb.core.find does (no device needed).
        list(usb.core.find(find_all=True, backend=find_backend))
        print(f"libusb OK {backend.lib._name}")
        return 0
    except Exception as e:  # report any failure as a non-zero exit for CI
        print(f"libusb FAILED: {e}", file=sys.stderr)
        print(f"  bundled libusb: {getattr(sys, '_inkroot_libusb', None)}", file=sys.stderr)
        for line in log.getvalue().splitlines():
            print(f"  pyusb: {line}", file=sys.stderr)
        return 3
    finally:
        usb_log.removeHandler(handler)


if __name__ == "__main__":
    if sys.argv[1:] == ["--inkroot-selftest"]:
        sys.exit(selftest())
    # Imported only here: edlclient.edl parses sys.argv with docopt at import time.
    from edlclient.edl import run

    run()
