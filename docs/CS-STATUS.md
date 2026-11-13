## 修正（重要）

以下の「結論」は Phase 1 前半の誤り。正しい経路は **physical TSI0 → logical TSI0** で、
CS の生 TS 受信は成功している。詳細は末尾の「Phase 1 実機結果」。

# CS 2K の現状（2026-11-13）

## 結論

CS 2K は **mirakc への登録は成功**していますが、この 4S-C00AS1 では
**TS を取得できませんでした**。BS 2K は同じ経路で動作します。

## 実測

| 項目 | BS15_0 | CS2 |
|---|---|---|
| 復調 lock（`read_isdbs_status`） | `0x67` | `0x67` |
| `ts_pin` / `ts_serial` | `1` / `192` | `1` / `192` |
| 物理 TSI0 / 論理 TSI1（`map_tsi_one_to_zero`） | TS 出力あり | 0 バイト |
| 物理 TSI1 / 論理 TSI1（identity） | 未使用 | 0 バイト |
| TSID | 0x40f1 (16625) | 0x6020 (24608) / 0 |
| mirakc API | 200、MPEG-2 1440x1080 + AAC | 404（tuner open 失敗） |

CS2 は RF 的に lock まで到達します。しかし、試した 2 つの TSI 経路
（physical0→logical1 と physical1→logical1）のどちらでも DEMUX 出力が
0 バイトでした。TSID を 0（自動）と実測値 0x6020 の両方で試しています。

## 判断

- CS の RF/復調は生きているが、XCode6 の TSI/DEMUX 経路に CS 2K の
  TS が乗っていない可能性が高い。
- 4S-C00AS1 の製品ファームは BS4K/110°CS4K 向けで、CS 2K の
  TS 出力を公開していない、という説明と整合する。
- これ以上の CS 取得は、製品の TSI ルート解析（Phase 1）または
  別ハードウェアでの受信確認が必要。

## 登録状態

`config/config.full.yml` に BS 6 + CS 12 + BS4K 6 を登録し、
`/api/channels` は 200（1490 bytes）で一覧を返します。
CS のストリーム要求は tuner open 失敗で 404 になります。

## Phase 1 追加試験（2026-11-13 夜）

静的解析（`docs/analysis/phase1-cs4k.md`）で、製品の 2K DEMUX は常に
**logical TSI0** を使い、`Dxd_SetTsConfig` は route slot 0→physical TSI0、
slot 2→physical TSI1 であることを確認した。`abs` は crossbar `0x61f0` を
書かない（`xc_mini_device_config_tsi(...,0,0,-1)` のみ）。

これに合わせて `bs2k` に native 経路を追加し、CS2 を
**physical TSI1 → logical TSI0（remap なし）** で再試験した。
結果は依然 **0 バイト**。lock は `0x67`、`ts_serial=192` のまま。
TSID 自動/実測の両方で同じ。

4K は `adsb` の TLV demux(type 0x24) が 4 本の 4MiB リングを持ち、
外部タップは type 0x24 への追加 `connect_output` が正しい（ring の
max_reader=1 は製品 MMT が消費済み）。ただし TLV demux に選局入力が
無いため duplicate は 0 バイト。

`bAdsbInitialized` はレポート上の `abs:0x02027184` が
abs の実マップ（0x8000–0x10d7000）の外にあり、書き込みには使えない。
`abs` を SIGSTOP して直接 `0x6004` を送る試験も TLV は出ず、
約1分後に製品側ウォッチドッグで再起動した（abs は CONT 済み、復旧済み）。
このため製品プロセスの停止は避ける。

## 現在の登録状態

`config.full.yml`（BS6/CS12/BS4K6）で `/api/channels` 200 を一度確認。
CS 登録時に API が不安定になるため、安定運用は BS/BS4K の
`config/config.yml` に戻している。CS は `config.full.yml` で再現可能。

## Phase 1 実機結果（追記）

**CS の受信経路は特定できた。** 正解は **physical TSI0 → logical TSI0（remap なし）**。
`T4S_CS_TSI_PORT=0` で CS2 を 8 秒取得すると **49,119,888 バイト**の生 TS が
出力され、mirakc API の `?decode=0` でも 8 秒で **47,480,832 バイト**
（先頭 `47 1f ...`）を取得できた。製品の 2K DEMUX が logical TSI0 を
使うという静的解析結果と一致する。

```
# 正しい CS 経路
t4s-tuner --type CS --channel CS2 --duration 8 -   # physical TSI0 -> logical TSI0
```

### 復号のブロッカー

