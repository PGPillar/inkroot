# Staged tool resources

Binaries here are staged at build time (not committed; see the root `.gitignore`).
Tauri's `resources/**/*` glob keeps the `resources/` prefix under `resource_dir()`.
`INKROOT_TOOLS_DIR` overrides this root at runtime.

Layout (paths relative to `resources/`; `.exe` on Windows where shown):

| purpose | path |
|---|---|
| adb | `tools/adb[.exe]` |
| edl | `tools/edl/edl[.exe]` |
| EDL loaders | `tools/loaders/<rel>` |
| WinUSB driver installer | `tools/wdi-simple.exe` |
| Magisk APK | `magisk/Magisk-v<version>.apk` |
| boot service | `boot-service/init.bootsvc.rc`, `boot-service/bootsvc.sh`, `boot-service/inkroot-default-service.sh` |

Harmless extras the app ignores: `tools/libwinpthread-1.dll` (Windows) and a `legal/` folder
staged by the release track.
