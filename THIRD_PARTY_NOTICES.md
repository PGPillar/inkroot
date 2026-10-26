# Third-party notices

Inkroot itself is MIT-licensed (see [LICENSE](LICENSE)). It bundles or builds on the components
below, each under its own licence. Device firmware is never bundled.

## Magisk 30.7

- Licence: GPL-3.0
- Source: https://github.com/topjohnwu/Magisk/tree/v30.7
- Shipped unmodified as `magisk/Magisk-v30.7.apk`. Binaries from it run only on the tablet.

## edl (bkerler/edl) 3.62

- Licence: GPL-3.0
- Source: https://github.com/bkerler/edl/tree/2f8e89a848afaaef68997fcbcb5b178d958d497b
- Shipped as a separate standalone executable built with PyInstaller by `tools/build_edl.py` from
  that exact commit.
- Bundled runtime and libraries, confirmed from the edl build environment (edl's own `uv.lock`
  plus `tools/edl/requirements-build.txt`, darwin-aarch64 build of 2026-10-26). Only packages the
  frozen edl actually imports are bundled:
  - CPython 3.12 (PSF-2.0), including its OpenSSL (Apache-2.0)
  - PyInstaller 6.16.0 bootloader (GPL-2.0-or-later with the PyInstaller bootloader exception)
  - libusb-1.0 (LGPL-2.1-or-later), via libusb-package 1.0.30.0 (Apache-2.0)
  - pyusb 1.3.1 (BSD-3-Clause)
  - pyserial 3.5 (BSD-3-Clause)
  - pycryptodome 3.23.0 and pycryptodomex 3.23.0 (BSD-2-Clause / Public Domain)
  - colorama 0.4.6 (BSD-3-Clause)
  - docopt 0.6.2 (MIT)
  - Exscript 2.6.32 (MIT), with configparser 7.2.0 (MIT) and future 1.0.0 (MIT)
  - paramiko 4.0.0 (LGPL-2.1), with bcrypt 5.0.0 (Apache-2.0), cryptography 50.0.0
    (Apache-2.0 OR BSD-3-Clause, with statically linked OpenSSL, Apache-2.0), PyNaCl 1.6.2
    (Apache-2.0, with libsodium, ISC), invoke 3.0.3 (BSD-2-Clause), cffi 2.1.1 (MIT-0) and
    pycparser 3.0 (BSD-3-Clause)
  - requests 2.34.2 (Apache-2.0), with urllib3 2.7.0 (MIT), idna 3.18 (BSD-3-Clause),
    charset-normalizer 3.4.9 (MIT) and certifi 2026.7.22 (MPL-2.0)
  - passlib 1.7.4 (BSD-3-Clause)
  - importlib-resources 7.1.0 (Apache-2.0), packaging 26.3 (Apache-2.0 OR BSD-2-Clause) and
    setuptools 84.2.1 (MIT), pulled in by libusb-package and PyInstaller's runtime support
- edl declares lxml 6.1.1 (BSD-3-Clause) and pylzma 0.6.1 (LGPL-2.1-or-later) as dependencies, but
  the edl client never imports them, so they are installed at build time and not bundled.

## Qualcomm firehose loader

- File: `000a50e100000000_d40eee56f3194665_fhprg.bin`
- Licence: proprietary Qualcomm/OEM code, redistributed as the edl project distributes it. Inkroot
  grants no licence to it.
- Source: https://github.com/bkerler/Loaders/tree/bf0d8017eb97464530114a348c5157a6ea6a3372

## Android SDK Platform-Tools r37.0.1 (adb)

- Licence: Google, Android Software Development Kit License / Apache-2.0 components (see
  `NOTICE.txt` in the platform-tools zip, staged as `legal/platform-tools-NOTICE.txt`).
- Source: https://android.googlesource.com/platform/packages/modules/adb/
- Shipped unmodified from Google's release. The Windows build also includes `libwinpthread-1.dll`
  (MinGW-w64 runtime; the platform-tools `NOTICE.txt` carries the mingw-w64 project's
  permissive MIT-style licence text, though it does not name the DLL itself).

## libwdi / wdi-simple v1.5.1 (Windows only)

- Licence: LGPL-3.0
- Source: https://github.com/pbatard/libwdi/tree/v1.5.1
- Built unmodified by `tools/build-wdi.sh`.

## IBM Plex Sans and IBM Plex Mono

- Licence: SIL Open Font License 1.1
- Source: https://github.com/IBM/plex

## Written offer / source

For each GPL/LGPL component above, the complete corresponding source is available at the pinned
link, and the build scripts in this repository (`tools/`) reproduce the binaries we ship. Device
firmware is never bundled.