生 TS を `t4s-cas` に通すと、ECM 応答が
`ECM not entitled/unsupported response (requires 0800)` で停止する。
CS2（TBSチャンネル1）と CS8（ショップチャンネル）の両方で同じ。
4S-C00AS1 の内蔵 CAS は CS 2K のプロバイダ 0x0800 に entitlement が
無い可能性が高い。これはソフトウェアではなくカード契約/鍵の問題。

CS を復号するには、CS 2K の契約済み B-CAS/ACAS カード、または
正規の鍵供給が必要。`.111` のカードを流用してはいけない。

### 現在の mirakc

`config.full.yml`（BS6/CS12/BS4K6）で `/api/channels` 200。
CS は `decode=0` で受信可能、`decode=1` は CAS entitlement で 404。

## CS 無料/クリアサービスの映像確認（2026-11-13）

CS8（ショップチャンネル）のトランスポンダを `decode=0` で 6 秒取得
（35,651,584 バイト）。PAT/PMT を確認すると 4 サービス:

| program | video | audio | 状態 |
|---:|---|---|---|
| 55 | 1440x1080 MPEG-2 | AAC | clear（映像デコード可） |
| 218 | 1440x1080 MPEG-2 | AAC | clear（映像デコード可） |
| 318 | 0x0 | AAC | scrambled |
| 349 | 0x0 | AAC | scrambled |

program 218 の video/audio PID を抜き出し、`ffmpeg` で 1 フレームを
デコードしたところ、実映像（画面に `TOEI ch.` 表示）を確認した。
これはカード復号を必要としないクリアサービスで、CS の受信・映像確認は
完了している。`decode=1` は同じ TS 内の scrambled サービス（provider
0x0800）で CAS が停止するが、ユーザー指示により無視する。

mirakc の `decode=0` は全 TS を配信する。クリアサービスだけを選ぶには
`mirakc-arib filter-service` などのサービスフィルタが必要（未導入）。

## CS remap 経路（2026-11-14 01:14 JST）

製品が `MIXER/Sink=false` のまま CS キーも効かなかったため、BS で成功している
`physical TSI0 → logical TSI1` remap を CS に適用して直接 TS を取得した。
mirakc は停止したまま（再起動なし）。

### 実装

`rust/t4s-tuner/src/main.rs` の `run_2k`:

- 既存の CS: `T4S_CS_TSI_PORT`（既定 0）→ `(port, logical=0, remap=false)`
- 追加: `T4S_CS_REMAP=1` のとき `(port, logical=1, remap=true)`
  - `T4S_CS_TSI_PORT` 未指定なら `(0, 1, true)`（BS と同じ経路）
  - `T4S_CS_TSI_PORT=1` なら `(1, 1, true)`
- 環境変数なしの既存動作は変更なし。

ビルド・配置:

```text
cargo build --release --target armv7-unknown-linux-musleabihf -p t4s-tuner
新 binary: 550,812 bytes, md5 02b495734554844c787f4e6faa23525c
実機 /mnt/t4s/opt/t4s/bin/t4s-tuner と md5 一致、755
```

補足: 01:12 頃に実機が自然再起動し（uptime 97 秒）、`/mnt/t4s` が未マウント
だったため、mirakc を起動せず手動で `/media/usb` と `/mnt/t4s` を mount した。

### 実行結果

```sh
T4S_CS_REMAP=1 /mnt/t4s/opt/t4s/bin/t4s-tuner \
  --type CS --channel CS8 --duration 8 - > /tmp/cs2k-remap.ts 2> /tmp/cs2k-remap.log
```

- **13,484,488 bytes**（0 bytes ではない）
- 先頭 16 bytes: `47 10 0f 16 70 40 1c ad 45 b0 8a 60 7b 5d d0 e2`（先頭 `0x47`）
- ログ:
  - `mkra: opened ISDB-S RF 12411000 TSID 6080 / raw`
  - `t4s-tuner: CS CS8 -> RF 12411000 kHz TSID 6080`
- bytes > 0 のため `T4S_CS_TSI_PORT=1` の追加試行は行っていない。

### ffprobe

```text
format: mpegts / duration 2.60 s / bit_rate 41.46 Mbps
program 55:  mpeg2video Main 1440x1080, AAC LC 48000 Hz 2ch
program 218: mpeg2video 0x0, AAC 0 Hz 0ch
program 318: mpeg2video 0x0
program 349: mpeg2video 0x0
scrambled: 38,132 / 71,726 TS packets = 53.16%
```

