# 03 · Notes & gotchas (the things that cost us days)

Onyx's Android 13 on the Note Max deviates from stock in ways that quietly break the normal root
workflow. If you're rooting this device — or debugging why "my Magisk boot script never runs" —
read this. Everything below is observed behaviour on firmware **v4.2 (`TKQ1.230721.002`)**.

## 1. The verity flag will brick you
The device is **system-as-root with dm-verity / AVB enforcing** and a **locked bootloader**.
A `boot` patched with verity **disabled** is rejected by the verified-boot chain → hang on the
ONYX logo. Always patch with **`KEEPVERITY=true KEEPFORCEENCRYPT=true`**. See
[01-rooting](01-rooting.md). Recovery from a bad flash is [EDL](02-unbrick-edl.md).

## 2. The Magisk *app* can't grant `su` on Onyx 4.2
The Magisk **daemon** (`magiskd`) works — boot-time and daemon-spawned processes get real root.
But the Magisk **app's** root service crashes on launch: Onyx modified `ActivityManagerService`
so that `addPackageDependency` throws for the app's root process (`RootServerMain`), so the app
never finishes starting and can **never display a superuser prompt**. Consequently:
- Interactive `su` from an app or `adb shell` gets **no prompt → denied**.
- You can't "just tap allow." Root has to be used from contexts that are **auto-granted** (below).

## 3. `magiskd` does **not** run `/data/adb/service.d` here
The usual "drop a script in `service.d` / `post-fs-data.d`" trick is **silently dead** on this
device — the scripts are never executed (a marker file test never fired). So the standard way to
run something as root at boot doesn't work. Our workaround is in [boot-service/](../boot-service/).

## 4. `init` can't help you the obvious ways
- `init` runs as `u:r:init:s0` and **cannot even `lstat` `/data/adb`** ("Permission denied") — that
  dir is magisk-context only. So an `init` action can't write your script into `/data/adb`.
- The **magisk SELinux domain cannot `entrypoint` `/system/bin/sh`** — only Magisk's *own* binaries.
  An `init` `service`/`exec` that runs `sh` in `u:r:magisk:s0` **SIGABRTs inside init's
  `Service::Start`** (you'll see init itself abort in the tombstone).
- **Android 13 `init` SIGKILLs the entire process group** of a oneshot `exec` when it finishes
  ("Sending signal 9 to service … process group"), so anything your boot hook backgrounds is reaped.
- Magisk's `overlay.d/sbin` staging only decompresses Magisk's **own** known `.xz` (magisk,
  init-ld, stub) — dropping your **own** `.xz` there does **not** stage it to `/debug_ramdisk`.

## 5. What actually works: exec the magisk binary + `su` auto-grant + `setsid`
Two facts unlock everything:
- **`init` _can_ reach `/debug_ramdisk/magisk`** (it's how Magisk runs its own `--boot-complete`),
  even though it can't reach `/data/adb`. Running the **magisk binary** (not `sh`) in
  `u:r:magisk:s0` does **not** abort.
- **`magisk su` auto-grants to a uid-0 caller.** Since `init` is uid 0, `magisk su -c '…'` from an
  `init` `exec` runs your command as root **with no app / prompt** — sidestepping the broken app.
  The su'd shell is spawned by `magiskd` (a survivor), and **`setsid`** puts your long-running
  process in its own session so `init`'s process-group SIGKILL can't reach it.

The reusable pattern (full working example in [boot-service/](../boot-service/)):
```
# Magisk overlay.d init hook, baked into the patched boot:
on property:sys.boot_completed=1
    exec u:r:magisk:s0 root root -- /debug_ramdisk/magisk su -c /data/adb/<your-bootstrap>.sh
```
…where `<your-bootstrap>.sh` (now running as real root) `setsid`-launches your actual service so it
survives, and can freely read/write `/data/adb`, do `input`, toggle radios, write `/sys`, etc.

## 6. Onyx freezes your apps
Onyx's power manager (`ApplicationFreezeHelper`) **freezes third-party apps** (`pm` shows
`enabled=3`) at boot/idle, so an app you installed to run at boot may be disabled. Turn off
**"Freeze new apps"** (Onyx *Settings → battery/power → app freeze*) and allow the app to
auto-start. Root scripts can re-enable a specific app with `pm enable <package>`.

## 7. Small practical notes
- Enable **USB debugging** via Onyx's "USB Debug Mode" toggle (Developer options).
- Find the active slot: `adb shell getprop ro.boot.slot_suffix` → flash `boot_<suffix>`.
- Inspect boot behaviour with `adb shell 'logcat -b all -d | grep -i init'` and tombstones under
  `/data/tombstones`.
- `magisk -c` (version string) is your quick "is the daemon alive / am I rooted" check.

- **Persistent ADB over Wi-Fi.** `adb tcpip 5555` does **not** survive a reboot. If you have a
  root boot-service (see [../boot-service/](../boot-service/)), re-enable it every boot with
  `setprop service.adb.tcp.port 5555; stop adbd; start adbd`, then `adb connect <device-ip>:5555`.
  (If your service deep-suspends the SoC, Wi-Fi is off during suspend — the device is only
  reachable while awake.)

### 8. If you suspend the SoC, keep an RTC wake-alarm armed
`echo mem > /sys/power/state` from a root script is a great battery win on this e-ink device
(the panel holds its last image while the CPU sleeps). **But arm a wake source first, and keep it
armed for the whole sleep** — write your next wake time to `/sys/class/rtc/rtc0/wakealarm`
(`echo 0 > wakealarm; echo +<seconds> > wakealarm`) *before* you sleep, and don't clear it until
you actually wake. If you sleep with **no** alarm armed, Android's own autosleep can suspend the
device on battery with nothing scheduled to wake it — it then sits dead until someone presses the
power button. Re-arm each pass so the target stays correct across early wakeups:
```sh
WAKE_AT=$(( $(date +%s) + INTERVAL ))
while [ $(date +%s) -lt $WAKE_AT ]; do
  rem=$(( WAKE_AT - $(date +%s) ))
  echo 0 > /sys/class/rtc/rtc0/wakealarm; echo +$rem > /sys/class/rtc/rtc0/wakealarm
  echo mem > /sys/power/state 2>/dev/null      # suspends on battery; returns immediately on charger
  sleep 20
done
```

---

*These are field notes, not gospel — Onyx changes things between firmware versions. Re-verify on
your unit, and please open an issue/PR if your revision behaves differently.*
