# 4S-C00AS1 mirakc

SHARP **4S-C00AS1**（ViXS XCode6 / ARMv7）上で、[mirakc-BS4K](https://github.com/yuchi0531/mirakc-BS4K)
を **USB メモリ上の Alpine Linux chroot** で動かすための構成です。

チューナー取得と CAS 復号を別プロセスに分け、mirakc の
`tuners[].command` と `filters.decode-filter` に接続します。

```
mirakc (Alpine chroot, USB)
  ├─ tuners[].command : t4s-tuner  … 選局と TS/TLV 取り出し（CAS なし）
  └─ filters.decode-filter : t4s-cas … BS/CS 2K の MULTI2 復号（BS4K は素通し）
```

## 対応状況

| 放送 | 経路 | 状態 |
|---|---|---|
| BS 2K (ISDB-S, MPEG-TS) | `t4s-tuner` が生 TS、`t4s-cas` が本体 CAS で MULTI2 復号 | 実機で復号・映像確認済み（mirakc API 15s / 38.99MB / MPEG-2 1440x1080 + AAC） |
| CS 2K | 同上（TSID は PAT から学習） | 選局・ロックまでは確認。この環境では DEMUX 出力 0 バイト（信号/TSID 未確定） |
| BS4K (ISDB-S3, MMT/TLV) | 本体 ACAS が復号済みの ES を XCode6 REMUX で MPEG-TS 化 | 本体が視聴（LED 緑）状態のときのみ。HDMI 無しでは本体がデコードを開始しない |

BS4K の詳細と制約は [docs/4K-NO-HDMI.md](docs/4K-NO-HDMI.md) を参照してください。

## クイックスタート

```sh
# 1. ビルド（ホスト）
make build          # rust/target/.../t4s-tuner, t4s-cas, t4s-probe
make mirakc         # mirakc-BS4K を armv7-musl でビルド

# 2. USB 用 ext4 イメージと Alpine rootfs を用意
make image          # .tmp/img/t4s.img.gz, alpine.tar.gz

# 3. 実機へ配置（telnet 192.168.1.154:2323）
make deploy HOST=192.168.1.154

# 4. 起動
make up HOST=192.168.1.154
```

完全な手順（telnet 開通から API 確認まで）は [docs/SETUP.md と docs/OPERATIONS.md / docs/REMOTE.md](docs/SETUP.md)。

## ライセンス / 帰属

- 本リポジトリの `rust/t4s-core` は
  [Karin-Laboratory/mkra_4S-C00AS1](https://github.com/Karin-Laboratory/mkra_4S-C00AS1)
  （MIT、原作者 otti83 さん）の実装を分割・再利用しています。`NOTICE` を参照。
- mirakc は [mirakc/mirakc](https://github.com/mirakc/mirakc)（MIT）および
  [yuchi0531/mirakc-BS4K](https://github.com/yuchi0531/mirakc-BS4K) のフォークです。