program 55 の video PID `0x100F` はスクランブル PID に含まれず、クリア映像
（MPEG-2 Main 1440x1080 + AAC LC）を確認できた。program 218 の video PID
`0x100D` はスクランブル PID に含まれ、今回の取得では復号なしの映像は出なかった。
（過去の §CS 無料/クリアサービス の表では 218 を clear と記録しているが、
今回の 8 秒取得では 55 が clear、218 は scrambled だった。）

### 終了時の注意

`t4s-tuner` はデータ取得後に以下の cleanup エラーで **rc=1** になった。

```text
mkra: BS2K cleanup remove PID: No error information (os error 1038)
mkra: BS2K disconnect needs stop/retry: No error information (os error 1038)
mkra: BS2K cleanup stop retry: No error information (os error 1038)
mkra: BS2K cleanup disconnect retry: No error information (os error 1038)
mkra: BS2K cleanup ring ownership: retaining graph with live writer/readers
t4s-tuner: graph cleanup failed; refusing to restore the frontend
```

TS 自体は取得できているが、graph cleanup に失敗して frontend を戻せていない。
次の選局前に状態確認（必要なら再起動）が必要。

mirakc は停止状態を維持。キー送信・0x6004・電源操作は行っていない。

## CS remap 既定化と cleanup 修正（2026-11-14 01:33 JST）

### 実装

`rust/t4s-tuner/src/main.rs`:

- CS の既定を **`(physical=T4S_CS_TSI_PORT or 0, logical=1, remap=true)`** に変更
  （BS と同じ remap 経路）。
- 旧 native 経路 `(port, 0, false)` は `T4S_CS_NATIVE_TSI=1` で選択。
- `T4S_CS_TSI_PORT` は物理ポート指定として維持。
- `T4S_CS_REMAP` は後方互換のため受理（remap が既定なので実質 no-op）。
- usage に環境変数を追記。

`rust/t4s-core/src/bs2k.rs`:

- `Drop` の cleanup を best-effort 化。
- `No error information (os error 1038)` は remove PID / stop / disconnect の
  再試行後に benign として無視。
- remove PID / stop / disconnect を 1 回再試行。
- ring が live の場合は最大 10 × 100 ms 待ってから判定。
- ring が live でも後続の stop / destroy / pipeline cleanup / TSI restore を続行。
- `graph cleanup failed; refusing to restore the frontend` を廃止。
- `restore_frontend()` は常に呼ぶ。TS 取得成功 + frontend 復元成功なら rc=0、
  復元失敗時のみ rc=1。

ビルド・配置:

```text
t4s-tuner: 552,388 bytes, md5 01cfa56831ff28cfc6c0a05ecb8e90ed
実機 /mnt/t4s/opt/t4s/bin/t4s-tuner と md5 一致、755
mirakc は停止したまま
```

### 状態確認と復旧

前回の remap 成功後、`0x61f0` は `0x0`（logical TSI1 → physical TSI0 の
remap が残存）で、`snapshot_tsi_mapping` が拒否する状態だった。

```text
t4s-probe isdbs: lock=103, bank2=[96,128,0] (CS8/TSID 0x6080)
t4s-probe restore-tsi: before 61f0=0x0 -> restored 61f0=0x10
```

TSI mapping は復旧したが、前回の cleanup 失敗で graph の残骸は残っていた。

### 新既定での CS テスト

```sh
t4s-tuner --type CS --channel CS8 --duration 8 - > /tmp/cs8.ts 2> /tmp/cs8.log
```

- **rc=1 / 0 bytes / 0x47 なし**
- ログ:

```text
physical TSI config request ...
logical TSI1 to physical TSI0 config_ex ...
restore logical TSI1 identity and physical TSI1 ...
TSI rollback: configuration MMRs and identity mapping exactly restored
t4s-tuner: ISDB-S open: No error information (os error 1038)
```

`Source::open` の remap 設定後、DEMUX 構築中に 1038 で失敗した。
TSI rollback は実行されたが、前回の graph 残骸が残っているため、
**この状態では新 cleanup 経路を正常系で検証できない**。
BS テストは実施せず、追加のキー送信・0x6004 も行っていない。

### 次に必要

**要・電源再投入**。再起動後に graph が初期化された状態で、
CS 既定（remap）と BS を 10 秒間隔で再テストする。

## 再起動後の再テスト（2026-11-14 01:39–01:40 JST）

ユーザーによる電源再投入後（uptime 70.79 秒）、mirakc を起動せず手動で
`/media/usb` と `/mnt/t4s` を mount した。バイナリは
`552,388 bytes / md5 01cfa56831ff28cfc6c0a05ecb8e90ed`。

状態確認:

