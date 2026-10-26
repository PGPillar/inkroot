#!/system/bin/sh
# Inkroot default boot service. This is the harmless default: it does nothing
# except write a small log so you can confirm that root is working.
# It runs as root, detached, for the life of the boot.
# Replace it with your own script any time. Inkroot only touches it again if you
# Root again or use "Reinstall boot service": both put this default back.
exec >>/data/local/tmp/service.log 2>&1
echo "=== $(date) service start uid=$(id -u) ctx=$(cat /proc/self/attr/current 2>/dev/null) ==="

# Heartbeat: one line with the battery level every 5 minutes.
while :; do
  echo "$(date) alive; battery=$(dumpsys battery 2>/dev/null | sed -n 's/.*level: //p' | head -1)%"
  sleep 300
done
