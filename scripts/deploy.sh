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
# Bundle the static WebUI when a local build exists.
WEBUI_DIST=${WEBUI_DIST:-$HOME/projects/mirakc-webui/dist}
if [ -d "$WEBUI_DIST" ]; then
    tar -C "$WEBUI_DIST" -cf "$HTTP/www.tar" .
fi
# Use the freshly built standalone crate binary when it exists; otherwise the
# copy already in .tmp/http is transferred.
REMOTE_BIN="$ROOT/rust/target/remote/armv7-unknown-linux-musleabihf/release/t4s-remote"
if [ -f "$REMOTE_BIN" ]; then
    cp "$REMOTE_BIN" "$HTTP/t4s-remote"
fi

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
for f in mirakc t4s-tuner t4s-cas t4s-remote; do
  wget -q http://192.168.100.100:$PORT/\$f -O \$f
  chmod 755 \$f
done
wget -q http://192.168.100.100:$PORT/config.yml -O ../config.yml
wget -q http://192.168.100.100:$PORT/strings.yml -O ../strings.yml
if wget -T 20 -q http://192.168.100.100:$PORT/www.tar -O /tmp/www.tar; then
  mkdir -p /mnt/t4s/opt/t4s/www
  tar -xf /tmp/www.tar -C /mnt/t4s/opt/t4s/www
  chown -R 0:0 /mnt/t4s/opt/t4s/www
  find /mnt/t4s/opt/t4s/www -type d -exec chmod 755 {} \;
  find /mnt/t4s/opt/t4s/www -type f -exec chmod 644 {} \;
else
  echo "www.tar not available, skipping WebUI"
fi
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
