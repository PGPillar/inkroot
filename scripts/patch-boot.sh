#!/usr/bin/env bash
# Magisk-patch a boot image with the verity flags this device REQUIRES.
# Patching runs on the device (magiskboot is an Android arm64 binary), via adb.
# Usage: ./patch-boot.sh <Magisk.apk> <stock-boot.img> [out.img]
set -euo pipefail
APK=${1:?Magisk apk}; STOCK=${2:?stock boot image}; OUT=${3:-patched-boot.img}
rm -rf mgk_extract && unzip -o "$APK" -d mgk_extract >/dev/null
adb shell 'rm -rf /data/local/tmp/mgk && mkdir -p /data/local/tmp/mgk'
adb push mgk_extract/lib/arm64-v8a/libmagiskboot.so /data/local/tmp/mgk/magiskboot >/dev/null
adb push mgk_extract/lib/arm64-v8a/libmagiskinit.so /data/local/tmp/mgk/magiskinit >/dev/null
adb push mgk_extract/lib/arm64-v8a/libmagisk.so     /data/local/tmp/mgk/magisk     >/dev/null
adb push mgk_extract/lib/arm64-v8a/libinit-ld.so    /data/local/tmp/mgk/init-ld    >/dev/null
adb push mgk_extract/assets/boot_patch.sh mgk_extract/assets/util_functions.sh \
         mgk_extract/assets/stub.apk /data/local/tmp/mgk/ >/dev/null
adb push "$STOCK" /data/local/tmp/mgk/stock_boot.img >/dev/null
# KEEPVERITY=true is mandatory here — verity is enforcing; patching it off bricks the device.
adb shell 'cd /data/local/tmp/mgk && chmod 755 boot_patch.sh magiskboot magiskinit magisk init-ld && \
  KEEPVERITY=true KEEPFORCEENCRYPT=true PATCHVBMETAFLAG=false RECOVERYMODE=false sh boot_patch.sh stock_boot.img'
adb pull /data/local/tmp/mgk/new-boot.img "$OUT" >/dev/null
echo "patched -> $OUT  ($(shasum -a256 "$OUT" | cut -c1-16))"
echo "flash it with:  adb reboot edl && edl --memory=ufs w boot_<slot> $OUT && edl reset"
