#!/bin/sh
# ondevice-remote.sh — start/stop the t4s-remote web UI on the 4S-C00AS1.
#
# usage: sh scripts/ondevice-remote.sh {start|stop|restart|status} [host]
#
# Runs on the host and talks to the device over the busybox telnet shell
# (port 2323), like scripts/telnet-run.sh.  The binary is expected at
# /mnt/t4s/opt/t4s/bin/t4s-remote (see docs/REMOTE-UI.md for the build and
# deploy steps) and manages /tmp/t4s-remote.log and /tmp/t4s-remote.pid.
#
# `start` is idempotent: a live pid is left alone, a stale pid file is removed.
# The UI itself is unauthenticated.  Keep it on a trusted LAN.
set -eu

ACTION=${1:-}
HOST=${2:-${HOST:-192.168.1.154}}
PORT=${PORT:-8484}
BIN=/mnt/t4s/opt/t4s/bin/t4s-remote
PIDFILE=/tmp/t4s-remote.pid
LOGFILE=/tmp/t4s-remote.log

# Busybox telnetd needs a moment before `exit`, otherwise the output of the
# command is lost with the session teardown.  The device<->host path also drops
# the occasional handshake, so retry when the session produced nothing at all.
run() {
    tries=0
    while [ "$tries" -lt 3 ]; do
        out=$({ printf '%s\n' "$1"; sleep 4; printf 'exit\n'; } \
              | timeout 30 nc "$HOST" 2323 || true)
        if [ -n "$out" ]; then
            printf '%s\n' "$out"
            return 0
        fi
        tries=$((tries + 1))
        sleep 1
    done
    return 1
}

case "$ACTION" in
    start)
        # Runs on the device.  Every branch prints something, so run() does not
        # treat a successful no-op as a lost session and start it a second time.
        run "
P=\$(cat $PIDFILE 2>/dev/null)
if [ -n \"\$P\" ] && kill -0 \"\$P\" 2>/dev/null; then
    echo \"already running: pid \$P, http://$HOST:$PORT/\"
else
    rm -f $PIDFILE
    if [ ! -x $BIN ]; then
        echo \"t4s-remote missing: $BIN\"
        exit 1
    fi
    $BIN --daemon --port $PORT
    sleep 1
    P=\$(cat $PIDFILE 2>/dev/null)
    if [ -n \"\$P\" ] && kill -0 \"\$P\" 2>/dev/null; then
        echo \"started: pid \$P, http://$HOST:$PORT/\"
        tail -2 $LOGFILE 2>/dev/null
    else
        echo \"start failed; $LOGFILE:\"
        tail -5 $LOGFILE 2>/dev/null
        exit 1
    fi
fi
"
        ;;
    stop)
        run "
P=\$(cat $PIDFILE 2>/dev/null)
if [ -n \"\$P\" ] && kill -0 \"\$P\" 2>/dev/null; then
    kill \$P
    echo \"stopped: pid \$P\"
else
    echo \"not running (stale pid '\$P')\"
fi
rm -f $PIDFILE
"
        ;;
    restart)
        "$0" stop "$HOST"
        sleep 1
        "$0" start "$HOST"
        ;;
    status)
        run "P=\$(cat $PIDFILE 2>/dev/null); echo pid=\$P; ps -A | grep t4s-remote; netstat -ltn 2>/dev/null | grep :$PORT; tail -3 $LOGFILE 2>/dev/null"
        ;;
    *)
        echo "usage: sh scripts/ondevice-remote.sh {start|stop|restart|status} [host]" >&2
        exit 2
        ;;
esac
