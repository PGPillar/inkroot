#!/usr/bin/env bash
# Write a good boot image back over EDL, verify, and reset. Device must already be in EDL (9008).
# Usage: ./edl-restore.sh <boot-partition> <image.img>
#   e.g. ./edl-restore.sh boot_b boot_b.stock.img
set -euo pipefail
PART=${1:?boot partition, e.g. boot_b}; IMG=${2:?image to write}
command -v edl >/dev/null || { echo "edl not on PATH — see docs/02-unbrick-edl.md"; exit 1; }
echo ">> writing $IMG -> $PART"
edl --memory=ufs w "$PART" "$IMG"
echo ">> verifying"
edl --memory=ufs r "$PART" /tmp/edl_verify.img
if [ "$(shasum -a256 /tmp/edl_verify.img | cut -d' ' -f1)" = "$(shasum -a256 "$IMG" | cut -d' ' -f1)" ]; then
  echo "   OK — hashes match"
else
  echo "   !! hash mismatch — do NOT reset; re-check cable/port and re-write"; exit 1
fi
edl reset
echo ">> reset sent. Unplug the deep-flash cable, use the normal charger, hold power ~5s."
