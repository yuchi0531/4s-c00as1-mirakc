#!/bin/sh
# softreboot.sh <host> — DIAGNOSTIC ONLY.
#
# A clean 2K capture no longer needs this: the DEMUX output ring is large enough
# that the ARC never wedges (docs/CS-STATUS.md).  It is kept for the case where
# an earlier build (or a manual experiment) has left a DEMUX/ring alive and the
# next `Source::open` fails with os error 1038.  There is no software way to
# release that state; only a reboot clears it.
#
# This only reboots the set-top box itself.  It never touches physical power,
# and it does not mount, tune, send keys or run mirakc.
set -eu
HOST=${1:-192.168.1.154}
ROOT=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)

echo "[*] soft-rebooting $HOST (diagnostic recovery for a wedged DEMUX) ..."
sh "$ROOT/scripts/telnet-run.sh" "$HOST" 'sync; (sleep 1; reboot) & echo rebooting'

echo "[*] waiting for the box to come back (expect ~90-110 s) ..."
for _ in $(seq 1 60); do
    if timeout 3 nc -z "$HOST" 2323 2>/dev/null; then
        echo "[*] $HOST telnet is back"
        exit 0
    fi
    sleep 3
done
echo "[!] $HOST did not answer on 2323 within ~180 s" >&2
exit 1
