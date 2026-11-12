# セットアップ手順（telnet 開通から API 確認まで）

対象: SHARP 4S-C00AS1（ViXS XCode6 / ARMv7 / Linux 3.10.24）
前提: 実機の telnet が開通していること（例: `192.168.1.154:2323`、root シェル）。
本手順は **本体の `/mmc`・`/mmc2`・squashfs root には書き込みません**。
すべて USB メモリ上の ext4 イメージに置きます。

## 0. 危険性と復旧

- メーカー非公式の操作です。自己責任で行ってください。
- 誤った FED メッセージや TSI 操作で、製品が「受信できません」状態になることがあります。
  その場合は電源を抜き差し（または `reboot`）で既知状態に戻します。
- 本手順は USB の読み書きのみを行います。内部ストレージの純正ファイルは変更しません。

## 1. 実機の確認（読み取りのみ）

```sh
nc 192.168.1.154 2323
# / # uname -a
# Linux vixs 3.10.24 ... armv7l GNU/Linux
# id
# uid=0(root) gid=0(root)
# ls -l /dev/sda1 /media/usb
```

USB を挿すと `/dev/sda1`（FAT32）が現れます。マウントされていない場合:

```sh
mkdir -p /media/usb
mount -t vfat /dev/sda1 /media/usb
```

## 2. ホスト側のビルド

必要なもの: Rust 1.99+（`rustup`）、`armv7-unknown-linux-musleabihf` target、
`mkfs.ext4`、`python3`、`ffprobe`。

```sh
make build          # t4s-tuner / t4s-cas / t4s-probe
make mirakc         # mirakc-BS4K (armv7-musl)
```

`make build` は `rust-lld` をリンカに使い、クロス GCC を要求しません
（`rust/.cargo/config.toml`）。

## 3. USB 用 ext4 イメージ

FAT32 のままでは Alpine の symlink / パーミッションが保持できないため、
USB 上に **ext4 のループバックイメージ**を置きます。

```sh
make image
# .tmp/img/t4s.img.gz  (512MB, ext4, 旧カーネル互換)
# .tmp/img/alpine.tar.gz (Alpine 3.20 armhf minirootfs)
```

`mkfs.ext4` は kernel 3.10 が解釈できない機能を無効化しています:

```sh
mkfs.ext4 -F -q -I 256 \
  -O ^64bit,^metadata_csum,^metadata_csum_seed,^orphan_file,^casefold,^verity,^project,^ea_inode \
  t4s.img
```

64bit / metadata_csum を有効にしたイメージは、このカーネルで
`mount: Invalid argument` になります。

## 4. 実機への転送（HTTP pull）

ホストで配信サーバを起動:

```sh
make serve            # http://0.0.0.0:8080/.tmp/http
```

実機（telnet）で取得:

```sh
cd /media/usb
wget -q http://192.168.100.100:8080/t4s.img.gz -O t4s.img.gz
gunzip -f t4s.img.gz
losetup /dev/loop0 t4s.img
mount -t ext4 /dev/loop0 /mnt/t4s

wget -q http://192.168.100.100:8080/alpine.tar.gz -O alpine.tar.gz
gunzip -f alpine.tar.gz
mkdir -p /mnt/t4s/alpine
tar xf alpine.tar -C /mnt/t4s/alpine
```

`192.168.100.100` はホストの IP に置き換えてください。

## 5. バイナリと設定の配置

```sh
mkdir -p /mnt/t4s/opt/t4s/bin /mnt/t4s/opt/t4s/epg
cd /mnt/t4s/opt/t4s/bin
for f in mirakc t4s-tuner t4s-cas; do
    wget -q http://192.168.100.100:8080/$f -O $f
    chmod 755 $f
done
wget -q http://192.168.100.100:8080/config.yml -O ../config.yml
wget -q http://192.168.100.100:8080/strings.yml -O ../strings.yml
```

起動スクリプトは USB の FAT32 側に置きます（毎回の再起動で使うため）:

```sh
cd /media/usb
wget -q http://192.168.100.100:8080/ondevice-up.sh -O ondevice-up.sh
wget -q http://192.168.100.100:8080/ondevice-down.sh -O ondevice-down.sh
chmod 755 ondevice-*.sh
```

## 6. 起動

```sh
sh /media/usb/ondevice-up.sh
```

`ondevice-up.sh` は次を行います:

1. `/media/usb`（FAT32）をマウント
2. `/media/usb/t4s.img` を loop0 で `/mnt/t4s` にマウント
3. Alpine rootfs `/mnt/t4s/alpine` に `/dev`, `/proc`, `/sys`, `/tmp`, `/dev/shm` を bind
4. `/mnt/t4s/opt/t4s` を chroot の `/opt/t4s` に bind
5. chroot 内で mirakc を起動（`PATH`, `RUST_LOG`, `MIRAKC_CONFIG` を設定）

ログ: `/mnt/t4s/var/log/mirakc.log`
PID: `/mnt/t4s/run/mirakc.pid`

停止:

```sh
sh /media/usb/ondevice-down.sh
```

## 7. API 確認

ホストから:

```sh
curl -s http://192.168.1.154:40772/api/version
curl -s http://192.168.1.154:40772/api/tuners
```

BS 2K の復号ストリーム（実測）:

```sh
curl -sS --max-time 15 \
  'http://192.168.1.154:40772/api/channels/BS/BS15_0/stream?decode=1' \
  -o bs.ts
ffprobe -hide_banner bs.ts
```

2026-11-13 の実測: 15 秒で 38,993,832 バイト、`ffprobe` で
MPEG-2 video 1440x1080 + AAC。`decode=1` で MULTI2 が解けた状態です。

`/api/services` は EPG キャッシュ（`services.json`）が必要です。
本構成では EPG ジョブを無効にしているため、空のままです。
EPGStation 連携には `mirakc-arib` / `mirakc-arib-tlv` の導入と
`jobs` の有効化が別途必要です。

## 8. 既知の制約

- **CS 2K**: PAT から実 TSID を実測し、`t4s-core/src/isdbs.rs` の
  `cs_tsid()` に反映済みです（`docs/analysis/tsid-111.md`）。
  ただし mirakc の `channels` に CS を登録すると `/api/channels` が
  停止するため、現在の `config.yml` からは外しています。
  `config/channels-cs.yml.example` に登録用の定義を残しています。
  t4s-tuner の CS 選局は実 TSID を FED に渡す実装まで完了していますが、
  この機体での DEMUX 出力確認は未完了です。
- **BS4K**: 本体が HDMI シンクを検出してデコードを開始しない限り、
  ES リングが作られないため取得できません。詳細は
  [4K-NO-HDMI.md](4K-NO-HDMI.md)。
- `/api/services` は EPG 未構築のため空です（チャンネル配信は可能）。
