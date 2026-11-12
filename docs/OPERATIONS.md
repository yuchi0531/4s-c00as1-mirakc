# 運用

## 起動・停止

```sh
make up HOST=192.168.1.154
make down HOST=192.168.1.154
```

実機側で直接:

```sh
sh /media/usb/ondevice-up.sh
sh /media/usb/ondevice-down.sh
```

## ログ

```sh
# 実機
tail -f /mnt/t4s/var/log/mirakc.log

# tuner / decode-filter の標準エラーは mirakc のログに混ざる
```

`RUST_LOG=debug` を渡す場合:

```sh
RUST_LOG=debug sh /media/usb/ondevice-up.sh
```

## API

```sh
curl -s http://192.168.1.154:40772/api/version
curl -s http://192.168.1.154:40772/api/channels
curl -s http://192.168.1.154:40772/api/tuners
```

チャンネル配信（BS 2K、復号あり）:

```sh
curl -sS --max-time 15 \
  'http://192.168.1.154:40772/api/channels/BS/BS15_0/stream?decode=1' \
  -o bs.ts
ffprobe -hide_banner bs.ts
```

`decode=0` はスクランブルされたままです。`decode=1` で `t4s-cas` が
decode-filter として MULTI2 を復号します。

## 状態がおかしくなったとき

1. `sh /media/usb/ondevice-down.sh`
2. 必要なら `reboot`（USB イメージは消えません）
3. 再起動後、USB が自動マウントされていない場合は:
   ```sh
   mkdir -p /media/usb
   mount -t vfat /dev/sda1 /media/usb
   sh /media/usb/ondevice-up.sh
   ```

製品側が「受信できません」になった場合は、電源を抜き差ししてください。
本構成は `/mmc`・`/mmc2`・squashfs root を変更しません。

## バイナリ更新

ホストで `make build` / `make mirakc` の後、HTTP サーバを起動して:

```sh
make deploy HOST=192.168.1.154
```

`deploy.sh` は USB の ext4 イメージへバイナリと設定を転送し、mirakc を再起動します。

## 4K を有効にする

`docs/4K-NO-HDMI.md` のとおり、本体がデコードを開始する状態
（HDMI シンク / ダミープラグ）が必要です。画面を出さないダミープラグを
HDMI 端子に付けた後、BS4K チャンネルを要求してください。

```sh
curl -sS --max-time 15 \
  'http://192.168.1.154:40772/api/channels/BS4K/45328/stream?decode=1' \
  -o bs4k.ts
ffprobe -hide_banner bs4k.ts
```
