# 遠隔のみでの再起動・復旧

本体の電源ボタンを押すと reboot がかかり、USB のマウントと mirakc が
停止した状態になります。telnet は起動直後から使えるため、物理操作なしで
次を実行すれば元に戻せます。

```sh
# 1. telnet
nc 192.168.1.154 2323

# 2. USB (FAT32) をマウント
mkdir -p /media/usb
grep -q ' /media/usb vfat ' /proc/mounts || mount -t vfat /dev/sda1 /media/usb

# 3. ext4 イメージ + Alpine + mirakc を起動
sh /media/usb/ondevice-up.sh

# 4. 確認（ホスト側）
# curl -s http://192.168.1.154:40772/api/version
```

`ondevice-up.sh` は `/media/usb/t4s.img` を loop0 で `/mnt/t4s` にマウントし、
Alpine rootfs に `/dev`・`/proc`・`/sys`・`/tmp` を bind して mirakc を
chroot 内で起動します。内部ストレージは触りません。

## 注意

- **`Tune=7` を送らない**。製品の SI がクリアされた状態で止まり、
  再起動が必要になります（Phase 0 の試験で確認済み）。
- BS4K は HDMI シンク（ダミープラグを含む）が必要です。遠隔だけでは
  デコードパイプラインを開始できません。
- mirakc が落ちても telnet は残ります。`ondevice-up.sh` を再実行してください。
- どうしても telnet も応答しない場合は、電源の抜き差しが必要です。
