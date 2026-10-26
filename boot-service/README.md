# boot-service — run a script as root at every boot

> **Easiest: use the [Inkroot app](https://github.com/n0dal/inkroot#download).** It does every step in this guide for you, with checks at each step. This guide is the manual path for tinkerers.

On the Note Max (Onyx 4.2), Magisk's normal **`/data/adb/service.d` scripts never run**, and the
Magisk **app can't grant interactive `su`**. This is a tiny, reliable way to run *your own* script
as **real root at every boot** anyway. (Full reasoning in [../docs/03-notes-and-gotchas.md](../docs/03-notes-and-gotchas.md).)

## How it works
1. **`init.bootsvc.rc`** — a Magisk `overlay.d` init hook baked into the patched `boot`. At
   `boot_completed` it runs the **magisk binary** (which `init` *can* reach at `/debug_ramdisk`)
   and calls `magisk su -c` — **auto-granted because `init` is uid 0**, so no app/prompt.
2. **`bootsvc.sh`** — runs as root; installs your service into `/data/adb` and **`setsid`-launches
   it** so it survives Android 13's process-group SIGKILL of the boot exec.
3. **`service.sh`** — *your* script (start from `example-service.sh`), running as root, detached,
   for the whole boot. Read/write `/data/adb`, `input`, `/sys`, radios, settings — all fair game.

## Use it
```bash
# edit example-service.sh to do what you want, then:
./build-and-flash.sh Magisk-v30.7.apk boot_b.stock.img boot_b
```
That patches `boot` (verity-safe), bakes in the hook, seeds your scripts, and EDL-flashes.

**Change the service later without re-flashing** (the hook re-seeds from `/data/local/tmp` each boot):
```bash
adb push example-service.sh /data/local/tmp/service.sh
adb reboot
```

**Run a one-off root command** (handy for testing): drop `oneshot.sh` and reboot —
it runs once as root and its output lands in `/data/local/tmp/oneshot.out`:
```bash
printf '#!/system/bin/sh\nid; ls -la /data/adb\n' > oneshot.sh
adb push oneshot.sh /data/local/tmp/oneshot.sh && adb reboot
```

## Verify
```bash
adb shell 'cat /data/local/tmp/bootsvc.log'   # boot ... uid=0 ctx=u:r:magisk:s0
adb shell 'cat /data/local/tmp/service.log'   # your service's output
adb shell 'ps -A -o PID,PPID,ARGS | grep service.sh'   # PPID 1 = detached & surviving
```
