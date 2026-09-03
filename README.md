# BOOX Note Max — Root & Recovery (Onyx 4.2 / Snapdragon 855)

Community notes for **rooting** the ONYX BOOX Note Max with Magisk, and for **recovering
(un-bricking)** it over Qualcomm EDL when a boot modification goes wrong. Everything here was
worked out and verified end-to-end on real hardware — including the mistakes, so you don't have
to repeat them.

> [!CAUTION]
> **Your BOOX is bricked, stuck on the ONYX logo, or bootlooping? → [Recovery guide](docs/02-unbrick-edl.md).**
> You can get it back. It needs a Qualcomm "deep-flash" (EDL) cable and a stock `boot` image — details inside.

> [!WARNING]
> Modifying `boot` on a device with a **locked bootloader and enforcing verity** can brick it.
> This worked on the exact firmware below; yours may differ. **Read the whole rooting guide first,
> make a stock backup, and have an EDL recovery cable on hand before you flash anything.** You do
> this at your own risk.

---

## What's here

| Guide | For |
|---|---|
| **[01 · Rooting with Magisk](docs/01-rooting.md)** | Get root on a working device (patch `boot`, flash via EDL). |
| **[02 · Un-brick over EDL](docs/02-unbrick-edl.md)** | Restore a bricked/stuck device to a bootable state. |
| **[03 · Notes & gotchas](docs/03-notes-and-gotchas.md)** | The non-obvious things that cost us days (verity flag, broken Magisk app, `service.d`, SELinux, cable). |
| **[boot-service/](boot-service/)** | Run a script as **root at every boot** on this device — where Magisk's normal `service.d` silently does nothing. |

## Device this was verified on

| | |
|---|---|
| Model | ONYX BOOX **Note Max** (13.3″ e-ink) |
| SoC | Qualcomm **SM8150 / SDM855** (Snapdragon 855), UFS storage |
| OS | Android 13, **Onyx firmware v4.2** (`TKQ1.230721.002`) |
| Boot | A/B slots, **system-as-root, dm-verity/AVB enforcing**, bootloader **locked**, `fastboot` stripped |
| Root | **Magisk 30.x**, patched `boot`, flashed over **Qualcomm EDL (9008)** |

Other Note Max firmware revisions are likely similar but **not identical** — treat versions and
partition layout as things to re-verify on your unit.

## The 60-second version

1. Enable **USB debugging** (Onyx: *Settings → About → tap build repeatedly*, then *Settings →
   System → Developer options → USB debugging*).
2. **Dump your stock `boot`** and keep it somewhere safe — this is your lifeline. *([how](docs/02-unbrick-edl.md#make-a-stock-backup-first))*
3. **Patch it with Magisk** — critically with `KEEPVERITY=true KEEPFORCEENCRYPT=true`. Patching
   with verity **off** bricks this device. *([how](docs/01-rooting.md))*
4. **Flash the patched `boot`** to the active slot over EDL with [bkerler/edl](https://github.com/bkerler/edl).
5. Root works at the **daemon** level even though the **Magisk app can't grant interactive `su`**
   on Onyx 4.2 — see [notes](docs/03-notes-and-gotchas.md) and [boot-service/](boot-service/).

## Credits

- [topjohnwu/Magisk](https://github.com/topjohnwu/Magisk) — the root solution.
- [bkerler/edl](https://github.com/bkerler/edl) — the open-source Qualcomm EDL client used for backup & restore.

Binaries you need (Magisk APK, the `edl` tool, the SoC firehose loader) are **not redistributed
here** — grab them from their official sources as the guides describe. This repo is documentation
and scripts only.

## License

[MIT](LICENSE) for everything in this repository. Not affiliated with ONYX or Qualcomm.
"BOOX" and "ONYX" are trademarks of their respective owners.
