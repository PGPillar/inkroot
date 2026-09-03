#!/system/bin/sh
# Runs as ROOT (uid 0, u:r:magisk:s0) at every boot, invoked by the overlay.d init hook.
# Installs your service into root-persistent storage and launches it DETACHED so it survives
# init's boot-exec process-group SIGKILL.
LOG=/data/local/tmp/bootsvc.log
echo "boot $(date) uid=$(id -u) ctx=$(cat /proc/self/attr/current 2>/dev/null)" >> $LOG

# dev convenience: adopt an updated service.sh pushed to /data/local/tmp (push + reboot to update)
[ -f /data/local/tmp/service.sh ] && { cp -f /data/local/tmp/service.sh /data/adb/service.sh; chmod 0755 /data/adb/service.sh; }

# optional one-shot root command: drop /data/local/tmp/oneshot.sh -> runs once as root, then consumed
if [ -f /data/local/tmp/oneshot.sh ]; then
  echo "oneshot $(date)" >> $LOG
  /system/bin/sh /data/local/tmp/oneshot.sh >> /data/local/tmp/oneshot.out 2>&1
  rm -f /data/local/tmp/oneshot.sh
fi

# launch the service detached (new session) so init's process-group kill can't reap it
if [ -f /data/adb/service.sh ]; then
  pgrep -f /data/adb/service.sh >/dev/null 2>&1 || \
    setsid /system/bin/sh /data/adb/service.sh </dev/null >/dev/null 2>&1 &
fi
