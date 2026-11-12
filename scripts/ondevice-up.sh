#!/bin/sh
# ondevice-up.sh — run on the 4S-C00AS1 (outside the chroot).
#
# Mounts the USB ext4 image and starts mirakc inside Alpine.
# Nothing is written to /mmc, /mmc2 or the squashfs root.
set -eu

USB=/media/usb
IMG="$USB/t4s.img"
ROOT=/mnt/t4s/alpine
DATA=/mnt/t4s
LOGDIR="$DATA/var/log"
PIDFILE="$DATA/run/mirakc.pid"

mkdir -p "$USB" "$DATA"
if ! grep -q " $USB vfat " /proc/mounts; then
    mount -t vfat /dev/sda1 "$USB"
fi

if ! grep -q " $DATA ext4 " /proc/mounts; then
    losetup /dev/loop0 "$IMG" 2>/dev/null || true
    mount -t ext4 /dev/loop0 "$DATA"
fi

mkdir -p "$ROOT/dev" "$ROOT/proc" "$ROOT/sys" "$ROOT/tmp" "$ROOT/dev/shm"
for d in dev proc sys tmp; do
    mountpoint -q "$ROOT/$d" || mount --bind "/$d" "$ROOT/$d"
done
mountpoint -q "$ROOT/dev/shm" || mount --bind /dev/shm "$ROOT/dev/shm"

mkdir -p "$LOGDIR" "$DATA/run" "$DATA/opt/t4s/epg" "$ROOT/opt/t4s"
mountpoint -q "$ROOT/opt/t4s" || mount --bind "$DATA/opt/t4s" "$ROOT/opt/t4s"

if [ -f "$PIDFILE" ] && kill -0 "$(cat "$PIDFILE")" 2>/dev/null; then
    echo "mirakc already running (pid $(cat "$PIDFILE"))"
    exit 0
fi

chroot "$ROOT" /bin/sh -c '
    cd /opt/t4s
    export PATH=/usr/local/bin:/usr/bin:/bin
    export RUST_LOG=${RUST_LOG:-info}
    export MIRAKC_CONFIG=/opt/t4s/config.yml
    export TZ=Asia/Tokyo
    exec /opt/t4s/bin/mirakc
' >> "$LOGDIR/mirakc.log" 2>&1 &
echo $! > "$PIDFILE"
echo "mirakc started (pid $(cat "$PIDFILE"))"
