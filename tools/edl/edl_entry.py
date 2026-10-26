"""Entry point for Inkroot's frozen edl (PyInstaller onedir).

`edl --inkroot-selftest` is a hidden CI check that libusb loads inside the frozen app;
the app never calls it. Everything else is handed to edl unchanged.
"""
import sys


def selftest():
    try:
        import usb.backend.libusb1
        from edlclient.Library.Connection.usblib import usb_class

        # The same DLL-path setup edl does before opening USB (prepends edlclient/Windows
        # on Windows, a no-op elsewhere), so the self-test sees what a real edl run sees.
        usb_class.load_windows_dll(None)
        backend = usb.backend.libusb1.get_backend()
        if backend is None:
            raise RuntimeError("pyusb found no libusb-1.0 backend")
        if sys.platform == "win32":
            print("libusb OK PATH")
        else:
            print(f"libusb OK {backend.lib._name}")
        return 0
    except Exception as e:  # report any failure as a non-zero exit for CI
        print(f"libusb FAILED: {e}", file=sys.stderr)
        return 3


if __name__ == "__main__":
    if sys.argv[1:] == ["--inkroot-selftest"]:
        sys.exit(selftest())
    # Imported only here: edlclient.edl parses sys.argv with docopt at import time.
    from edlclient.edl import run

    run()
