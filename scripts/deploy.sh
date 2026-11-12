#!/bin/sh
# deploy.sh — push binaries/config to the device over HTTP pull + telnet.
# The device must already have the ext4 image + Alpine rootfs (docs/SETUP.md).
set -eu
HOST=${1:-192.168.1.154}
ROOT=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
HTTP="$ROOT/.tmp/http"
PORT=${HTTP_PORT:-8080}

cp "$ROOT/config/config.yml" "$ROOT/config/strings.yml" "$HTTP/"
cp "$ROOT/scripts/ondevice-up.sh" "$ROOT/scripts/ondevice-down.sh" "$HTTP/"

echo "[*] Start the HTTP server in another terminal: make serve"
echo "[*] Transferring to $HOST ..."

sh "$ROOT/scripts/telnet-run.sh" "$HOST" "
set -e
mkdir -p /media/usb
grep -q ' /media/usb vfat ' /proc/mounts || mount -t vfat /dev/sda1 /media/usb
if ! grep -q ' /mnt/t4s ext4 ' /proc/mounts; then
  losetup /dev/loop0 /media/usb/t4s.img 2>/dev/null || true
  mount -t ext4 /dev/loop0 /mnt/t4s
fi
mkdir -p /mnt/t4s/opt/t4s/bin /mnt/t4s/opt/t4s/epg /mnt/t4s/run /mnt/t4s/var/log
cd /mnt/t4s/opt/t4s/bin
for f in mirakc t4s-tuner t4s-cas; do
  wget -q http://192.168.100.100:$PORT/\$f -O \$f
  chmod 755 \$f
done
wget -q http://192.168.100.100:$PORT/config.yml -O ../config.yml
wget -q http://192.168.100.100:$PORT/strings.yml -O ../strings.yml
cd /media/usb
wget -q http://192.168.100.100:$PORT/ondevice-up.sh -O ondevice-up.sh
wget -q http://192.168.100.100:$PORT/ondevice-down.sh -O ondevice-down.sh
chmod 755 ondevice-*.sh
echo deployed
"

echo "[*] Restarting mirakc ..."
sh "$ROOT/scripts/telnet-run.sh" "$HOST" "
kill -TERM \$(cat /mnt/t4s/run/mirakc.pid 2>/dev/null) 2>/dev/null || true
sleep 1
rm -f /mnt/t4s/run/mirakc.pid
sh /media/usb/ondevice-up.sh
"
