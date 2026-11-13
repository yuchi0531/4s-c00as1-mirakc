#!/bin/sh
# ondevice-up.sh — run on the 4S-C00AS1 (outside the chroot).
#
# Mounts the USB ext4 image, starts mirakc inside Alpine, and starts the
# t4s-remote web UI on the host side (LAN, port 8484).  Nothing is written to
# /mmc, /mmc2 or the squashfs root.
#
# Both steps are idempotent.  A t4s-remote failure never stops mirakc, so this
# can be used as the post-reboot bring-up script.
set -eu

USB=/media/usb
IMG="$USB/t4s.img"
ROOT=/mnt/t4s/alpine
DATA=/mnt/t4s
LOGDIR="$DATA/var/log"
PIDFILE="$DATA/run/mirakc.pid"
REMOTE_BIN="$DATA/opt/t4s/bin/t4s-remote"
REMOTE_PID=/tmp/t4s-remote.pid
REMOTE_LOG=/tmp/t4s-remote.log
REMOTE_PORT=${REMOTE_PORT:-8484}

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
else
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
fi

# Start the remote UI after the USB image is mounted.  Never abort on failure:
# mirakc and the rest of the device must keep working even if t4s-remote is
# missing or cannot bind.
start_remote() {
    if [ ! -x "$REMOTE_BIN" ]; then
        echo "t4s-remote not installed, skipping ($REMOTE_BIN)"
        return 0
    fi
    if [ -f "$REMOTE_PID" ] && kill -0 "$(cat "$REMOTE_PID")" 2>/dev/null; then
        echo "t4s-remote already running (pid $(cat "$REMOTE_PID"))"
        return 0
    fi
    rm -f "$REMOTE_PID"
    echo "t4s-remote starting on port $REMOTE_PORT"
    if ! "$REMOTE_BIN" --daemon --port "$REMOTE_PORT"; then
        echo "t4s-remote start failed (mirakc unaffected); see $REMOTE_LOG" >&2
        return 0
    fi
    sleep 1
    if [ -f "$REMOTE_PID" ] && kill -0 "$(cat "$REMOTE_PID")" 2>/dev/null; then
        echo "t4s-remote started (pid $(cat "$REMOTE_PID"), port $REMOTE_PORT)"
    else
        echo "t4s-remote did not stay up; see $REMOTE_LOG" >&2
    fi
    return 0
}

# Browser bursts (page + status + key POSTs) were overflowing the listen/SYN
# queues; the host saw ~2/5 connections to 8484 fail while mirakc 40772 did
# too.  Raise the queues best-effort.  A failure here must not affect boot.
if [ -w /proc/sys/net/core/somaxconn ]; then
    echo 1024 > /proc/sys/net/core/somaxconn 2>/dev/null || true
fi
if [ -w /proc/sys/net/ipv4/tcp_max_syn_backlog ]; then
    echo 1024 > /proc/sys/net/ipv4/tcp_max_syn_backlog 2>/dev/null || true
fi

start_remote
