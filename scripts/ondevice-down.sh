#!/bin/sh
# ondevice-down.sh — stop mirakc and unmount the USB image.
set -eu

USB=/media/usb
DATA=/mnt/t4s
ROOT="$DATA/alpine"
PIDFILE="$DATA/run/mirakc.pid"

if [ -f "$PIDFILE" ]; then
    pid=$(cat "$PIDFILE")
    kill "$pid" 2>/dev/null || true
    sleep 1
    kill -9 "$pid" 2>/dev/null || true
    rm -f "$PIDFILE"
fi

for d in dev/shm dev proc sys tmp; do
    mountpoint -q "$ROOT/$d" && umount "$ROOT/$d" || true
done
mountpoint -q "$DATA" && umount "$DATA" || true
losetup -d /dev/loop0 2>/dev/null || true
echo "stopped"
