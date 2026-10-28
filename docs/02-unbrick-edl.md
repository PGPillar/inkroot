# 02 · Un-brick / recover over Qualcomm EDL

> **Easiest: use the [Inkroot app](https://github.com/n0dal/inkroot#download).** It does every step in this guide for you, with checks at each step. This guide is the manual path for tinkerers.

**Your BOOX is stuck on the ONYX logo, bootlooping, or dead after a `boot` flash? It's almost
certainly recoverable.** The SoC drops into Qualcomm's **Emergency Download mode (EDL / 9008)**,
and from there you write a good `boot` image back. Verified end-to-end on the Note Max (SDM855).

You need two things:
1. A **Qualcomm EDL "deep-flash" cable** (see below), and
2. A **good `boot` image** to write back (your stock backup — or a known-good patched one).

---

## What cable, exactly

A **Qualcomm "deep-flash" / EDL cable**: a USB cable (USB-A on the host end) with a small **button
or built-in switch that momentarily shorts USB `D+` to ground**, which forces the SoC into 9008
mode. They're sold as "Qualcomm deep flash cable" / "EDL cable" for a few dollars. A normal USB
cable will **not** force EDL.

> [!IMPORTANT]
> **Plug the cable's USB-A end into a _native_ USB-A port on a real machine.** A USB-C laptop with
> a USB-C→USB-A **adapter/dongle does not work** — the adapter drops the data lines and the host
> never sees the `9008` device. On a MacBook with no USB-A port, use a desktop/mini-PC/NUC/server
> with real USB-A ports. (We recovered ours from a Linux host's onboard USB-A.)

> [!NOTE]
> **Backing up or flashing a tablet that still boots doesn't need the deep-flash cable or USB-A.** Enter EDL with `adb reboot edl` and a normal USB-C cable works, including straight into a Mac (verified: our stock backup was read this way from macOS). The native-USB-A rule above is for the **deep-flash cable** method.

## Install the EDL tool

Open-source client: **[bkerler/edl](https://github.com/bkerler/edl)**.

**Tested with:** `bkerler/edl` commit `2f8e89a848afaaef68997fcbcb5b178d958d497b` (client 3.62), Loaders submodule `bf0d8017eb97464530114a348c5157a6ea6a3372`. With no loader given it detected `SM8150`, HWID `0x000a50e100000000`, and picked `Loaders/qualcomm/model_generic/SDM855LA/000a50e100000000_d40eee56f3194665_fhprg.bin`. To use exactly that: `git checkout 2f8e89a8 && git submodule update`, and pass `--loader=Loaders/qualcomm/model_generic/SDM855LA/000a50e100000000_d40eee56f3194665_fhprg.bin`.

```bash
git clone --recursive https://github.com/bkerler/edl
cd edl
python3 -m venv .venv && . .venv/bin/activate
pip install .
```

On Linux you'll also want the udev rules bundled in that repo (`cp edl.rules /etc/udev/rules.d/`
then re-plug) so the tool can claim the `05c6:9008` device without root fights. The `edl` tool
**bundles the common firehose loaders** and auto-selected the SDM855 loader for us — you usually
don't have to supply one.

## How to get into EDL (9008)

- **From a device that still boots / has adb:**
  ```bash
  adb reboot edl
  ```
- **From a brick (won't boot, keys do nothing):** use the **deep-flash cable**:
  1. Plug the cable's **USB-A end into the host's native USB-A port**.
  2. Hold the cable's button (the D+/GND short).
  3. Plug the USB-C end into the tablet.
  4. Hold the tablet's **power button ~15 s**. The screen might not change.
  5. Keep the cable button held **~10 more s**, then release.

Confirm the host sees it:
```bash
lsusb | grep 05c6:9008      # → "Qualcomm, Inc. Gobi Wireless Modem (QDL mode)"  (Linux)
# macOS: system_profiler SPUSBDataType | grep -i 9008
```

> [!NOTE]
> On this model a plain **power-button "restart" does not reach EDL**, and the usual
> recovery/volume-key combos didn't either. The **deep-flash cable was the only reliable way in.**

## Make a stock backup first

If your device still boots, **do this before you ever touch `boot`** — this backup is your
lifeline. Drop into EDL (`adb reboot edl`) and read the partitions out:

```bash
edl --memory=ufs r boot_a      boot_a.img
edl --memory=ufs r boot_b      boot_b.img          # the active slot is the one that matters
edl --memory=ufs r vbmeta_a    vbmeta_a.img        # optional, for completeness
edl --memory=ufs r vbmeta_b    vbmeta_b.img
edl reset
```

Keep `boot_<active-slot>.img` somewhere safe and offline. (Find the active slot with
`adb shell getprop ro.boot.slot_suffix` while the device boots.) These are your device's own
firmware images — back them up, don't redistribute them.

## Restore (the part that un-bricks it)

With the tablet in EDL (`lsusb` shows `05c6:9008`) and your good `boot` image on the host:

```bash
# write the good image to the active slot's boot partition (here: slot B)
edl --memory=ufs w boot_b boot_b.stock.img        # ~90 MB, ~4 min

# optional integrity check: read it back and compare hashes
edl --memory=ufs r boot_b verify.img
sha256sum verify.img boot_b.stock.img             # must match

# leave EDL and boot
edl reset
```

Then **unplug the deep-flash cable**, put the tablet on its **normal charger**, and hold power
for up to 30 s until it restarts. It should boot the restored system.

There's a small wrapper in [`scripts/edl-restore.sh`](../scripts/edl-restore.sh) that runs the
write + verify + reset for you.

## Gotchas learned the hard way

- **Run `edl` in the foreground.** Backgrounding it over SSH can detach so the job never really
  starts. A stuck `edl` that can't reach the device will **peg a CPU core at 100 %** — kill it with
  `pkill -9 -f edl`.
- **Native USB-A only** (repeated because it cost us hours): USB-C adapters silently fail.
- **E-ink keeps its picture.** In EDL, or powered off, the screen may still show the last thing it displayed. That's normal.
- **Don't panic at a black screen.** After a bad `boot` flash the device looks dead; it's still
  reachable over EDL with the cable. A brick here is a soft brick.
- Keep the EDL tool + your stock `boot` image together and offline — that's the entire rescue kit.
