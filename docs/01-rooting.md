# 01 · Rooting the BOOX Note Max with Magisk

> **Easiest: use the [Inkroot app](https://github.com/n0dal/inkroot#download).** It does every step in this guide for you, with checks at each step. This guide is the manual path for tinkerers.

Verified on Onyx firmware **v4.2 (`TKQ1.230721.002`)**, SoC **SM8150/SDM855**, Magisk **30.7**.

> [!IMPORTANT]
> The one thing that will brick you: this device is **system-as-root with dm-verity/AVB
> enforcing**. You **must** patch `boot` with `KEEPVERITY=true KEEPFORCEENCRYPT=true`. Patching
> with verity disabled (Magisk's default when it can't detect your setup) produces a `boot` the
> verified-boot chain rejects → **brick**. We did this once; recovery is [EDL](02-unbrick-edl.md).

## Why not the normal "patch in the Magisk app" flow?

On Onyx 4.2 the **Magisk app itself is broken** (its root service crashes against Onyx's modified
`ActivityManagerService`, so it can never show a superuser prompt — details in
[notes](03-notes-and-gotchas.md)). Root at the **daemon** level still works fine; you just can't
rely on the app to patch or to grant `su` interactively. So we patch `boot` **manually** with
Magisk's own `boot_patch.sh`, which also lets us force the verity flags. It's a few commands.

## What you need

- The Note Max with **USB debugging** on:

  **Turn on USB debugging**

  *Method A — Onyx menu (try this first):*
  1. Open **Settings → Apps**.
  2. Tap the **⋮** menu (top-right) → **USB Debug Mode**, and turn it on.

  *Method B — if you can't find it:*
  1. Install **Activity Launcher** (free and open source) from F-Droid or Google Play. On a BOOX,
     Google Play may first need to be turned on under **Settings → Apps**.
  2. In Activity Launcher, open Android **Settings** (`com.android.settings`).
  3. Tap **About tablet**, then tap **Build number** 7 times.
  4. Go back to **System → Developer options** and turn on **USB debugging**.

  Then plug the tablet into your computer, tap **Allow** on "Allow USB debugging?", and tick
  **Always allow from this computer**.

  > Method A comes from field notes and hasn't been confirmed on firmware 4.2 hardware yet. If it
  > isn't there on your tablet, Method B works on any Android 13 build.

- `adb` on your computer.
- **Magisk 30.7**, from the official release: <https://github.com/topjohnwu/Magisk/releases/tag/v30.7>.
  Download the file named `Magisk-v30.7.apk` (not `app-debug.apk`); its SHA-256 is
  `e0d32d2123532860f97123d927b1bb86c4e08e6fd8a48bfc6b5bee0afae9ebd5`. It's an `.apk` but also a
  zip; we extract binaries from it.
- Your **stock `boot` image** for the active slot. If you don't have it yet, dump it first —
  see [Make a stock backup first](02-unbrick-edl.md#make-a-stock-backup-first). **Keep this file.**
- A **Qualcomm EDL "deep-flash" cable** and [bkerler/edl](https://github.com/bkerler/edl) for
  flashing (and for rescue if anything goes wrong). See the [recovery guide](02-unbrick-edl.md).

## Step 1 — Patch the boot image

`magiskboot` is an Android arm64 binary, so the patching runs **on the device** over adb. The
helper script does the extraction + patch + pull for you:

```bash
./scripts/patch-boot.sh Magisk-v30.7.apk boot_b.stock.img patched-boot.img
```

<details><summary>What that script does (do it by hand if you prefer)</summary>

```bash
# 1. extract Magisk's binaries + scripts from the APK (an APK is a zip)
unzip -o Magisk-v30.7.apk -d magisk_extract >/dev/null

# 2. push them to the device
adb shell 'rm -rf /data/local/tmp/mgk; mkdir -p /data/local/tmp/mgk'
adb push magisk_extract/lib/arm64-v8a/libmagiskboot.so /data/local/tmp/mgk/magiskboot
adb push magisk_extract/lib/arm64-v8a/libmagiskinit.so /data/local/tmp/mgk/magiskinit
adb push magisk_extract/lib/arm64-v8a/libmagisk.so     /data/local/tmp/mgk/magisk
adb push magisk_extract/lib/arm64-v8a/libinit-ld.so    /data/local/tmp/mgk/init-ld
adb push magisk_extract/assets/boot_patch.sh magisk_extract/assets/util_functions.sh \
         magisk_extract/assets/stub.apk /data/local/tmp/mgk/
# Don't name it stock_boot.img: Magisk writes its own copy under that name and would truncate it.
adb push boot_b.stock.img /data/local/tmp/mgk/boot.img

# 3. patch — NOTE the verity flags. This is the part that matters.
adb shell 'cd /data/local/tmp/mgk && chmod 755 boot_patch.sh magiskboot magiskinit magisk init-ld && \
  KEEPVERITY=true KEEPFORCEENCRYPT=true PATCHVBMETAFLAG=false RECOVERYMODE=false \
  sh boot_patch.sh boot.img'

# 4. pull the result
adb pull /data/local/tmp/mgk/new-boot.img patched-boot.img
```
</details>

## Step 2 — Flash the patched boot over EDL

The bootloader is locked and `fastboot` is stripped, so EDL is the only path. **Know which slot is
active** first:

```bash
adb shell getprop ro.boot.slot_suffix     # e.g. "_b"  → flash partition boot_b
```

Then (full command reference in the [recovery guide](02-unbrick-edl.md)):

```bash
adb reboot edl                            # drops into Qualcomm 9008
edl --memory=ufs w boot_b patched-boot.img
edl reset
```

If `adb reboot edl` isn't possible (device won't boot), use the **deep-flash cable** method in the
[recovery guide](02-unbrick-edl.md#how-to-get-into-edl-9008) to reach EDL, then flash the same way.

## Step 3 — Verify

After it boots:

```bash
adb shell '/debug_ramdisk/magisk -c'      # prints the Magisk version → daemon is alive & rooted
```

Root now works for boot-time and daemon-spawned processes. Interactive `su` from an app or adb
shell **won't** prompt (broken app) — to actually *use* root, see **[boot-service/](../boot-service/)**,
which runs a script of yours as root at every boot. That directory also explains why the usual
Magisk `service.d` doesn't work here and what does.

> [!TIP]
> Any third-party app you install (including anything you want run at boot) may get **frozen** by
> Onyx's power manager (`enabled=3`). Turn it off in Onyx *Settings → power/battery → App freeze*
> (a.k.a. "Freeze new apps" off), and allow the app to auto-start. See
> [notes](03-notes-and-gotchas.md#onyx-freezes-your-apps).
