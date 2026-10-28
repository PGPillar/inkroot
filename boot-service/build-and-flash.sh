#!/usr/bin/env bash
# Build a Magisk-patched boot with the boot-service init hook baked in, then flash it over EDL.
# Usage: ./build-and-flash.sh <Magisk.apk> <stock-boot.img> <boot-partition>
#   e.g. ./build-and-flash.sh Magisk-v30.7.apk boot_b.stock.img boot_b
# Requires: adb (device booted with USB debugging), and `edl` on PATH for the flash step.
set -euo pipefail
APK=${1:?Magisk apk}; STOCK=${2:?stock boot image}; PART=${3:?boot partition, e.g. boot_b}
HERE="$(cd "$(dirname "$0")" && pwd)"

echo ">> extracting Magisk binaries from $APK"
rm -rf mgk_extract && unzip -o "$APK" -d mgk_extract >/dev/null

echo ">> pushing to device"
adb shell 'rm -rf /data/local/tmp/mgk && mkdir -p /data/local/tmp/mgk'
adb push mgk_extract/lib/arm64-v8a/libmagiskboot.so /data/local/tmp/mgk/magiskboot >/dev/null
adb push mgk_extract/lib/arm64-v8a/libmagiskinit.so /data/local/tmp/mgk/magiskinit >/dev/null
adb push mgk_extract/lib/arm64-v8a/libmagisk.so     /data/local/tmp/mgk/magisk     >/dev/null
adb push mgk_extract/lib/arm64-v8a/libinit-ld.so    /data/local/tmp/mgk/init-ld    >/dev/null
adb push mgk_extract/assets/boot_patch.sh mgk_extract/assets/util_functions.sh \
         mgk_extract/assets/stub.apk "$HERE/init.bootsvc.rc" /data/local/tmp/mgk/ >/dev/null
# Don't name the input stock_boot.img: Magisk writes its own copy under that name and would truncate it.
adb push "$STOCK" /data/local/tmp/mgk/boot.img >/dev/null
# seed the service scripts the hook will install (edit these before running, or push later)
adb push "$HERE/bootsvc.sh" /data/local/tmp/bootsvc.sh >/dev/null
adb push "$HERE/example-service.sh" /data/local/tmp/service.sh >/dev/null

echo ">> patching (KEEPVERITY=true — required on this device) + baking init hook"
adb shell 'cd /data/local/tmp/mgk && chmod 755 boot_patch.sh magiskboot magiskinit magisk init-ld && \
  KEEPVERITY=true KEEPFORCEENCRYPT=true PATCHVBMETAFLAG=false RECOVERYMODE=false sh boot_patch.sh boot.img && \
  ./magiskboot unpack new-boot.img && \
  ./magiskboot cpio ramdisk.cpio "add 644 overlay.d/init.bootsvc.rc init.bootsvc.rc" && \
  ./magiskboot cpio ramdisk.cpio "ls overlay.d" | grep init.bootsvc && \
  ./magiskboot repack new-boot.img patched-boot.img'
adb pull /data/local/tmp/mgk/patched-boot.img ./patched-boot.img >/dev/null
echo ">> built ./patched-boot.img  ($(shasum -a256 patched-boot.img | cut -c1-16))"

echo ">> flashing $PART over EDL"
adb reboot edl; sleep 12
edl --memory=ufs w "$PART" ./patched-boot.img
edl reset
echo ">> done. On boot, the hook installs bootsvc.sh + service.sh and launches your service as root."
echo "   To change the service later: edit example-service.sh, 'adb push it to /data/local/tmp/service.sh', reboot."
