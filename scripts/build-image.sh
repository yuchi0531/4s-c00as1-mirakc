#!/bin/sh
set -eu
ROOT=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
IMG="$ROOT/.tmp/img"
HTTP="$ROOT/.tmp/http"
mkdir -p "$IMG" "$HTTP"
if [ ! -f "$IMG/t4s.img" ]; then
  truncate -s 512M "$IMG/t4s.img"
  mkfs.ext4 -F -q -I 256 \
    -O ^64bit,^metadata_csum,^metadata_csum_seed,^orphan_file,^casefold,^verity,^project,^ea_inode \
    "$IMG/t4s.img"
fi
if [ ! -f "$IMG/t4s.img.gz" ]; then gzip -1 -k "$IMG/t4s.img"; fi
if [ ! -f "$IMG/alpine.tar.gz" ]; then
  curl -fsSL -o "$IMG/alpine.tar.gz" \
    https://dl-cdn.alpinelinux.org/alpine/v3.20/releases/armhf/alpine-minirootfs-3.20.0-armhf.tar.gz
fi
cp "$IMG/t4s.img.gz" "$IMG/alpine.tar.gz" "$HTTP/"
cp "$ROOT/config/config.yml" "$ROOT/config/strings.yml" "$HTTP/" 2>/dev/null || true
cp "$ROOT/scripts/ondevice-up.sh" "$ROOT/scripts/ondevice-down.sh" "$HTTP/"
echo "image ready: $HTTP"
