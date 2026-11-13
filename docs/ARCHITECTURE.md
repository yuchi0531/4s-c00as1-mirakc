# 4S-C00AS1 mirakc 構成

## 全体

```
BS/CS アンテナ
   │
   ▼
SC1501A (ISDB-S / ISDB-S3)
   │  TS (2K) / TLV (4K)
   ▼
ViXS XCode6 ── 本体 ACAS（4K はここで復号）
   │
   ├─ 2K: DEMUX/TSI → t4s-tuner（生 TS） → mirakc decode-filter（t4s-cas / MULTI2）
   └─ 4K: MMT_DEMUX → ES リング → REMUX（ハードウェア） → t4s-tuner（MPEG-TS）
```

mirakc は USB 上の ext4 イメージ内の Alpine rootfs で動きます。
本体ストレージ（`/mmc`, `/mmc2`, squashfs root）には書きません。

## プロセス分離

### `t4s-tuner`（mirakc `tuners[].command`）

- `--type BS4K --channel <TLV StreamID>`: 本体の ADSB 選局メッセージを送り、
  ES リングが揃ったら REMUX を組み立てて 188 バイト TS を stdout へ出す。
- `--type BS|CS --channel BSxx_y|CSn`: FED に直接選局し、DEMUX の生 TS を stdout へ出す。
  **復号はしない**（scrambling フラグはそのまま）。
  CS も BS と同じ logical TSI1 → physical TSI0 remap を既定で使う。
  旧 native 経路は `T4S_CS_NATIVE_TSI=1`、物理ポートは `T4S_CS_TSI_PORT` で指定。
- 2K DEMUX の出力 ring は製品既定の **0x6000 units (4.6 MiB)** ではなく
  **0x18000 units (18 MiB)** で作る。ホスト側の読み出しは約 2 MB/s で、
  フル CS トランスポンダは約 5–7 MB/s あるため、既定サイズでは数秒で ring が
  埋まる。埋まった ring への書き込みで ARC がブロックし、`remove_pid` /
  `disconnect` / `stop` / `destroy` がすべて `XCE_TIMEOUT (1038)` になる。
  DEMUX が解放されず、**次の `Source::open` が `SDEV_CREATE` の 1038 で
  落ちる（1 回/起動 になる）**。18 MiB では通常の取得で埋まらない。
- 終了処理（`bs2k::Source::drop`）は念のため **まず demodulator の TS 出力を
  止め**（`quiesce_ts_output`）、ring を drain してから graph を解体する。
  ring を破棄できなかった場合は明示的にエラーを残す
  （次回起動が失敗することを隠さない）。
- `restore_frontend()` は best-effort。この個体は OEM tuner ELF のハッシュが
  参照ファームと違うため固定アドレス復元が使えず、失敗しても取得成功なら
  rc=0 のままにする。`--no-restore-frontend` で明示的にスキップできる。
- 引数は mirakc の Mustache:
  `--type {{{channel_type}}} --channel {{{channel}}} --duration {{{duration}}} -`

### `t4s-cas`（mirakc `filters.decode-filter`）

- `--type BS|CS`: stdin の TS から PAT/PMT/ECM を追跡し、本体 `card` デーモンに
  問い合わせて MULTI2 を復号する。全サービス同時復号モード。
- `--type BS4K`: 本体 ACAS が既に復号しているため、バイト列をそのまま通す。
- 引数: `--type {{{channel_type}}} --channel {{{channel}}}`

## なぜ分離するか

mkra は選局・取得・復号・HTTP 配信を 1 プロセスで行います。本構成では
mirakc が配信・録画・EPG を担当し、デバイス固有の部分だけを 2 コマンドにします。

- チューナーコマンドは「選局して生ストリームを出す」だけ。CAS の状態を持たない。
- CAS は decode-filter として、mirakc の `?decode=1` パイプラインにだけ参加する。
- 2K の復号鍵は本体 `card` デーモンから取得する。鍵・カード情報はログに出さない。

## BS4K の制約

BS4K は本体の MMT_DEMUX と ES リングが存在するときだけ取り出せます。
ES リングは本体が「視聴（LED 緑）」状態でデコードを開始したときに作られます。
HDMI を接続しない場合、本体はデコードを開始しないため、4K 経路は動きません。
詳細は [4K-NO-HDMI.md](4K-NO-HDMI.md)。
