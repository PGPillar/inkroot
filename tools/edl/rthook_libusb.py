# PyInstaller runtime hook: point pyusb at the libusb bundled by the libusb-package wheel.
# At the pinned commit edl asks for "libusb-1.0.so" on darwin (which never exists there)
# and on linux (which needs the -dev package), so every lookup is redirected on those
# platforms. Windows is untouched: edl prepends its own edlclient/Windows DLL dir to PATH.
import sys

if sys.platform.startswith(("darwin", "linux")):
    import libusb_package
    import usb.backend.libusb1

    _bundled = str(libusb_package.get_library_path())
    _get_backend = usb.backend.libusb1.get_backend

    def _inkroot_get_backend(find_library=None):
        return _get_backend(find_library=lambda _name: _bundled)

    usb.backend.libusb1.get_backend = _inkroot_get_backend