```text
t4s-probe isdbs: lock=8, bank2=[0,0,8]
t4s-probe restore-tsi: 61f0=0x10（identity、変更なし）
```

### CS 新既定（remap）

```sh
t4s-tuner --type CS --channel CS8 --duration 8 - > /tmp/cs8.ts 2> /tmp/cs8.log
```

- **rc=1**
- **13,190,268 bytes**
- 先頭 16 bytes: `47 1f ff 10 ff ff ff ff ff ff ff ff ff ff ff ff`
- cleanup ログ:

```text
mkra: BS2K remove PID retry: No error information (os error 1038)
mkra: BS2K cleanup remove PID: ignored benign error: ... 1038
mkra: BS2K disconnect needs stop/retry: ... 1038
mkra: BS2K cleanup stop retry: ... 1038
mkra: BS2K cleanup stop retry: ignored benign error: ... 1038
mkra: BS2K cleanup disconnect retry: ignored benign error: ... 1038
mkra: BS2K cleanup ring ownership: retaining graph with live writer/readers
mkra: BS2K cleanup stop: ignored benign error: ... 1038
mkra: BS2K cleanup destroy DEMUX: ignored benign error: ... 1038
TSI rollback: configuration MMRs and identity mapping exactly restored
t4s-tuner: frontend restore warning: unrecognised tuner ELF; refusing fixed-address process access
t4s-tuner: frontend restore failed: unrecognised tuner ELF; refusing fixed-address process access
```

1038 は benign として無視でき、TSI rollback も成功したが、ring が live のまま
残り、`restore_frontend()` は tuner ELF 検証で失敗した。
**rc=0 / frontend 復元成功には至っていない。**

ffprobe:

```text
container: mpegts / duration 1.94 s / bit_rate 54.36 Mbps
program 55:  mpeg2video Main 1440x1080 + AAC LC 48000 Hz 2ch（clear, PID 0x100F）
program 218: mpeg2video 0x0 + AAC 0Hz（PID 0x100D は scrambled）
program 318/349: 0x0
scrambled: 38,068 / 70,161 packets = 54.26%
```

### BS（10 秒後）

```sh
t4s-tuner --type BS --channel BS13_0 --duration 8 - > /tmp/bs.ts 2> /tmp/bs.log
```

- **rc=1 / 0 bytes / 先頭なし**
- ログ:

```text
physical TSI config request ...
logical TSI1 to physical TSI0 config_ex ...
restore logical TSI1 identity and physical TSI1 ...
TSI rollback: configuration MMRs and identity mapping exactly restored
t4s-tuner: ISDB-S open: No error information (os error 1038)
```

CS 終了時の ring が live のまま残ったため、BS の `Source::open` が
DEMUX 構築で 1038 になり、TS は取得できなかった。

### 結論

- cleanup の 1038 無視・再試行・TSI rollback は動作した。
- ただし ring が live のまま残り、frontend restore は
  `verify_layout` の OEM ELF 不一致で失敗する。
- この実機では `restore_frontend()` のプロセスメモリ経由の復元が使えない。
  別の復元経路（プロセスメモリを使わない device assign のみ等）を検討する必要がある。
- BS 取得前に graph/ring を確実に破棄する追加対応も必要。
- 次のテスト前に再度電源再投入が必要。

## 真因の特定と修正（2026-11-14 03:5x JST）— リブート不要で連続取得に成功

上記の「ring が live のまま残る」の真因を実機で特定した。**原因は DEMUX 出力
ring のあふれ**であり、cleanup 順序や timeout ではなかった。

### 調査方法

`xcdrv::call` に一時的な env-gated ioctl トレース（`T4S_TRACE_IOCTL=1`）を入れ、
各 teardown コマンドの戻り値と所要 ms を実測した。

### 観測

fresh boot 直後の **最初**の `Source::open`（CS8, 8 秒）は成功し、TS も
13.7 MB 取得できた。しかし cleanup で:

```text
ioctl 0x1f6002a (remove_pid)  err=1038 ms=4979   <- kernel 既定 timeout
ioctl 0x0009002a (disconnect) err=1038 ms=2939   <- [dev,conn,1,3000]
ioctl 0x0003002a (stop)       err=1038 ms=1999   <- [dev,1,2000]
ioctl 0x0003002a (stop)       err=1038 ms=1880
ioctl 0x0009002a (disconnect) err=1038 ms=2880
ring status writer=0x1e00ad00 readers=0  → ring は live のまま
```

