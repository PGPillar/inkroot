#!/system/bin/sh
# EXAMPLE service — runs as ROOT, detached, for the life of the boot.
# Replace the body with whatever you want. Logs to /data/local/tmp/service.log.
exec >>/data/local/tmp/service.log 2>&1
echo "=== $(date) service start uid=$(id -u) ctx=$(cat /proc/self/attr/current 2>/dev/null) ==="

# Things root lets you do on this device (examples):
#   input keyevent 224                          # wake the screen
#   echo +1740 > /sys/class/rtc/rtc0/wakealarm   # arm an RTC wake (seconds from now)
#   echo mem   > /sys/power/state                # suspend the SoC (blocked while charging)
#   svc bluetooth enable | svc bluetooth disable # toggle radios
#   echo 1209600 > /sys/devices/system/cpu/cpufreq/policy0/scaling_max_freq  # cap CPU clock
#   settings put secure screensaver_enabled 0    # system settings

# simple heartbeat demo (replace me):
while :; do
  echo "$(date) alive; battery=$(dumpsys battery 2>/dev/null | sed -n 's/.*level: //p' | head -1)%"
  sleep 300
done