- すべて **各 timeout を使い切って 1038 (XCE_TIMEOUT)**。
- 対照的に **グローバル** TSI コマンド `0x50` / `0x53` と `0x19` / `0x36` は
  同じタイミングで **すべて ok (ms=0–2)**。→ kernel は生きているが、
  **per-device (0x2a class) コマンドだけ ARC が応答しない**。
- 次の `Source::open` は `SDEV_CREATE (0x28)` が **ms=4991 で 1038**。

### 切り分け（fresh boot ごとに1条件）

| 条件 | teardown | 連続 open |
|---|---|---|
| 既定 ring 0x6000 / 8 秒取得 | 1038 で全滅、ring live | 2 回目が 1038 |
| 既定 ring / **1 秒取得 ×5** | **5/5 成功** | **5/5 成功** |
| 300 ms 遅延のみ | 1038 で全滅 | 2 回目が 1038 |
| `quiesce` のみ | 不安定（成功も失敗も） | 不安定 |
| **ring 0x18000 / 8 秒 ×3** | **3/3 成功** | **3/3 成功** |

つまり **取得が長いほど ring があふれ、DEMUX が ring 書き込みで ARC 内に
ブロックする**。ホスト側の読み出しは約 2 MB/s、フル CS は約 5–7 MB/s なので
4.6 MiB の既定 ring は数秒で埋まる。1 秒取得や 18 MiB ring では埋まらない。

### 修正

1. `rust/t4s-core/src/xcdrv.rs`: DEMUX 出力 ring を
   **0x6000 → 0x18000 units (4.6 MiB → 18 MiB)**。（他フィールドは不変）
2. `rust/t4s-core/src/bs2k.rs`: teardown を整理。**先に demodulator の
   TS 出力を止めて ring を drain** してから graph を解体する。ring を破棄
   できなかった場合は `ring ... still present after destroy` を明示的に残し、
   `cleanup_ok=false` にする（次回起動が失敗することを隠さない）。
3. `rust/t4s-tuner/src/main.rs`: `--no-restore-frontend` を追加し、
   `restore_frontend()` を best-effort 化（この個体は OEM ELF 不一致で
   必ず失敗するため、成功した取得を rc=1 にしない）。

### 検証結果（リブート不要、同一起動内で 3 連続取得）

fresh boot 後、mirakc を起動せず、同一起動のまま:

| 順 | 放送 | 秒 | rc | bytes | 先頭16 | cleanup |
|---|---:|---:|---:|---:|---|---|
| 1 | CS8 | 8 | **0** | 31,490,752 | `47 10 0f 1a ...` | 1038 なし / ring 破棄 OK |
| 2 | BS13_0 | 8 | **0** | 16,088,100 | `47 01 40 fd ...` | 1038 なし / ring 破棄 OK |
| 3 | CS8 | 8 | **0** | 32,459,704 | `47 10 0d 9c ...` | 1038 なし / ring 破棄 OK |

- 各 run の間は 10 秒。3 回とも bytes>0 / 先頭 `0x47`。
- cleanup ログ:

```text
mkra: opened ISDB-S RF 12411000 TSID 6080 / raw (CAS is a separate decode filter)
mkra: BS2K teardown quiesce 0xc0->0x40
TSI rollback: configuration MMRs and identity mapping exactly restored
```

  1038 の行が **1 行も出ていない**。ring live 警告もない。
- 3 回終了後の graph は **devices count=11**（fresh boot と同じ、残骸なし）。
  `mmr61f0=0x10`（identity）に復元。
- rc=0。`frontend restore warning: unrecognised tuner ELF` は出るが
  best-effort のため rc は 0 のまま。

### ffprobe（host）

```text
cs8a.ts: mpegts / MPEG-2 Main 1440x1080 29.97fps + AAC LC 48000Hz 2ch
         program 55（クリア）, program 218（scrambled）
         188B sync ratio 1.0000
bs13.ts: mpegts / MPEG-2 + AAC + arib_caption（decode=0 のため映像は scrambled）
         188B sync ratio 1.0000
```

CS8 program 55 は 30 フレーム実デコード成功。

### 結論

- **リブート不要で連続取得できる。** 同一起動のまま CS8→BS13_0→CS8 を続けて
  取得でき、各 teardown も 1038 なしで完了し、graph 残骸も残らない。
- 真因は cleanup 順序ではなく **DEMUX 出力 ring のサイズ不足**（ホスト読み出し
  速度 < トランスポンダ速度）。ring を 18 MiB に広げることで解消。
- `restore_frontend()` はこの個体では依然失敗する（OEM ELF 不一致）が、
  取得・cleanup の成否とは独立に扱う（best-effort）。
