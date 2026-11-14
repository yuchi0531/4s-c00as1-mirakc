# Phase 4 — 青状態で BSP4K (NHK BS4K) の TS 取得と 4K 全局検証

> **用語の訂正（2026-11-14）**: 本文書（§0–§13）が「青状態」と呼んでいるのは、
> 実際には **製品が 4K を実デコードしている視聴状態（`MIXER/Sink/buf_buf=true`,
> `discover=Ok(readers≥1)`）** を指す。本体 LED の色ではなく、
> **「緑状態（視聴状態）」** と読み替えること。§14 以降は「緑状態」表記に統一する。
> （前段の「青＝電源ON/HDMI接続」という呼称は LED 色の見た目に引きずられたもので、
> 検証で本質的なのは **デコードが live かどうか**であることが判明した。）

対象: SHARP 4S-C00AS1（ViXS XCode6, ARMv7, Linux 3.10.24）
日時: 2026-11-13 21:43–21:55 JST（青ランプ、HDMI 接続、製品 UI が 4K を視聴中）
制約: ストレージ削除なし。書込みは USB `/mnt/t4s` と Q_UIM メッセージのみ。
`t4s-remote` / `t4s-arc` は他エージェントのもので編集していない。

---

## 0. 結論

**青状態で製品 UI が BSP4K を視聴している間、`t4s-tuner --type BS4K` の REMUX 経路で
4K TS がそのまま取得できる。** BS4K 5 局 + BSP4K の計 6 局すべて、
HEVC Main10 3840x2160 + AAC-LATM 48 kHz 2ch で取得に成功した。

- 製品側で 4K 選局 → ES ring（VIDEO 20 MiB / AUDIO 2 MiB）が live になる。
- `remux4k::discover` が `Ok`（`video_readers:1 audio_readers:2`）を返す。
- `t4s-tuner --type BS4K --channel <TLV_stream_id> --duration N -` で MPEG-TS 出力。
- BS4K は製品の ACAS で復号済みなので `t4s-cas` 不要（クリア TS）。
- USB へ書いてホストへ転送し、ffprobe で確認。

---

## 1. 前提：青状態の AV スレッドと SI

`t4s-remote` の `/api/status`（HTTP）で確認:

```
"threads": {"MIXER": true, "Sink": true, "buf_buf": true, "HevcFactory": true}
"si": {"ok": true, "tlv_stream_id": 45328, "original_network_id": 11,
       "service_id": 101, "service_id_hex": "0x0065"}
```

- BSP4K 視聴中は `MIXER` / `Sink` / `buf_buf` が生成される。
- `/tmp/adsb/AdsbSiMpt.dat` + `AdsbSiMhSdtActual.dat` から SI が読める。
- `t4s-probe`（引数なし）:
  ```
  si=Some(LiveService { tlv_stream_id: 45330, original_network_id: 11, service_id: 181 })
  discover=Ok(LiveEs { mmt_demux: 8, pipeline: 685140224, video_es: 503504128,
                       audio_es: 686859008, video_readers: 1, audio_readers: 2 })
  ```

---

## 2. 選局（製品 UI）のキー列

### 2.1 正しい UIM メッセージ ID

- 受信ディスパッチは `0x70dde2` で `movw r2,#0x2044; movt r2,#0x1` = **0x12044**。
- 送信側 FPM (`0x513662`) も `qid 0x7F, msg 0x12044, len 8, body {u32 key, u32 type}`。
- 別系統は **0x1115C**（`0x708d5e`）。どちらもキーとして受理される。
- `msg 0x2044` はディスパッチに一致せず捨てられる（`prev key` が更新されない）。

### 2.2 実測した選局キー列

- Q_UIM (qid 0x7F) へ `{keyCode, type}` を送る。
- `type=0` 押下。同じキーを連続で送るときは `type=2`（`key=~key`）を挟む。
- **CH UP `0x00C` を 1 回押すごとに次の 4K 局へ進む。**

```
key 0x00C, type 0 → 2 秒待ち → /api/status の SI を確認
（同じキーを続ける場合は type 2 を 1 回挟む）
```

製品の 4K チャンネル順（実測）:

| 順 | サービス | TLV_stream_id | service_id |
|---:|---|---:|---:|
| 1 | BSP4K (NHK BS4K) | 45328 | 101 |
| 2 | BS日テレ4K | 45170 | 141 |
| 3 | BS朝日4K | 45168 | 151 |
| 4 | BS-TBS4K | 45329 | 161 |
| 5 | BSテレ東4K | 45169 | 171 |
| 6 | BSフジ4K | 45330 | 181 |

- 選局ごとに `MIXER/Sink/buf_buf` が一瞬 false → true に戻る（再初期化）。
- SI の `tlv_stream_id` が切り替わったことを確認してから取得する。

---

## 3. 取得コマンド

```sh
/mnt/t4s/opt/t4s/bin/t4s-tuner --type BS4K --channel <TLV_stream_id> \
    --duration 10 - > /mnt/t4s/bs<sid>.ts 2> /mnt/t4s/bs<sid>.log
```

- `--channel` は **TLV_stream_id**（例 45328）。
- BS4K は復号済みなので `t4s-cas` は通さない。
- 取得中は製品が同じ 4K を視聴している必要がある（ES ring を製品が作るため）。

---

## 4. ffprobe 結果（全 6 局）

ホスト側で `ffprobe -analyzeduration 30M -probesize 50M`。

| 局 | service | bytes | ffprobe duration | bit_rate | video | audio |
|---|---:|---:|---:|---:|---|---|
| BSP4K | 101 | 31,725,752 | 10.527 s | 24.11 Mbps | hevc Main 10 3840x2160 | aac_latm LC 48 kHz 2ch |
| BS日テレ4K | 141 | 30,701,340 | 8.742 s | 28.09 Mbps | hevc Main 10 3840x2160 | aac_latm LC 48 kHz 2ch |
| BS朝日4K | 151 | 27,623,592 | 8.490 s | 26.03 Mbps | hevc Main 10 3840x2160 | aac_latm LC 48 kHz 2ch |
| BS-TBS4K | 161 | 30,581,960 | 8.541 s | 28.64 Mbps | hevc Main 10 3840x2160 | aac_latm LC 48 kHz 2ch |
| BSテレ東4K | 171 | 29,367,104 | 8.141 s | 28.86 Mbps | hevc Main 10 3840x2160 | aac_latm LC 48 kHz 2ch |
| BSフジ4K | 181 | 28,578,632 | 8.459 s | 27.03 Mbps | hevc Main 10 3840x2160 | aac_latm LC 48 kHz 2ch |

### 4.1 色情報（全 6 局）

| 局 | color_space | color_transfer | color_primaries |
|---|---|---|---|
| BSP4K (101) | bt2020nc | bt2020-10 | bt2020 |
| BS日テレ4K (141) | bt2020nc | **arib-std-b67 (HLG)** | bt2020 |
| BS朝日4K (151) | bt2020nc | arib-std-b67 | bt2020 |
| BS-TBS4K (161) | bt2020nc | arib-std-b67 | bt2020 |
| BSテレ東4K (171) | bt2020nc | arib-std-b67 | bt2020 |
| BSフジ4K (181) | bt2020nc | arib-std-b67 | bt2020 |

- 141 は再取得（22:04、`--duration 12`）でも HLG を確認:
  **37,779,352 bytes / 10.377 s / 29.13 Mbps / hevc Main10 3840x2160 /
  arib-std-b67 / bt2020nc / aac_latm LC 48 kHz 2ch**。
- 最初の BSP4K 取得時だけ `bt2020-10` だったのは、その番組が HLG では
  なかった（または SPS の色記述が BT.2020 10-bit だった）ため。
- 実尺が `--duration` より短いのは、ring 検出から読み出し開始までの遅れと、
  停止処理によるもの。
- 取得 TS は先頭が GOP 途中から始まることがあり、ffmpeg デコードでは
  最初の IDR まで参照エラーが出ることがある（TS 自体は有効）。

---

## 5. 注意・制約（実測）

- 製品 UI が 4K を視聴していないと ES ring が無く `discover` は Err。
  `tlvcap3`（ADBS FED 直接 + 自前 TLV）は青状態でも 0 バイトだった（別経路の限界）。
- 青状態で 4K 視聴中は load average が 25–37 まで上がり、telnet が数十秒遅延することがある。
  HTTP `/api/status` は安定して応答する。
- `t4s-remote` の `/api/key` は `msg 0x2044` を送るため、**画面は動かない**
  （`last_key` だけ更新される）。キー注入は `t4s-probe uimkey ... 0x1115c` を使う。
- 4K チャンネルサーチ（製品 UI）は BS4K/110CS4K を 0 件と報告した。
  ただし製品は 4K 局を選局・視聴でき、REMUX 取得もできる。
  サーチ側の判定と実視聴の不整合は今後の課題。
- 製品 UI の初期設定を抜けるキー列は `docs/analysis/phase4-device-blue.md` §3 に記録。

---

## 6. 次に必要なこと

1. mirakc の `config.yml` に BS4K チャンネル（`t4s-tuner --type BS4K --channel <stream_id>`）
   を登録し、API 40772 経由の録画/ストリームを検証する。
2. 製品 UI を 4K 視聴状態に保つ運用手順（起動時に 4K を選局するキー列）を
   `ondevice-up.sh` などに組み込めるか検討する。
3. 4K チャンネルサーチが 0 件になる理由（サーチ経路と REMUX 経路の差）を
   adsb/abs 側で切り分ける。
4. `t4s-remote` のキー msgId を 0x12044 / 0x1115C に修正すれば Web UI からも選局できる
   （本 Phase では他エージェント管轄のため未変更）。

---

## 7. 追記（22:11–22:50）: mirakc API と復旧

- `mirakc` の `/api/channels/BS4K/45170/stream?decode=1` は **HTTP 200** で
  4K TS を配信（46,137,344 bytes / 12.8 s / 28.8 Mbps ほか）。
  ffprobe は HEVC Main10 4K HLG + AAC-LATM 48 kHz 2ch。
  `decode=0` も成功（16,830,136 bytes）。詳細は
  `docs/analysis/phase4-mirakc-api.md`。
- 製品 UI を 4K へ戻す際、UI マクロ `4K→1→0→1→ENTER` は
  「選局・再生に失敗しました」となったが、
  **`t4s-probe send 141 0 0`（0x6004 直接）で 141 の 4K 視聴状態に復帰できた**
  （`si=Some((141,45170))`, `discover=Ok((1,2,8))`, MIXER/Sink true）。
- 再起動後の DirectFB はサーフェス配置が変わる。`0x3C884000` は透明オーバーレイで、
  実際の UI は `0x3A390000`（番組表）だった。OSD を見る場合は再探索が必要。
- 22:50 頃、`decode=0` 再試行直後に実機が **ping 不通 / no route to host** となり、
  遠隔からの復旧不能になった（電源再投入が必要）。
  `/api/channels/BS4K/45328/stream` は未検証のまま。
- 2026-11-14 00:22 JST、4K 局の連続選局で製品 UI が「選局・再生に失敗しました」の
  失敗状態（MIXER/Sink=false）になり復帰不能となったため、**ユーザー指示で検証を中止**。
  45328 以降の API 検証は「未検証・電源断で中止」。詳細は
  `docs/analysis/phase4-mirakc-api.md` §6。

---

## 8. 追記（2026-11-14 04:52–05:04 JST）: HDMI なしヘッドレスで 4K 到達性の検証

HDMI を物理的に抜いた状態で、キー注入と直接選局により 4K の
MIXER/Sink/ES ring が live になるかを 1 操作ずつ検証した。
mirakc は稼働中（pid 884、`/api/tuners isFree=true`）。

### 8.0 初期状態

```
threads: {MIXER:false, Sink:false, buf_buf:false, HevcFactory:true}
si:      {ok:false, reason:"no usable /tmp/adsb MH-SDT + MPT"}
regs:    {lock:72, bank2_selection:[64,208,0]}
q_uim:   {qnum:0, lspid:279, lrpid:470}
```

`/tmp/adsb/AdsbSiMpt.dat` は非ゼロ（8193 B）だが
`AdsbSiMhSdtt.dat` は全ゼロ → `si.ok=false` の理由。

### 8.1 手順1（直接選局 `send 141 0 0`、1 回だけ）

`t4s-probe send 141 0 0` を 1 回送信。8 秒後の状態:

```
threads: {MIXER:false, Sink:false, buf_buf:false, HevcFactory:true}
si:      {ok:false, ...}
```

一方 `t4s-probe discover` は送信直後〜数秒間だけ **Ok を返した**:

```
discover=Ok(LiveEs { mmt_demux: 8, pipeline: 685140224,
                     video_es: 503358208, audio_es: 686166272,
                     video_readers: 0, audio_readers: 0 })
```

- MMT_DEMUX (type 0x25, handle 8) と ES リング
  （VIDEO 20 MiB `0x1e00a300` / AUDIO 2 MiB `0x28e61100`）は**生成された**。
- しかし **video_readers=0 / audio_readers=0**。製品のデコーダ
  （MIXER/Sink）が接続していない = 4K 視聴状態ではない。
- しばらく後に再確認すると `discover=Err("no live 4K ES pair ...")` に戻った。

### 8.2 手順2（4K 取得）

手順1で MIXER/Sink=true にならなかったため、`t4s-tuner --type BS4K` は
実行していない（ES リングにリーダーが付かないため意味がない）。

### 8.3 手順3（キー注入、1 回ずつ・各後に status 確認）

| 手順 | キー | 送信 | 8 秒後の MIXER/Sink | discover |
|---|---|---|---|---|
| 3a | `0x13B` (RC_4K) | type0→type2 | false / false | `Err` |
| 3b | `0x00C` (CH UP) | type0→type2 | false / false | `Err` |
| 3c | `0x001 0x00A 0x001`→`0x015` (1,0,1,ENTER) | 各 type0→type2 | false / false | `Err` |

- 3c の後 `q_uim.lrpid` が **470 → 279 に変化**（`abs` がメッセージを消費）。
  つまりキーはキューから受理されている。しかし `si` は変わらず
  （`ok=false` のまま）、MIXER/Sink も復帰しない。
- `bank2_selection` は `[64,208,0]`（lock=72）のまま。選局は起きていない。

### 8.4 OSD（失敗ダイアログの有無）

`0x3A390000`（この起動での UI 面）を採取:

- 全画素が **カラーバー状のノイズパターン**（static/no-signal 相当）。
  文字・ボタン・ダイアログは読み取れない。
- 3a〜3c 後も OSD は **バイト単位で同一**
  （240x135 縮小 md5 が前後で一致）。→ キーで UI は変化していない。
- 採取した PPM（1920x1080 step=1 / step=2）はホストの `.tmp/` に保管
  （リポジトリには画像を追跡しない）。
- `0x3C884000`（旧 OSD 面）と `0x3F814000`（背景面）は今回すべて 0 画素
  （この起動では scanout でない）。UI 面アドレスは起動ごとに変わる。

### 8.5 abs スレッド

`abs`(pid 279) に **`T_HDMI` / `T_HDMID` / `T_HDMID2` / `TID_VDEC` /
`TID_AUD*` は存在する**が、**`MIXER` / `Sink` / `buf_buf` は存在しない**。
これは docs/4K-NO-HDMI.md の「HDMI シンク無しではデコードパイプラインを
開始しない」と一致する。

### 8.6 判定

**HDMI なしヘッドレスでは 4K 視聴状態（MIXER/Sink=true）に到達できなかった。**

- `send 141 0 0` は MMT_DEMUX と ES リングを作るが、製品デコーダは attach せず
  `video_readers=0` のまま。リングはリーダーが付かないので再利用できない。
- UI キー（RC_4K / CH UP / 1,0,1,ENTER）はキューには受理される
  （`lrpid` 変化）が、`si` も MIXER/Sink も OSD も変化しない。
  失敗ダイアログも出ない（そもそも UI がノーシグナル画面のまま）。
- 4K TS 取得は未達。取得できるのは製品が実際に 4K をデコードしている
  間だけで（§0/§2 の実測）、その起動には HDMI シンク（TV または
  EDID エミュレータ）が必要（docs/4K-NO-HDMI.md の選択肢 1/2）。

追加のキー送信・`send 141 0 0` の再送・電源操作は行っていない。
検証後の実機は `devices count=11` / `mmr61f0=0x10` / mirakc 稼働を維持している。

---

## 9. 追記（2026-11-14 08:0x JST）: HDMI 未接続で 4K live + 取得成功（重要な新事実）

本セッション開始時、実機は **前タスク中に自然再起動**しており（dmesg は fresh boot、
私の mount 時のみ `FAT-fs: Volume was not properly unmounted`）、前提と異なる状態だった:

- mirakc / t4s-remote は **停止**（pidfile 884 は stale、port 40772/8484 未 listen）。
- 製品は **4K(141/45170) を実デコード中**:
  `si=Some(141/45170)`, `discover=Ok(LiveEs{mmt_demux:8, video_readers:1, audio_readers:2})`,
  `MIXER`/`Sink`/`buf_buf`/`HevcFactory` スレッドあり、video ES ring の wpos が進行。

つまり **HDMI 未接続のまま 4K 視聴状態に到達していた**。

### 9.1 制約遵守

`tlvcap`/`tlvcap2`/`tlvcap3` は FED 再選局で live 4K を破壊し、`tlvcap2` は
さらに 0x6004 を送る（禁止）ため **実行していない**。キー送信・0x6004・選局・
reboot・電源操作・mirakc 起動もしない。行ったのは read-only 観測と、製品の
選択を変えない `t4s-tuner --type BS4K` 取得のみ。

`t4s-tuner --type BS4K --channel 45170` は `tune_4k` の先頭で
「live SI が要求 service と一致し discover が Ok」なら `send_change`（0x6004）を
**送らずに** REMUX tap へ進む（`rust/t4s-tuner/src/main.rs:204`）。今回は live が
141/45170 だったため 0x6004 は送信されていない（ログに `confirmed`/`send_change` 無し）。

### 9.2 取得結果

| 回 | bytes | 先頭16 | format | duration | bit_rate | read |
|---:|---:|---|---|---:|---:|---|
| 1 | 38,329,628 | `47 40 00 10 00 00 b0 0d b0 72 c1 00 00 00 8d e1` | mpegts | 10.71 s | 28.63 Mbps | rc=0 |
| 2 | 23,471,424 | `47 40 00 10 00 00 b0 0d b0 72 c1 00 00 00 8d e1` | mpegts | 6.51 s | 28.84 Mbps | rc=0 |

- 188 バイト TS、sync ratio **1.0000**。PAT TSID **45170**、program 141、PMT PID 256。
- ストリーム: video PID `0x1011` (type 0x24=HEVC), audio PID `0x1100` (type 0x11=AAC)。
- ffprobe:
  - video: **hevc / Main 10 / 3840x2160 / yuv420p10le / color_transfer=arib-std-b67 (HLG) /
    color_primaries=bt2020**
  - audio: **aac_latm / 48000 Hz / 2ch**
- ログ: `BS4K 45170 -> service 141, TLV 0xb072`、
  `remux4k: MMT_DEMUX=0x8 ... video_es=0x1e00bd00 (readers 1) audio_es=0x28e63300 (readers 2)`。
  read 中に 160-297 ms の gap を数十回 "recovered"（tap が live ES ring を共有するため）。
  `0 resyncs`。

### 9.3 取得前後の live 状態（壊していないことの確認）

| | 取得前 | 1回目後 | 2回目後 |
|---|---|---|---|
| discover | `Ok(readers 1/2)` | `Ok(readers 1/2)` | `Ok(readers 1/2)` |
| si | 141/45170 | 141/45170 | 141/45170 |
| graph devices | 22 | 22 | 22 |
| mmr61f0 | 0x10 | 0x10 | 0x10 |

同一 video_es/audio_es handle のまま。取得は製品の選択・デコードを止めていない。

### 9.4 HDMI シンクの有無（read-only 判定）

kernel 3.10 で **`/sys/class/drm` は存在しない**（CONFIG_DRM 無し）。
`dmesg` にケーブル接続/HPD の明示イベントは無い。read-only で観測できた材料:

- `/sys/class/drm/*/status` … **不可**（drm class 無し）。
- `dmesg | grep -iE 'hdmi|hotplug|edid|hpd|cec'` … `xc_aud_dev_register hdmi_delay...` と
  `HDMI UFLOW` ×4 のみ。HPD イベント行は無い。
- abs スレッド: **`T_HDMI` / `T_HDMID` / `T_HDMID2` / `T_OUTD` / `T_OUTD_DRV` / `T_AVM`
  が存在**（HDMI 制御スレッドは起動している）。
- `/proc/interrupts`: **`xcfb interrupt` (IRQ33) が ~120/s** 進行、**`HDMI Audio` (IRQ45)**
  も進行。ディスプレイ/Audio の scanout が動いている。
- 拡張 GPIO: `gpio2=0(in) gpio5=1(in) gpio6=1(out) gpio9=1(out)`。
- i2c-1 (XCODE6-0 I2C-1) の **DDC/EDID アドレス 0x50 が応答**: `i2cdetect` で 0x50 検出。
  ただし `i2cget`/`i2cdump` の内容は **不安定**で、標準 EDID ヘッダ
  `00 ff ff ff ff ff ff 00` は **40 回読んでも得られなかった**。読めるのは 128 バイトの
  内部レジスタ様パターン（0x80-0xF0 が 0x00-0x70 を反復）で、外部 EEPROM の
  EDID とは読めない。

**判定**: 「HDMI 未接続」と報告されているが、read-only では **外部 HDMI シンクの
存在を確定できない**（drm 無し・dmesg に HPD 無し・EDID ヘッダが読めない）。
一方で HDMI 制御系スレッドと xcfb/HDMI Audio 割り込みは動作しており、製品は
4K を実デコードしている。**EDID エミュレータ/ダミープラグ、または HDMI ケーブルが
実際には接続されている**可能性が高く、`docs/4K-NO-HDMI.md` の「HDMI シンクが
無ければデコードを開始しない」は、少なくとも今回の起動状態とは整合しない。
真偽は物理接続の確認（現地目視）が必要。

### 9.5 結論

- HDMI 未接続とされる状態で、**4K(141/45170) が live のまま、`t4s-tuner --type BS4K`
  で HEVC Main10 4K HLG + AAC-LATM の TS を 2 回連続取得できた**（0x6004 不使用、
  live 状態は無破壊）。
- ただし HDMI シンクの有無自体は read-only では確定できず、物理接続の確認が必要。
  「HDMI なしでも 4K が取れる」と断定はせず、この観測事実のみ記録する。

---

## 10. 追記（2026-11-14 09:4x JST）: HDMI 無し再起動で 4K は自動復帰しない（決定的テスト）

§9 の「HDMI 無しで 4K live を取得できた」観測を説明する仮説
「**HDMI が要るのは初期設定ウィザードを抜けるためだけ。設定済み＋前回視聴状態が
保存されていれば、HDMI 無しでも再起動後に 4K 視聴へ自動復帰し ES ring が出る**」
を、実機のソフトリブートで検証した。ユーザーは「HDMI にはケーブルもダミープラグも
刺さっていない」と明言。キー送信は行っていない（手順中の `send 141 0 0` 1 回のみ）。

### 10.1 時系列

| 時刻 | 事象 | si | discover readers | lock | devices | OSD |
|---|---|---|---:|---|---:|---|
| 09:38 | テイクオフ前 | 141/45170 | Ok(1,2) | 8 | 22 | video plane |
| 09:38:24 | `sync; (sleep 1; reboot) &` 発行 | — | — | — | — | — |
| +37s | ping DOWN | — | — | — | — | — |
| +85s | ping UP（復帰 ~90s） | — | — | — | — | — |
| +145-166s | 復帰直後（手動 loop mount 後） | **None** | **Err** | 8 | **11** | color bars |
| +166-245s | 90s ポーリング（5s 毎, 12 回） | **None** | **Err** | 8 | 11 | color bars |
| +~300s | `t4s-probe send 141 0 0` を 1 回送信 | 141/45170 | Ok(1,2) → 12s 後に Err | 8 | 22 | color bars |
| +330-832s | 90s+ 追観測 | 141/45170 | **Err** | 8 | 22 | color bars |

補足: 再起動後 `/mnt/t4s`（ext4 in t4s.img）は自動マウントされない（`/media/sda1` が
ro のみ）。mirakc を起動せず手動で `mount -o remount,rw /media/sda1` →
`losetup /dev/loop0 /media/sda1/t4s.img` → `mount -t ext4`。t4s-probe/t4s-tuner の
md5 は前回一致（`a84a4bc2…` / `43c416c5…`）。復帰後 0-90s は probe 不在のため
サンプル無し。

### 10.2 判定

- **HDMI 無し再起動では 4K 視聴状態へ自動復帰しない。** 復帰後 `si=None`・
  `discover=Err`・`devices count=11`（青状態）で、90 秒以上ポーリングしても変わらない。
  → 仮説「設定済みなら HDMI 無しでも自動復帰する」は **棄却**。
- 復帰後の OSD（`0x3A390000`）は **カラーバー / 無信号テストパターン**（distinct 色
  12663、動画ではない）。`0x3C884000` は全黒。初期設定ウィザードらしき文字画面は
  確認できなかった（日本語 OCR 不可のため色分布のみ）。
- **`send 141 0 0`（0x6004 を 1 回）を送ると** `si=Some(141/45170)` が入り ADSB
  パイプラインが生成され `devices count` が 11→22 に増えるが、`discover` は
  **約 12 秒だけ `Ok(1,2)` になり、直後に `Err` へ戻る**（readers が定着しない）。
  つまり選局（SI）は受け付けるが、**HDMI シンクが無いとデコーダ（ES ring の
  reader）が付かない**。「SI は記憶されるがデコードは始まらない」。
- 復帰後の HDMI 指標:
  - abs スレッド `T_HDMI`/`T_HDMID`/`T_HDMID2`/`T_OUTD`/`T_OUTD_DRV` は **存在**（制御
    スレッドは常時起動）。
  - `xcfb interrupt`(IRQ33) は ~120/s で進行（OSD の scanout）。
  - `HDMI Audio`(IRQ45) は **1334 のまま 5 秒間で増加しない**（テイクオフ前は ~25/s で
    進行していた）。
  - i2c-1 DDC 0x50 は応答するが EDID ヘッダは依然読めない（内部レジスタ様、前回同様）。
  - dmesg に明示的な HPD イベントは無く、`HDMI UFLOW` が起動直後に数回出るのみ。

### 10.3 結論

- docs/4K-NO-HDMI.md の「HDMI シンク無しでは 4K デコードパイプラインが始まらない」は
  再起動テストでも **支持された**。HDMI 無し再起動→青状態→SI のみ復元で、ES ring は出ない。
- §9 の「HDMI 未接続で 4K live」は「設定保存による自動復帰」では説明できない。
  あの時は **HDMI シンク（ケーブルまたは EDID エミュレータ/ダミープラグ）が実際に
  接続されていた**（= 一度デコードが始まった後、外れても視聴状態の間はデコードが
  継続する、という 4K-NO-HDMI.md の選択肢 2 の挙動）と考えるのが最も整合的。
- つまり **初期設定ウィザードだけでなく、4K デコード開始そのものに HDMI シンクが
  必要**。完全な「HDMI 不要」運用は本テストでは成立しなかった。

### 10.4 状態保全

テスト後も `devices count=22`、`lock=8`、`bank2=[0,0,8]`、`mmr61f0=0x10`、
`/mnt/t4s`（loop0, rw）マウント維持。mirakc/t4s-remote は起動していない。
loadavg は ~27（再起動直後の一時的上昇で収束傾向）。

---

## 11. 追記（2026-11-14 09:5x–11:0x JST）: 12 秒窓での取得試行（**実施できず**）

§10 の観測（`send 141 0 0` 直後 ~12 秒だけ `discover=Ok(1,2)`）を受け、
その窓で `t4s-tuner --type BS4K` を回して 4K TS が取れるかを検証する予定だった。

しかし **テスト開始時に実機がネットワークから消えた**ため、1 サイクルも実施できなかった。

- 最初の疎通喪失検出: 2026-11-14 01:34:55Z（= 10:34 JST 前後）。それ以前の直前タスクでは
  正常に `192.168.1.154:2323` へ接続できていた。
- 以降 02:03:07Z まで **約 28 分間**、`ping 192.168.1.154` は 100% loss。
- `2323`(telnet) / `8484`(t4s-remote) / `40772`(mirakc) いずれも closed。
- 192.168.1.0/24 と 192.168.100.0/24 をスイープしても、その MAC/IP は出現せず
  （192.168.1.154 の ARP エントリも無し）。IP が変わったわけではない。
- ホスト側の経路は正常（`192.168.1.154 via 192.168.100.3`、GW 疎通あり）。他の
  192.168.1.x ホストは応答するため、ホスト側ネットワークの問題ではない。
- ホストに USB シリアルも無く、本タスクの禁止事項（電源操作・reboot）により
  遠隔復旧は不可。**物理的な電源再投入が必要**。

`send 141 0 0` や `t4s-tuner` は 1 回も実行しておらず、`.tmp/w4k-*` も生成していない
（実機状態は未変更）。実機が復旧したら、§10 の「12 秒窓」仮説を改めて検証する。

---

## 12. 追記（2026-11-14 JST）: ソフト偽装の調査 — XCFB ioctl とシンク present ゲート

HDMI シンク無しで 4K デコードを起こすための**最小のソフト手段**を、ホスト上の
逆アセンブル（`.tmp/oem/abs`, `.tmp/abs.dis`, `.tmp/abs-refs.pkl`, `.tmp/oem/xcdrv.ko`）
だけで特定する。実機はオフラインのため一切触っていない。**本節は read-only 解析のみ**。

### 12.1 最重要の確定事項: ioctl 要求番号は固定 `0x61000`

`abs` の `/dev/xcfb_disp` 利用箇所（`vOutDriverMain`, `VID_PlayAnimationTask`,
`Debug_Dfb_Debug`, `Disc_Access_IF_Thread` 内の HDMI_SYS 群、`HDMI_DrvEvent`,
`HDMI_Hdcp`）を全数走査した結果、**`_IOR/_IOW` 合成ではなく、要求番号は常に
リテラル `mov.w r1, #0x61000`**（188 ioctl 中、XCFB 系はすべてこれ）。
コマンド種別は要求番号ではなく、**引数 struct の先頭ワード**に入る。

逆アセンブルから復元した引数 struct（16 バイト）:

```c
struct xcfb_disp_arg {
    u32 cmd;    // +0x00  XCFB_CMD_DISP_* （下の表）
    u32 token;  // +0x04  HDMI_LOCK(0x500) が返すセッショントークン
    u32 arg1;   // +0x08  コマンド依存（in/out）
    u32 arg2;   // +0x0c  コマンド依存（in/out）
};
ioctl(fd, 0x61000, &arg);
```

- 根拠: `abs:0x22dcc8`（`HDMI_SYS_XcfbDispCommand` の実体）。冒頭で
  `cmd=0x500` を `[r7,#0x10]` に置いて `ioctl(fd,0x61000,r7+#0x10)` を発行し、
  戻り値の `[r7,#0x14]`（= token）をグローバル `0x1449a38` に保存（`0x22dd2a`-`0x22dd32`）。
  次に `[r7,#0x10]=cmd`, `[r7,#0x14]=token`, `[r7,#0x18]=*arg1ptr`,
  `[r7,#0x1c]=*arg2ptr` を組み、再度 `ioctl(fd,0x61000,r7+#0x10)`（`0x22dd34`-`0x22dd5e`）。
  戻り後 `[r7,#0x18]`→`*arg1ptr`, `[r7,#0x1c]`→`*arg2ptr` を書き戻す（in/out）。
  末尾で `cmd=0x501`（UNLOCK）を送る。
- `HDMI_SYS_TMDS_Enable`（`abs:0x22eb46`）は同じレイアウトを直接組み立てるため、
  READ_REG/WRITE_REG の `arg1`/`arg2` の意味がこの経路で読める（§12.3）。
- **LOCK/UNLOCK (0x500/0x501) はディスプレイ排他ミューテックス**であり、
  HDMI レジスタや HPD/EDID を書き換えるコマンドではない。製品自身も
  EDID 読みの前後で必ずこのペアを発行している（`0x2365a0` の前段）。

### 12.2 XCFB ioctl 番号表（cmd 名 → 引数 struct の cmd 値）

`request` 列は全行 `0x61000`。`arg1`/`arg2` は struct の +0x08/+0x0c。
「出所」はこの値を組んでいる `abs` の関数（すべて `Disc_Access_IF_Thread` 配下の
HDMI_SYS 群の内部名）。

| cmd 値 | 名前 | arg1 | arg2 | 出所 / 根拠 |
|---|---|---|---|---|
| 0x500 | `XCFB_CMD_DISP_HDMI_LOCK` | — | — | 全 HDMI_SYS 関数（`0x22dcf2` 他） |
| 0x501 | `XCFB_CMD_DISP_HDMI_UNLOCK` | — | — | 同上 |
| 0x516 | `..._HDMI_SET_SPD` | SPD 構造体 | — | `HDMI_SYS_SendSpdInfo` (`0x22d6a0`) |
| 0x51a | `..._HDMI_HDCP_SET_SRM_LIST` | list ptr | — | `HDMI_SYS_HDCP_SetSRM_List` (`0x22e1b6`) |
| 0x51c | `..._HDMI_HDCP_SET_CFG` | cfg | — | `HDCP_SetStreamContentType`/`SetDelay` (`0x22e424`,`0x22e69e`) |
| 0x520 | `..._HDMI_READ_REG` | reg addr (in) | 値 (out) | `TMDS_Enable` (`0x22ece6`,`0x22ed42`) |
| 0x521 | `..._HDMI_WRITE_REG` | reg addr | 値 | `TMDS_Enable` (`0x22ebda`,`0x22ec68`,…) |
| 0x523 | （plug state） | HPD (out) | — | `Disc_Access_IF_Thread` plug 判定 (`0x22d8f0`) |
| 0x524 | （plug state 拡張） | RSEN (out) | — | 同 (`0x22d90e`)。`combined=0x523 | 0x524<<1` |
| 0x52c | `..._HDMI_SET_METADATA` | meta ptr | len | `HDMI_SYS_HDR_SetMetadata` (`0x22dff2`) |
| 0x50b | `..._HDMI_EDID_GET_BLOCK_NUM` | block 数 (out) | — | `HDMI_Hdcp` EDID_Read_ROM (`0x2365b8`) |
| 0x50c | `..._HDMI_EDID_READ_BLOCK` | block index | buf ptr | 同 (`0x23660c`) |
| 0x534 | （TMDS delay powerup） | — | — | `HDMI_SYS_TMDS_SetDelay` (`0x22e9f4`) |
| 0x535 | （TMDS delay setup） | — | — | 同 (`0x22e8fa`) |

下位（非 HDMI）コマンドも同じ request 0x61000 を使う（表外の補足）:

| cmd 値 | 名前 | 出所 |
|---|---|---|
| 0x200 / 0x201 | `..._SCREEN_LOCK` / `..._SCREEN_UNLOCK` | `VID_SetScreenDitherMode` (`1ecc2c`,`1ecd24`) |
| 0x210 相当 | `..._SCREEN_SET_DITHER_MODE` | 同 (`1eccf0`、mode は [r7,#0x10]) |
| 0x300 / 0x301 | `..._LAYER_LOCK` / `..._LAYER_UNLOCK` | `vOutDriverMain`/`Debug_Dfb_Debug` 他 |
| 0x30d | `..._LAYER_SET_OPACITY` | 同 |
| 0x31d / 0x31e | `..._LAYER_SET_FILTER_PARAM` / `..._GET_FILTER_PARAM` | `VID_PlayAnimationTask`/`Debug_Dfb_Debug` |
| 0x700 / 0x701 / 0x702 | `..._BLANK_GEN_LOCK` / `...,SET_CONFIG` / `...,UNLOCK` | `VID_PlayAnimationTask` (`1f143a`…) |

**未確定**: 0x523/0x524 の bit 割り当ては `abs` のログ文字列
（`Cable State <HPD|RSEN>` と `and #3` 分岐）からの推定で、値の並び
（`0x523` が HPD か RSEN か）は実機で 1 回読むまで確定しない。`combined` の
計算式 `a523 | (a524 << 1)` 自体は逆アセンブルで確定（`0x22d924`）。

### 12.3 READ_REG / WRITE_REG の引数と意味

`HDMI_SYS_TMDS_Enable`（`abs:0x22eb46`〜`0x22f034`）が唯一の読み書き実装:

- `READ_REG`: `{cmd=0x520, token, addr, 0}` を ioctl → 値は `arg2`。
  `abs:0x22ed2c` に `Read reg (addr, data) = (0x%04X, 0x%04X)`。
- `WRITE_REG`: `{cmd=0x521, token, addr, value}`。
- この関数は TMDS 有効化のため `0x3000`/`0x3004`/`0x3020` 等の XCode6 HDMI
  レジスタを読み書きする。**どのレジスタが HPD/EDID を操作できるかは未確定**
  （xcfb の .ko 本体が `.tmp/oem/` に無く、カーネル ioctl ハンドラを読めない）。
  ホスト上に `xcfb_disp` の登録元モジュールは見つからなかった
  （`find .tmp -iname '*xcfb*'` は 0 件、`xcdrv.ko` の strings に `xcfb` は無い）。
  **したがって WRITE_REG でシンク状態を作り込む道は、いまは根拠が無い。**

### 12.4 シンク present ゲートの結論

- HDMI 制御は `abs` の `HDMI_Main`(0x22f108) / `HDMI_Event`(0x232f60) /
  `HDMI_DrvEvent`(0x2348a4) / `HDMI_Hdcp`(0x234989) の 4 関数群が担い、
  **HPD/EDID の読みは上記 XCFB ioctl 経由で xcfb ドライバに委譲**している。
- シンク状態のグローバルは `fSendHpdRise`(0x1449a80), `tHdmiCableStat`(0x1449a88),
  `eHdmiAudioConn`(0x1e6d64c), `gHdmiDelay`(0x1253420)。このうち `.text` から
  直接参照されるのは `tHdmiCableStat`（`0x22d460` で `0` クリア）のみで、
  他は data.rel.ro 経由の間接参照だった。
- abs 側に **「シンク present を強制する」分岐は存在しない**。むしろ
  `eAvmTesttoolSetESDecRangeForce`（`0x4895ac`）・`eAvmTesttoolSetHdmiOutChange`
  （`0x4895e8`）・`eAvmTesttoolSetHdmiSubCapable`（`0x489684`）という
  **テストツール用のフック**が存在し、`cget_integer` で値を受け取って AVM の
  強制フラグ（`0x10379e4` 周辺）を書き換える。ただしこれらは
  **AVM 内部のデコード対象レンジ/HDMI 出力切替のフラグ**であり、
  「HDMI シンクが無いのに present にする」ものではない（`SetHdmiOutChange` は
  `0/1` を `0x489898` のテストフックで切替）。デコード開始の可否そのものを
  単一フラグで立てる経路は、abs の `vAvmMain`(0x4581c4) 側には見当たらない。
- `adsb` 側には `vAdsbAvmMain`(0x30be8) があり、`FDC_EVENT_AVM_START_VIDEO_DECODE`
  を処理する。`0x312cc`/`0x31344` は FDC メッセージ（`fdc_sysSendMessage`）を
  送るだけで、**HDMI シンクの有無を判定する分岐は adsb 側にも無い**。
- **結論（推定を含む）**: ゲートは「HDMI 接続 → HDMI_Event/DbEdid が
  HPD/EDID を確定 → AVM が出力解像度を確定 → ADSB が decode 開始」という
  **連鎖の最終段が、xcfb ドライバが返すシンク状態に依存**している。単一の
  ソフトフラグ（`fSendHpdRise` 等）を立てるだけでは decode は始まらない
  可能性が高い。**xcfb の ioctl ハンドラ本体（未入手）を読むまで断定しない。**
- 前後の実機テスト（§9/§10）と合わせた作業仮説:
  - 選択肢1（EDID エミュレータ/ダミープラグ）が依然**最も確実**。
  - ソフトのみで行くなら、`0x523`/`0x524`（plug state）を read して
    HPD/RSEN の実値を確認し、もし「シンク無し」を返すのであれば、
    xcfb ハンドラの READ/WRITE_REG で HPD を模擬できるかが唯一の突破口。
  - そのためには **xcfb_disp を実装する .ko の入手**が必須（現状は未取得）。

### 12.5 read-only プローブ `t4s-probe xcfb`

`.tmp/http/t4s-probe`（armv7-musl, sha256 `24bb7600…c32a`）に実装を追加した。
実機未接続のため**配置していない**。実機復帰後に転送して使う。

- 使い方: `t4s-probe xcfb [reg_hex]`
  1. `/dev/xcfb_disp` を O_RDWR で open（製品と同じ `open64(path,2)`）。
  2. `HDMI_LOCK`(0x500) を発行してトークンを取得。
  3. `plug_state`: `0x523`→`arg1`(HPD 想定)、`0x524`→`arg1`(RSEN 想定)、
     `combined = hpd | (rsen<<1)` を 16 進表示。
  4. `read_reg`（既定 0x3004,0x3000,0x3020,0x3006。引数で 1 つ指定可）を
     `0x520` で read し 16 進表示。
  5. `edid_block_count`（`0x50b`）と、あれば先頭 4 ブロックを
     `EDID_READ_BLOCK`（`0x50c`）で読み、先頭 8 バイトと標準ヘッダ
     `00 ff ff ff ff ff ff 00` の有無を判定。
  6. `HDMI_UNLOCK`(0x501)。
- **write 系 ioctl は実装していない**（`0x521` WRITE_REG, `0x516` SPD,
  `0x51a`/`0x51c` HDCP, `0x52c` SET_METADATA は送らない）。将来 `--write` を
  足す場合もフラグで分離し、既定は read-only のままにする。
- 判定基準:
  - `plug_state` の `combined` が 0（HPD=0,RSEN=0）→ シンク無し。製品の
    `Cable State <None>` と一致するはず。
  - `edid_block_count == 0` または `std_header=false` → EDID が読めない。
  - どちらも「シンク無し」を示すなら、§9.4/§10 の観測と整合。

### 12.6 実験計画（実機復帰後・read-only から）

1. **状態観測（read-only）**: `t4s-probe xcfb` を 1 回だけ実行。
   - `plug_state`, `read_reg`(0x3004 等), `edid_block_count`, 先頭 EDID を記録。
   - 併せて §10 の read-only 手順（`si`, `discover`, `devices count`, bank2,
     `/proc/interrupts` の `HDMI Audio` 増分）を取得。
   - **判定**: HPD/RSEN と EDID が「無し」を返すか。返せば「シンク無し」確定。
2. **xcfb ハンドラの入手**: 実機の `/lib/modules/3.10.24/` から xcfb_disp の
   .ko を read-only で吸い出し、`READ_REG`/`WRITE_REG` のレジスタ意味を確定。
   （本ホストには未取得。ここが唯一の未確定点。）
3. **最小 write の検討**（まだ実施しない）: 手順2で「HPD を模擬できるレジスタ」
   が判明した場合に限り、`--write` を別フラグで実装。1 レジスタ・1 回から。
   - **ウォッチドッグ注意**: §9.4 / phase4-ui §4.4 の通り、画面・xcfb を
     触った後に再起動が観測された事例がある。write は 1 回ごとに間隔を空け、
     直後に `t4s-probe xcfb` と OSD グリッドで状態を確認。異常時は reboot。
   - IOMMU/DMA を持つ HDMI サブシステムへの無根拠な write は、
     ブリックにつながるため**行わない**。

### 12.7 本節の状態保全

実機はオフラインのまま。ホスト上で完結し、`t4s-probe` のビルドのみ実施。
`.tmp/http/t4s-probe` を更新（実機未配置）。mirakc・電源操作・実機アクセスは無し。

---

## 13. 追記（2026-11-14 20:26–21:0x JST）: 青状態 live 中の mirakc API 4K 取得と、本体ボタン経路の read-only 調査

対象: SHARP 4S-C00AS1（ViXS XCode6, ARMv7, Linux 3.10.24）。
この時点で実機は **ユーザーが本体ボタンで電源ON**（boot 20:26 JST, uptime 535s）し、
**製品が 4K(141/45170) を実デコード中の青状態**だった（`si=Some(141/45170)`,
`discover=Ok(readers 1/2)`, `MIXER/Sink/buf_buf=true`）。HDMI は物理的に不在。
制約: キー送信・電源操作・reboot は行っていない。mirakc の起動は行った。

### 13.0 前準備

- mirakc は停止していた（40772 closed）。pidfile `/mnt/t4s/run/mirakc.pid` は **884** だが、
  `/proc/884/comm = kworker/0:1` の **stale pidfile**（mirakc ではない）。`t4s-remote` も停止。
- `rm -f /mnt/t4s/run/mirakc.pid` → `sh /media/usb/ondevice-up.sh` で起動。
  mirakc **pid 959**、`t4s-remote` pid 918（8484）。
- `/api/version` = `{"current":"4.0.0-dev.0","latest":"4.0.0-dev.0"}`。
- `/api/tuners` = `[{...,"types":["BS","CS","BS4K"],"isAvailable":true,"isFree":true,"isFault":false}]`。
- 起動直後 `/api/status`（t4s-remote, 8484）:
  `threads={MIXER:true,Sink:true,buf_buf:true,HevcFactory:true}`,
  `si={ok:true,tlv_stream_id:45170,original_network_id:11,service_id:141}`,
  `regs={ok:true,lock:8,bank2_selection:[0,0,8]}`, `q_uim={msqid:98307,lspid:279,lrpid:279}`。
- `t4s-probe discover` = `Ok(LiveEs{mmt_demux:8, video_readers:1, audio_readers:2})`。
- 設定確認（`/mnt/t4s/opt/t4s/config.yml`）: BS4K チャンネル 6 局、
  `tuners[].command = t4s-tuner --type {{{channel_type}}} --channel {{{channel}}} --duration {{{duration}}} -`,
  `tlv-decoder: cat -`、`decode-filter: t4s-cas --type {{{channel_type}}} --channel {{{channel}}}`。
  BS4K は REMUX で MPEG-TS を出すため `tlv-decoder: cat -`、`t4s-cas` は素通し。

### 13.1 Part A: mirakc API による 4K 取得（1回目 = 成功）

```
GET /api/channels/BS4K/45170/stream?decode=1   (curl --max-time 30)
```

| 項目 | 値 |
|---|---|
| HTTP | **200** |
| bytes | **104,529,920**（30 s でクライアント側タイムアウト。ストリームは自然終了しない） |
| 先頭 16 bytes | `47 40 00 10 00 00 b0 0d b0 72 c1 00 00 00 8d e1` |
| 先頭 4 bytes | `47 40 00 10`（MPEG-TS sync 0x47、program 141 の PAT/PMT は §9.2 と同じ配置） |
| ffprobe | container `mpegts` / duration 10.827 s / bit_rate 29.55 Mbps |

ffprobe ストリーム:

- video: **hevc / Main 10 / 3840x2160 / yuv420p10le / color_transfer=arib-std-b67 (HLG) /
  color_primaries=bt2020**
- audio: **aac_latm / LC / 48000 Hz / 2ch**
- program: `program_id=141`

取得直後の確認:

- `/api/status`: `MIXER/Sink/buf_buf=true` のまま、`si=141/45170`、`lock=8`（**製品の 4K 視聴は
  この時点では継続**）。
- ただし `t4s-probe discover` は **Err**（`no live 4K ES pair found ...`）に変化
  （取得前は `Ok(readers 1/2)`）。→ 取得（またはその停止処理）の前後で
  **製品デコーダの ES ring reader が 0 になった**。

### 13.2 Part A: 2回目（連続取得の確認）= 408 と製品デコード停止

1回目の 10 秒後に同 API を再実行:

```
GET /api/channels/BS4K/45170/stream?decode=1   (curl --max-time 20)
→ HTTP 408 / 38 bytes / body = {"code":408,"reason":null,"errors":[]}
```

mirakc ログ（`/mnt/t4s/var/log/mirakc.log`）:

```
11:37:11 INFO Streaming started channel=BS4K/45170 ... stream.id=0.0.1
11:37:43 INFO Streaming stopped stream.id=0.0.1
11:38:35 INFO Streaming started channel=BS4K/45170 ... stream.id=0.1.1
11:38:54 INFO Streaming stopped stream.id=0.1.1
```

2回目（11:38:35）は tuner が起動したが **最初の TS パケットが来ず**、mirakc は 408 を返した。

**この直後、製品のデコードチェーンが停止した**:

| 時刻 | MIXER | Sink | buf_buf | si | lock |
|---|---|---|---|---|---|
| 取得前 | true | true | true | 141/45170 | 8 |
| 1回目後 | true | true | true | 141/45170 | 8 |
| 2回目後〜5分以上 | **false** | **false** | **false** | 141/45170 | 8 |

`t4s-probe discover` は **Err**、`devices count=12`、IRQ33 (`xcfb interrupt`) は ~120/s で
進行（OSD scanout は生存）。`/api/tuners` は `isFree=true`。**B 以降 5 分以上この状態が継続**。

### 13.3 失敗の機序（推定・コード根拠あり）

mirakc の tuner パイプラインは `t4s-tuner --type BS4K --channel 45170 --duration -` を spawn する。
`rust/t4s-tuner/src/main.rs` の `tune_4k`（`main.rs:204`）は:

```rust
let live_now = si::live_service(...);
if live_now.is_some_and(|l| l.service_id == service_id) && remux4k::discover(drv).is_ok() {
    wait_for_decode_chain(drv)?;   // 0x6004 を送らず live を再利用
    return ...;
}
send_change(service_id, 0x40, tuner::TUNE_CHANNEL_CHANGE)?;   // ← 0x6004 を送る
```

- **1回目**は `live_now=141` かつ `discover=Ok` だったので **0x6004 を送らず** REMUX tap だけを
  張り、§9 と同じ「live 共有取得」で 200 が返った。
- **2回目**は実行時点で `discover` が既に **Err**（reader 0）だったため、条件が成立せず
  **`send_change`（`ADR_COMM_QID` への `0x6004`、`mode=TUNE_CHANNEL_CHANGE`）を送った**。
  これは §10 で観測した「HDMI 無しで 0x6004 を送ると SI のみ復元し MIXER/Sink は true に
  ならない」経路そのもの。結果、製品は SI=141/45170 を保持したまま **デコードを再開せず**
  MIXER/Sink=false に落ちた。
- したがって **2回目の 408 とデコード停止の直接原因は、mirakc 経由の `t4s-tuner` が
  discover=Err のため自動的に 0x6004 を送ったこと**（禁止されている経路を、mirakc の
  tuner コマンドが内部で踏んだ）。

**未確定**: 1回目の取得自体が製品デコーダの reader を外したかどうか。§9 では直接
`t4s-tuner --type BS4K` を 2 回連続実行しても readers 1/2 が維持された。今回の差
（mirakc が `--duration -` で spawn し kill する点、パイプ構成）は未検証。
「API 取得 1 回で discover が Err になった」という観測事実のみ確実。

### 13.4 XCFB read-only プローブ（HDMI 不在の直接確認）

`t4s-probe-new`（xcfb 入り、sha256 `24bb7600…c32a`）で read-only 実行
（`t4s-probe` 本体 sha256 は `83ce8753…f38d` で xcfb 無し。新プローブは `t4s-probe-new` として配置済み）:

```
xcfb ioctl=0x61000 dev=/dev/xcfb_disp
lock token=<取得>
plug_state combined=0x0 (0x523=0x0 bit0=HPD, 0x524=0x0 bit1=RSEN)
read_reg addr=0x3004 value=0x00000001
read_reg addr=0x3000 value=0x000000ce
read_reg addr=0x3020 value=0x00000020
read_reg addr=0x3006 value=0x000000e1
edid_block_count=0
edid: no blocks (sink absent or EDID not readable)
unlock ok
```

→ **HPD=0 / RSEN=0 / EDID block 数 0**。§12.1 の予測どおり。**HDMI シンクは物理的に不在**。
それでも §13.0 の青状態（本体ボタン ON）では 4K が live だった。
（write 系 ioctl は送っていない。read のみ。）

### 13.5 Part B: 本体ボタン経路の read-only 所見

キー送信・電源操作は一切行っていない。観測は `/proc`、文字列、`bdh_out_msgh.lst` のみ。

#### 13.5.1 前面パネルの queue/msgId（既知の再確認）

`/tmp/bdh_out_msgh.lst`（qid→msqid 対応、実機 read-only）:

- `127-98307` = **Q_UIM(0x7F)**
- `123-10748232` = **Q_FPM(0x7B)**

`/proc/sysvipc/msg` で両キューとも **`lspid=lrpid=279`（abs）/ `qnum=0`**。
abs は `T_FPM`(tid 392), `T_FPD`(315), `T_FPD3`(363), `T_FPM_DISP`(639),
`T_UIM`(473), `T_UIM1/2/3`, `T_UIM_THUMB` を保持しており、FPM/FPD タスクは稼働中。
（§1/§phase3 の「FPM → Q_UIM 0x12044 → UIM」経路と一致。）

#### 13.5.2 abs 内の前面パネル/電源/LED 文字列（新規抽出）

`.tmp/oem/abs` の文字列から、本体ボタン＝前面マイコン（FPD/FPM）系の起動・電源シーケンス:

- **電源ON メッセージ**: `msgIniRequestFpmPowerOnOStart`, `msgIniRequestFpmPowerOnQStart`
  （O=通常開始, Q=QuickStart 開始）。
- **QuickStart 判定**: `QuickStartON` / `QuickStartOFF`,
  `[FPM] Fpm_IsQsRestartFromPoff : TRUE/FALSE`。
- **前面マイコン再起動種別**: `FPD_RESTART_NORMAL` / `FPD_RESTART_QSTART`
  （`FPD : Write Restart Data FPD_RESTART_*`）。
- **電源キー**: `FPM : eFpmPowerOffKey() msgFpmRequestUimRestartParticular RC_POWER` /
  `RC_POW_ON`。`[IpControl] Power Key Not Send to FPM`（IP Control 由来の電源キーは FPM へ
  送らない分岐）。
- **起動理由**: `STARTREASON_NORMAL` / `_TV_VIEW` / `_EXTERNAL` / `_QUICK` / `_FIRST` /
  `_EPG_*` / `_FACTORY` / `_FLASHCLEAR` / `_DOWNLOAD` / `_PCCARD` / `_TIMEGET` / `_UNDEF`、
  `[FPM] Clear AC Recovery mode by ChangeStartReason.`,
  `FPM : Reserve End.  Change StartReason -> NORMAL`。
- **LED**: LED 制御は FPM ではなく表示サブタスク **`T_FPM_DISP`(0x639)** 側。
  `[FPMD DBG] msgUimRequestFpmLedEnable Send.` / `msgUimRequestFpmLedDisable Send.`,
  `[FPMD] SND_LED_ANS Timeout!`（LED は UIM→FPM_DISP のメッセージで enable/disable）。

#### 13.5.3 電源ON 時の「前回視聴チャンネル復元」

- tune ステートマシンに **`WAIT_TUNE_LAST_CH`** 状態が存在する
  （`WAIT_TUNE_DEFAULT_CH` / `WAIT_TUNE_CHANGE_CH` / `WAIT_TUNE_RETURN_CH` 等と同じ enum 群）。
- チャンネル情報の保存/復元は **SDA/SDM**:
  `eSdmSetLastChInfo` / `eSdmGetLastChInfo`,
  `[eSdaGetLastChInfo]: NETWORK_TYPE_CS2 -> NETWORK_TYPE_CS1`（保存値の正規化）,
  `fLastChannelInvalid`（無効フラグ）, `[LastChannel]EXTENT5`。
- 起動時の復元系: `eDBRListRestore`, `BUP_API_IsNeedRestore`, `## bVOBURestore TRUE 3`。
- 推定（文字列＋§10 の実測から）: **本体ボタンの電源ON は `msgIniRequestFpmPowerOnOStart`
  または `...QStart` で始まり、起動理由に応じて `eSdmGetLastChInfo` →
  `WAIT_TUNE_LAST_CH` により前回視聴チャンネル（= 直前に見ていた 4K 141/45170）を復元し、
  デコードを開始する**。これが「本体ボタンで青（4K 視聴）に入れる」経路。
- 一方、§10 の **ソフトリブート（`reboot`）は `FPD_RESTART_NORMAL` / `eFpdRestart` 系の
  別経路**を通り、上記の「電源ON＝前回チャンネル復元」を再実行しないため `si=None` の
  ままになる、という §10 との差を説明できる。
- **未確定**: 実際のボタン押下でどの msgId が発行されるか、`bAdsbInitialized` の現在値、
  `eSdmGetLastChInfo` が返す保存値。今回はキー送信・pagemap 読みを行っていないため未確認。

#### 13.5.4 まとめ（Part B）

- 本体ボタン経路は **前面マイコン（FPD/FPM, Q_FPM=0x7B）→ UIM（Q_UIM=0x7F）** で、
 キー入力は `abs` の FPM/FPD タスクが処理する。本 boot でも FPM/FPD タスクは稼働。
- 「青（4K 視聴）」に入る条件は **電源ON（本体ボタン）時の通常/Quick 起動と、
  前回視聴チャンネル（SDA/SDM 保存）の復元 → tune (`WAIT_TUNE_LAST_CH`)**。
  これは HDMI シンク有無とは独立（§13.4 で HPD=0/EDID=0 を確認）。
- ソフトリブートが 4K 復帰しない §10 の挙動は、起動種別（FPD_RESTART_NORMAL/
  QSTART）と AC 復帰/起動理由の差で説明できる（推定）。
- LED（青/緑）は `T_FPM_DISP` の `msgUimRequestFpmLedEnable/Disable` 系で制御される。

### 13.6 状態保全（テスト後）

- mirakc: **pid 959** 稼働（`/mnt/t4s/run/mirakc.pid`=959、40772 listen）。
  `t4s-remote`: pid 918（8484）。`/mnt/t4s`（loop0）rw マウント維持。
- 製品: `MIXER/Sink/buf_buf=false`、`si=141/45170`（SI は保持）、`lock=8`,
  `bank2_selection=[0,0,8]`, `devices count=12`, load ~30。
  **§10 と同じ「SI は残るがデコードが再開しない」失敗状態**で、5 分以上復帰せず。
  復帰には本体ボタン（電源再投入）または通常の選局操作が必要。
- キー送信・電源操作・reboot は行っていない。mirakc の再起動は 1 回（起動のみ）。
- 実施したのは: mirakc 起動 / API 4K 取得 2 回 / read-only 観測 / xcfb read-only プローブ。

### 13.7 結論

1. **青状態（本体ボタン電源ON）で製品が 4K を実デコードしていれば、HDMI なしでも
   mirakc API `BS4K/45170/stream?decode=1` は HTTP 200 で HEVC Main10 4K HLG + AAC-LATM の
   TS を配信できる**（1回目: 104,529,920 bytes / 30 s）。
2. **HDMI は完全に不在**（xcfb ioctl 0x61000 の read-only で HPD=0, RSEN=0,
   edid_block_count=0）。§9.4 の「シンク有無を確定できない」は本測定で **シンク無しに確定**。
3. **連続取得（2回目）は失敗（408）し、その過程で `t4s-tuner` が `0x6004` を送って
   製品が §10 の失敗状態（MIXER/Sink=false）に落ちた**。原因は `tune_4k` の
   「discover=Err なら 0x6004」フォールバック。**4K の API 再取得は discover=Ok の
   ときのみ行うべき**（Err の状態で叩くと 0x6004 が飛ぶ）。
4. 本体ボタンで青に入る経路は前面マイコン FPD/FPM → UIM で、電源ON 時の
   前回チャンネル復元（`eSdmGetLastChInfo` → `WAIT_TUNE_LAST_CH`）と推定。
   ソフトリブートが復帰しない差は起動種別の差で説明できる（未確定）。

---

## 14. 追記（2026-11-14 21:32–22:27 JST）: HDMI なしでの 4K チャンネル切替検証

対象: SHARP 4S-C00AS1。HDMI は物理的に不在（§13.4 で HPD=0/RSEN=0/EDID=0 確定）。
実機は 21:19 / 21:52 に**ユーザーが本体ボタンで再起動**し、21:32:33 に**緑状態**
（`MIXER/Sink/buf_buf=true`, `si=141/45170`, `discover=Ok(readers 1/2)`）へ復帰した。
制約: キー連打・0x6004 連投・電源/reboot は行っていない。

### 14.1 ベースラインと live を壊さない取得（t4s-tuner 直接）

緑（21:33）で `t4s-tuner --type BS4K --channel 45170 --duration 8 -` を 1 回:

| 項目 | 値 |
|---|---|
| rc / bytes | 0 / **19,801,288** |
| 先頭 4 B | `47 40 00 10` |
| remux4k | 0 resyncs、longest gap 340 ms recovered |
| 取得後 | `MIXER/Sink=true`, `si=141/45170`, `discover=Ok(readers 1/2)` 維持 |

**t4s-tuner 直接の tap は live を壊さない**（§9 と同じ）。

### 14.2 キー経路（CH UP 0x00C）は 4K 局を選べない

`uimkey 0x00C 0 0x12044` → 0.2s → `uimkey 0x00C 2 0x12044`（msg 0x12044, type0+type2）:

- 送信約 80 秒後の時点で **si は 141/45170 のまま**、`discover=Ok(readers 1/2)`、
  `MIXER/Sink=true`、`last_key` は null。**選局に至らない**。
- （その後 si が 101/45328 等へ変化したのは、この検証と並行して別経路で
  `send <service> 0 0`（0x6004）が実行されたためで、キー単独の効果ではない。
  §15 で切り分け済み。）

### 14.3 0x6004（`send <service> 0 0`）は 4K 切替に有効

緑（`discover=Ok`）のとき `send 101 0 0` を 1 回:

- si **141/45170 → 101/45328**、`discover=Ok(readers 1/2)` 維持、`MIXER/Sink=true`。
- `t4s-tuner --type BS4K --channel 45328 --duration 8 -` → rc=0 / **13,998,480 B** /
  先頭 16 B `47 40 00 10 00 00 b0 0d b1 10 c1 00 00 00 65 e1`
  （TSID 0xB110=45328, program 101）。
- ffprobe: video **hevc / Main 10 / 3840x2160 / yuv420p10le /
  color_transfer=bt2020-10 / color_primaries=bt2020**、audio **aac_latm / 48000 / 2ch**。
  （NHK BS4K(101) は HLG ではなく bt2020-10。§phase4-bsp4k 冒頭の局別色表と一致。）
- 参考（別経路の記録）: mirakc API `BS4K/45328/stream?decode=1` は HTTP 200 /
  41,091,072 B（15 s クライアント timeout）でも取得できた。

### 14.4 判定

- **HDMI なしでも、緑状態であれば (a) キー以外の `0x6004` 切替、(b) `t4s-tuner` 直接取得、
  (c) mirakc API 配信、がすべて可能**。緑状態の成立条件は HDMI シンクではなく
  「本体ボタン電源ON → 前回チャンネル復元 → 4K デコード開始」（§13.5）。
- **キー（CH UP）では 4K 局を選べない**。
- ただし 0x6004 と API の使い方によっては live を失う（→ §15）。
- 検証用 TS は `.tmp/hdmi-free/{step1-45170,step2-45328}.ts` に保管（リポジトリ非追跡）。

---

## 15. 追記（2026-11-14 22:00–22:27 JST）: discover=Err の条件と対策

目的: §13.2 で観測した「4K の live を失う（`discover=Err`, `MIXER/Sink=false`）」の
**正確な境界条件**を、緑状態を起点に 1 条件ずつ切り分けて特定する。

### 15.0 前提と用語

- 用語は **「緑状態（視聴状態）」** に統一（冒頭の訂正注記）。緑 = `MIXER/Sink/buf_buf=true`
  かつ `discover=Ok(readers≥1)`。
- 実機は 21:52 にユーザー再起動 → bring-up 後 22:00 に緑復帰
  （`mirakc` pid 1257, `t4s-remote` pid 1261, `si=141/45170`, `discover=Ok(1/2)`）。
- 各手の後に必ず `t4s-probe-new discover` と `/api/status` を確認。

### 15.1 各手と結果（時系列）

| # | 操作 | si | discover | 判定 |
|---|---|---|---|---|
| 基準 | （緑、tap なし） | 141/45170 | Ok(1,2,8) | — |
| 1.1 | `send 141 0 0`（**同一局** 0x6004 再送） | 141/45170 | **Ok(1,2,8)**（20 s 通し） | 安全 |
| 1.2 | `send 101 0 0`（**別局・単発**、tap なし） | →101/45328 | **Ok(1,2,8)** | 安全 |
| 1.3 | `send 151 0 0`（30 s 以上あけて別局） | →151/45168 | **Ok(1,2,8)** | 安全 |
| 1.4a | `t4s-tuner --channel 45168 --duration 6` 直接 | 151/45168 | Ok(1,2,8)、rc=0 / 15,075,908 B | 安全 |
| 1.4b | `send 161 0 0`（tuner tap の直後） | 151→161/45329 | **一時 Err(~4 s) → Ok(1,2,8) に回復** | 一過性 |
| 1.4c | **mirakc API #1** `BS4K/45329/stream?decode=1` | 161/45329 | HTTP200 / 48,005,120 B、直後 **Ok** | 安全 |
| 1.4d | **API #2, #3**（10 s あけて連続取得） | 161/45329 | → **Err**（`MIXER/Sink` は true のまま） | **破壊** |

mirakc ログ（`/mnt/t4s/var/log/mirakc.log`）:
`Streaming started channel=BS4K/45329 stream.id=0.0.1 (13:05:23)` →
`0.1.1 (13:06:37)` → `0.2.1 (13:07:29)`。以後 `discover=Err`。

### 15.2 コード経路（Step 2）

- `t4s-probe-new send <sid> <mode> <nd>`（`rust/t4s-probe/src/main.rs:821`）と
  `t4s-tuner` の `send_change`（`rust/t4s-tuner/src/main.rs:180`）は、
  **同一の 0x6004 ボディ** `[0, oid, 9, tune, 0x0b, sid, freq, tsid, not_decode, bid]` を
  qid **0x36F (ADSB_COMM)** へ `msgsnd` する（`tuner.rs::ChangeChannelRequest::body`）。
  相違は `oid` のみ（probe 0x41 / tuner 0x40）。
- `tuner.rs:1163` の警告: **「live ES ring への tap は 0x6004 の前に解放しないと、
  製品側のバッファ teardown が `XCE_IN_USE` で失敗する」**。
- mirakc の tuner は `t4s-tuner --duration -` を spawn し、クライアント切断で kill する。
  → **tap の生成/解放が短時間に重なると製品側 teardown が失敗**しうる。

### 15.3 結論（Err になる条件）

- **仮説の棄却**: 「別サービスへの 0x6004 単発」は原因では**ない**（1.2/1.3 とも Ok 維持）。
- **支持された仮説**: **「短間隔の連投」**。
  - 1.1–1.3 の `send` 単発はすべて安全。1.4b でも tap 直後の単発は一過性 Err→回復。
  - **落ちたのは mirakc API の連続取得（同一局を約 2 分で 3 回、`--duration -` の
    spawn/kill が連続）**の後（1.4d）。
- 機序（推定）: mirakc 由来の `t4s-tuner` tap が解放される前に次の 0x6004 / tap が
  入ると、製品の buffer teardown が `XCE_IN_USE` で失敗 → live 喪失。
  `discover=Err` は「**tap と 0x6004 の近接 × 繰り返し**」で発生する。
- 補足: 一度 Err になると、たとえ `MIXER/Sink=true` が残っていても
  **`discover` は製品が再デコードしない限り Ok に戻らない**。

### 15.4 ソフトでの緑復帰 = 不可能（本体ボタンが必要）

Err 状態（22:08, `MIXER/Sink=true`, `si=161/45329`, `discover=Err`）から 1 手ずつ試行:

| # | 操作 | 結果 |
|---|---|---|
| 3a | `send 161 0 0`（SI と同局へ再送） | Err のまま 20 s → 約 2 分後さらに **MIXER/Sink=false へ悪化** |
| 3b | `send 161 3 0`（TUNE_RESUME=製品の resume 経路） | Err のまま |
| 3b2 | `send 141 0 0`（別局） | **si すら変化せず**（ADSB TUM が 0x6004 を処理しない） |
| 3c | `adsbinit`（0x6001→0x6002→0x6004, 製品の初期化列） | Err のまま |

- 補足: Err 化後は ADSB_COMM キュー（msqid 1605681, qid 0x36F）・Q_ADSB_TUM
  （msqid 884763）は空（`qnum=0`）だが、**製品側 TUM 状態機械が再 tune/再デコードを行わない**。
- **ソフトで緑に戻す手段は見つからなかった。復帰には本体ボタンでの電源ON
  （`eSdmGetLastChInfo` → `WAIT_TUNE_LAST_CH` の前回チャンネル復元）が必要**。
  本測定でも 21:19 / 21:52 の再起動後に緑復帰を実測している。

### 15.5 推奨運用

1. **緑（`discover=Ok`）のときだけ** 4K 取得/切替を行う。`discover=Err` のときは
   **0x6004 を送らない**（3a で悪化を実測）。
2. **mirakc API の同一局連続取得は避ける**。1 局につき 1 回、次まで **10 秒以上**あける。
   連投（`--duration -` の spawn/kill 連続）が Err の主因。
3. `send`（0x6004）での切替は **1 手ずつ、各後に `discover` を確認**。単発は安全。
   直前に `t4s-tuner` / API の tap を動かさない（近接させない）。
4. 取得前に `t4s-probe-new discover` で `readers≥1` を確認してから実行する。
5. Err 化したら即その手を止め、**ユーザーに本体ボタンでの緑復帰を依頼**する。

### 15.6 状態保全（テスト後）

- mirakc pid 1257（40772）/ t4s-remote pid 1261（8484）稼働、`/mnt/t4s`（loop0）rw 維持。
- 製品: `MIXER/Sink/buf_buf=false`, `si=161/45329`, lock=8, bank2=[0,0,8], load ~28。
  §10 と同じ失敗状態で、緑復帰待ち（本体ボタン）。
- 実施: bring-up 1 回 / `send` 計 6 回 / `t4s-tuner` 3 回 / API 3 回 / read-only 観測。
  キー連打・電源操作・reboot は行っていない。

### 15.7 追記（2026-11-14 23:51 – 2026-11-15 00:08 JST）: 緑復帰後の確認検証と os error 1038

ユーザーが本体ボタンで緑に復帰（`mirakc` pid 937 / `t4s-remote` pid 941、23:51:29 に
`MIXER/Sink/buf_buf=true`, `si=141/45170`, `discover=Ok(readers 1/2)`）。安全運用
（1 手ずつ・各後 discover 確認・10 秒以上あける・API 不使用）で再確認した。

| # | 操作 | si | discover | 取得 |
|---|---|---|---|---|
| 開始 | 緑 | 141/45170 | Ok(1,2,8) | — |
| CV-A | `t4s-tuner --channel 45170 --duration 6` 直接（tap 無し・3 回試行） | 141/45170 | **Ok 維持** | **rc=1 / 0 B / os error 1038**（REMUX open failed, retried once → 失敗） |
| CV-B | `send 101 0 0` | →**101/45328** | 一過性 Err → Ok(0,0,8) → **Ok(1,2,8) に回復** | — |
| CV-C | `t4s-tuner --channel 45328 --duration 6` 直接 | 101/45328 | Ok 維持 | **rc=1 / 0 B / os error 1038** |
| CV-D | `send 141 0 0` | **101/45328 のまま変化せず** | → **Err（持続）** | — |

最終状態（00:08）: `MIXER/Sink/buf_buf=true`（製品デコードは生存）だが `discover=Err`
（持続）、`si=101/45328`。4 回以上ポーリングしても Err のまま。以降は送信を停止。

**観測 1: `t4s-tuner` 直接は os error 1038 で失敗しうる（discover=Ok でも）**。
`rust/t4s-core/src/bs2k.rs:328-338` / `tssrc.rs` の記載どおり、以前の host REMUX tap が
解放されず残っていると `SDEV_CREATE` / REMUX create が **XCE_TIMEOUT(1038)** を返す。
今回はセッション開始時点（緑・`discover=Ok`）から既に 1038 だった。**製品デコード
（`discover`）が健全でも、host tap 側は wedged になりうる**。これは §13.2 / §15.3 の
「API 連続取得」の残骸とみられ、`scripts/softreboot.sh` の記載どおり**再起動でしか
解放されない**（CV-B の 0x6004 再構築で一時的に回復したが、CV-C で再発）。

**観測 2: 0x6004 は wedged tap を一時的に解消しうるが、恒久復帰にはならない**。
CV-B で `send 101` により discover が Ok に戻った（tap 再構築が通った）が、直後の
CV-C で再び 1038。

**観測 3: `discover=Err` 後は 0x6004 が効かない**。CV-D の `send 141 0 0` は si すら
変えられず（101/45328 のまま）、discover=Err が持続。§15.4 の 3b2 と一致。

**結論（確認検証）**: 前段の結論を再確認。**Err を避けるには (1) 緑のときだけ取得、
(2) 1 局 1 回・10 秒以上あける、(3) tap と 0x6004 を近接させない、(4) Err では送らない、
が有効**。加えて **緑でも host REMUX tap が wedged（os error 1038）なことがあり、その
場合 `t4s-tuner` 直接取得は失敗する。回復は再起動のみ**。`discover` が Ok であることは
host tap が健全であることを保証しない。

---

## 16. 追記（2026-11-15）: 4K tap の teardown と os error 1038

> 実機検証は §16.7 に記載（2026-11-15、3 回連続取得で合格）。

### 16.1 1038 の正体と発生箇所

- `os error 1038` は Rust の `io::Error::last_os_error()` が保持する **errno 1038**。
  `xcdrv` の全 ioctl は `sys::ioctl_ptr`（`rust/t4s-core/src/sys.rs:39-43`）/
  `sys::ioctl_value`（同 `:53-57`）を通り、失敗時にこの errno をそのまま返す。
- このドライバでは **1038 = `XCE_TIMEOUT`**（`xcdrv.rs:267`、`docs/ARCHITECTURE.md:35`、
  `docs/CS-STATUS.md:214`）。**`XCE_IN_USE`（-1040）ではない**。`tuner.rs:1163` の
  `XCE_IN_USE` は「製品側が自分のバッファを解体する際、host tap がまだ
  reader/connection を握っている」ときに製品側で起きるエラーで、1038 はその
  上流（ARC がバッファ書き込みで詰まり、device-control が timeout する）現象。
- `Tap::open` の失敗経路で 1038 を返し得る実呼び出しは、`XcDrv::create_remux`
  （`SDEV_CREATE`, `xcdrv.rs:887-896`、実際の ioctl は `:890`）と
  `XcDrv::create_remux_tsout_buffer` / `create_remux_tsinfo_buffer`
  （`SBUF_CREATE`, `xcdrv.rs:916-936`）。今回のログは
  `REMUX open failed (... os error 1038)` までで、どの ioctl かまでは出ていない。
  **前回 tap の REMUX device / TSOUT・TSINFO ring が残っていれば、この create が
  1038 で落ちる（推測。bs2k の `SDEV_CREATE` 1038 と同じ機序）**。
- 一度 1038 になると再起動まで直らないのは、**残った device/ring を誰も
  destroy できず、次の create が同じ timeout を返し続ける**ため。

### 16.2 bs2k との差分（旧 remux4k teardown の問題）

`bs2k::Source::drop`（`bs2k.rs:254-388`）は次を行う:

1. `quiesce_ts_output` で出力元を止める。
2. reader を drain する。
3. PID/connection を外す。disconnect が timeout したら **stop して再試行**。
4. ring の writer/reader が 0 になるまで待ち、`force_reset_buffer` →
   `destroy_buffer`。
5. destroy 後も `sbuf_status_raw` が成功するなら「ring が残っている = 次の
   start は 1038」と**明示ログ**し、`dirty` を立てる。

旧 `remux4k::Tap::teardown`（修正前）は:

- reader を drop するだけで **drain しない**。
- disconnect / stop が **timeout 2000 ms 固定・再試行なし**。
- TSOUT/TSINFO ring を **reset せず、destroy 後の存在確認もしない**。
- `Tap::close()` が無く、Drop だけ。二重解放の概念も無い。
- `t4s-tuner` 終了直後にプロセスが消えるため、製品/ARC の後始末が次回 open と
  競合し得る。

つまり 2K で解消済みの「backpressure で ARC が詰まる → 全 device-control が
1038 → ring が残る」に対する防御が、4K REMUX 側だけ欠けていた。

### 16.3 修正内容

- `remux4k::Tap` に **明示的 `close()` を追加**（`remux4k.rs:731`）。`Drop` からも
  呼ぶ。`closed` フラグで**冪等**（二重 close で二重 destroy しない）。
- teardown を `close_graph()`（`remux4k.rs:927`）に集約し、順序を固定:
  1. TSOUT reader を **最大 250 ms drain** してから drop（writer の parked write
     を解く）。
  2. TSOUT/TSINFO の output connection を **timeout 3000 ms・再試行付き**で
     disconnect。
  3. `stop_device` を **timeout 3000 ms・再試行付き**で実行。
  4. stop 後に timeout した output disconnect を**もう一度再試行**。
  5. stream / program を remove（timeout は再試行後 benign）。
  6. video/audio ES input を disconnect（同）。
  7. TSOUT/TSINFO ring の writer/reader が 0 になるのを最大 2 s 待ち、
     `force_reset_buffer` → `destroy_buffer`。**destroy 後も `sbuf_status_raw`
     が成功するなら「解放できなかった」と明示ログ**し、cleanup 失敗として扱う。
  8. 最後に REMUX device を destroy。失敗/timeout は同様に明示ログ。
- **1038/1039 の扱いを分離**:
  - disconnect / stop / remove / reset の timeout は **再試行後に benign**。
  - ring/device の destroy 後に**実体が残っている場合は hard failure**。
    「benign」で隠さず、`remux4k: teardown could not release ...` を残す
    （`remux4k.rs:896` の `timeout_noise`、`:900` の `retry_cleanup`、
    `:927-` の `close_graph` / `CloseReport`）。
- `XcDrv::disconnect_timeout`（`xcdrv.rs:1278`）を追加し、REMUX 用 disconnect が
  呼び出し側指定の timeout を使えるようにした（既存 `disconnect` は 2000 ms の
  ままなので bs2k の挙動は不変）。
- `t4s-tuner` の 4K 終了時に **300 ms の待機**を追加（`t4s-tuner/src/main.rs:270-275`）。
  tap の Drop（=close）後に ARC の後始末が終わる前にプロセスが消えないようにする。

### 16.4 ユニットテスト（ホスト側で実施済み）

`rust/t4s-core/src/remux4k.rs` の `close_tests` で、実ドライバ無しに
`RemuxOps` の fake を差し込んで検証:

- `teardown_detaches_outputs_stops_then_destroys_rings_before_the_device`:
  output disconnect → stop → stream/program remove → input disconnect →
  reset/destroy ring → device destroy の順序を検証。
- `second_close_is_a_no_op_and_does_not_destroy_anything_twice`:
  2 回目の close が ioctl を一切追加しない（冪等）ことを検証。
- `os_error_1038_on_disconnect_is_retried_and_then_benign`:
  1038 が再試行後に benign になり、cleanup_ok を落とさないことを検証。
- `a_ring_that_survives_destroy_is_reported_as_a_hard_leak`:
  destroy 成功でも status が残る ring を leaked として報告することを検証。

実行結果: `cargo test -p t4s-core` → **150 passed / 0 failed**。
`cargo check --workspace --release --target armv7-unknown-linux-musleabihf` → 成功
（既存の `t4s-call` の未使用 import/dead_code 警告のみ）。

### 16.5 実機復帰後の検証手順（**実施済み → §16.7**）

前提: 本体ボタンで緑復帰済み。**1038 が出たら 0x6004 を送らない**（§15.4）。

1. 緑の確認:
   ```sh
   t4s-probe-new discover        # Ok(video_readers>=1, audio_readers>=1) を確認
   ```
2. 同一局を 3 回連続取得する。各回の間に **10 秒以上**あける:
   ```sh
   for i in 1 2 3; do
     t4s-tuner --type BS4K --channel 45170 --duration 10 > /tmp/tap4k-$i.ts 2> /tmp/tap4k-$i.err
     echo "run $i rc=$? bytes=$(stat -c %s /tmp/tap4k-$i.ts)"
     t4s-probe-new discover
     sleep 10
   done
   ```
3. 合格条件:
   - 3 回とも `rc=0` かつ `bytes > 0`。
   - 3 回とも stderr に `os error 1038` が無い。
   - 各回後の `discover` が `Ok(readers 1/2)` のまま。
   - stderr に `remux4k: teardown could not release ...` が無い。
4. 失敗時:
   - `/tmp/tap4k-$i.err` と `discover` の出力を保存。
   - `could not release buffer/device` が出た場合は host tap が残った可能性が高い。
     それ以上 `t4s-tuner` / 0x6004 を連打せず、復帰手順（再起動）を検討する。

### 16.6 未検証・残課題

- TSOUT ring は製品リテラルの 0x468000（4.5 MiB）のまま。ホスト読み出しが
  4K の実ビットレートに追いつかない場合は、drain だけでは writer を解けず
  再び 1038 になる可能性がある（**推測**。2K のように ring 拡大が必要になるかは
  実機の bytes/所要時間を見て判断する）。
- `discover=Ok` は製品デコードの健全性しか保証せず、host tap の残骸は検出しない
  （§15.7 の観測どおり）。今回の修正は「残骸を作らない」方向の対策であり、
  既に wedged な個体をソフトで解放するものではない。

### 16.7 実機検証（2026-11-15 00:26–00:33 JST、**合格**）

ユーザーが再起動 → 本体ボタンで緑復帰（`si=141/45170`, `discover=Ok(readers 1/2)`,
uptime 68 s）。mirakc/t4s-remote は未起動の状態で検証を実施。

**ビルド・配置**

| 項目 | 値 |
|---|---|
| ソース | HEAD `a11000f`（remux4k teardown 修正入り） |
| ビルド | `cargo +1.99.0 build --release --target armv7-unknown-linux-musleabihf -p t4s-tuner`（9.55 s, 成功） |
| `cargo test -p t4s-core` | **150 passed / 0 failed** |
| ホスト binary | 566,228 B / md5 `93e48326949601be314e1478f66e73fe` |
| 旧 device binary | 552,884 B / md5 `43c416c53b99fa9d54a8e47018691b6e`（Nov 14 04:03 = 修正前） |
| 配置 | `/tmp/new-tuner` へ wget（md5 一致）→ `cp` で `/mnt/t4s/opt/t4s/bin/t4s-tuner` を置換、`chmod 755`。旧版は `t4s-tuner.old` に退避 |
| 配置後 md5 | **`93e48326949601be314e1478f66e73fe`**（一致）, 566,228 B, `-rwxr-xr-x` |

**3 回連続取得（各回 10 秒以上あけ、`--type BS4K --channel 45170 --duration 10`）**

| 回 | rc | bytes | 先頭 4 B | `os error 1038` | `teardown could not release` | 取得後 discover | teardown drain |
|---:|---:|---:|---|---:|---:|---|---|
| 1 | 0 | 30,822,976 | `47 40 00 10` | 0 | 0 | Ok(1,2,8) | 982,656 B |
| 2 | 0 | 31,138,440 | `47 40 00 10` | 0 | 0 | Ok(1,2,8) | 576,000 B |
| 3 | 0 | 31,100,276 | `47 40 00 10` | 0 | 0 | Ok(1,2,8) | 1,124,352 B |

- **合格条件をすべて満たした**: 3 回とも rc=0 / bytes>0 / 1038 なし / teardown leak なし、
  かつ各回後・全 3 回通して `discover=Ok(readers 1/2)` を維持（製品デコードは無傷）。
- 旧バイナリでは同条件で `os error 1038`（REMUX open failed）が再現していた（§15.7 CV-A/C）。
  修正版は 3 回連続で 1038 を出さず、**teardown 修正の実機効果を確認**。

**ffprobe（run 1 を host へ putfile、`.tmp/tap4k/run1-45170.ts`）**

- container mpegts / 188 B TS / sync ratio **1.0**。
- video: **hevc / Main 10 / 3840x2160 / yuv420p10le / color_transfer=arib-std-b67 (HLG) /
  color_primaries=bt2020**。
- audio: **aac_latm / LC / 48000 Hz / 2ch**。

**mirakc 起動**

- `sh /media/usb/ondevice-up.sh` → mirakc pid 989 / t4s-remote pid 993（8484）。
- 起動後 `/api/status`: `MIXER/Sink/buf_buf=true`, `si=141/45170`, `discover=Ok(1,2)`
  を維持。**API の 4K 連続取得は行っていない**（§15.3 の Err 主因のため）。

**結論**: remux4k teardown 修正（`a11000f`）により、4K tap を 3 回連続で開閉しても
`os error 1038` を起こさず、緑状態を維持したまま反復取得できることを実機で確認した。

---

## 17. 追記（2026-11-15 00:36–00:49 JST）: mirakc API 4K 連続取得（teardown 修正後）— **不合格**

目的: teardown 修正版（`t4s-tuner` md5 `93e48326…`, 566,228 B）で、mirakc API の
**同一局 4K 連続取得**（§15.3 の Err 主因）が解消したかを検証する。
§16.7 の「t4s-tuner 直接 3 回連続」は合格していたため、差は **mirakc 経由**
（`--duration -` を spawn し、クライアント切断で kill する点）にあると予想した。

### 17.1 前提

- 実機は緑（`MIXER/Sink/buf_buf=true`, `si=141/45170`, `discover=Ok(1,2)`）。
- mirakc pid 989 / t4s-remote pid 993。`/mnt/t4s` rw。
- 配置済み `t4s-tuner` md5 `93e48326949601be314e1478f66e73fe`（§16.7 と同一）。
- 手順: `curl --max-time 20 '…/api/channels/BS4K/45170/stream?decode=1'` を
  各回の間に **10 秒以上**あけて 3 回。各回直後に `/api/status` と `discover`。

### 17.2 結果

| 回 | 時刻 | HTTP | bytes | 先頭 4B | 直後 MIXER/Sink | 直後 discover |
|---:|---|---:|---:|---|---|---|
| 1 | 00:37:10 | **200** | 66,912,256 | `47 40 00 10` | true / true | **Ok(readers 2/3)** |
| 2 | 00:38:05 | **404** | 38 | `7b22636f`（`{"code":404…`） | true / true | **Err**（その後…） |
| 3 | — | **未実施** | — | — | — | — |

- **API #1 は成功**（200 / 66,912,256 B / 先頭 `47 40 00 10`）。
- **API #2 は 404**（`{"code":404,"reason":null,"errors":[]}`、38 B）。mirakc の
  `error.rs` では 404 = `TunerUnavailable` / `TunerNotFound` / `ChannelNotFound` /
  `ServiceNotFound` のいずれか。**HTTP 200 の最初の TS パケットが来る前に失敗**した。
- **API #2 の後、`discover=Err`** となった。**API #3 は実施せず**、以降 `send` もキーも
  送っていない（プロトコルどおり即停止）。
- その後 00:48 にかけて **`MIXER/Sink=false` へ悪化**（§10 の失敗状態）。4 回以上
  ポーリングしても Err のまま（持続）。

mirakc ログ（`/mnt/t4s/var/log/mirakc.log`）:

```
15:37:09 Streaming started channel=BS4K/45170 ... stream.id=0.0.1   (API#1)
15:37:31 Streaming stopped stream.id=0.0.1
15:38:04 Streaming started channel=BS4K/45170 ... stream.id=0.1.1   (API#2)
15:38:14 Streaming stopped stream.id=0.1.1
```

（API#2 は tuner を起動したが、10 秒で停止 = 最初の TS パケットが来ず 404。）

### 17.3 ffprobe（API #1: `.tmp/api4k/api4k-1.ts`）

- container **mpegts** / duration 18.64 s / bit_rate 28.73 Mbps / sync ratio **1.0**。
- video: **hevc / Main 10 / 3840x2160 / yuv420p10le / color_transfer=arib-std-b67 (HLG) /
  color_primaries=bt2020**。
- audio: **aac_latm / LC / 48000 Hz / 2ch**。

API #1 自体は健全な 4K TS を返している（旧 §13.1 と同じ）。

### 17.4 考察（なぜ直接取得は通り、API 連続は落ちるか）

- §16.7 の直接 3 連続は、`t4s-tuner` が `--duration 10` で**自分で正常終了**し、
  `Tap::drop`（=冪等 `close()`）→ 300 ms 待機 → プロセス終了、の順で後始末が完走した。
- mirakc 経由は `--duration -` で**クライアント切断時に外部から kill** される。
  mirakc は **SIGTERM → 2 秒の猶予 → SIGKILL**（`command_util.rs:51-310`,
  `kill_on_drop(true)`）。`t4s-tuner` は SIGTERM を捕捉して `STOP` を立て
  `pump` から抜ける設計（`main.rs:37-44, 142-143`）だが、
  - **#1 で readers が 2/3 に増えていた**（旧 tap または ES reader が一部残った）こと、
  - **#2 が最初の TS パケットを得られず 10 秒で停止**したこと、
  から、**kill 経路では teardown が完走せず tap/ES reader が leak し、次の open を
  妨げる**と推定される。§15.3 の「tap と 0x6004 / 次 tap の近接」と同じ機序。
- したがって **remux4k teardown 修正（`a11000f`）は「正常終了」経路では有効
  （§16.7 合格）だが、mirakc の kill 経路（SIGTERM→SIGKILL）の連続取得では不十分**。

### 17.5 合格条件と判定

- 合格条件: 3 回とも HTTP 200 / bytes>0 / 各回後 `discover=Ok` 維持。
- 判定: **不合格**（API #1 のみ 200、API #2 が 404、以後 `discover=Err` → `MIXER/Sink=false`）。

### 17.6 推奨運用（API 経由）

1. **mirakc API の同一局 4K 連続取得は引き続き避ける**（§15.3/§15.5 と同じ）。
   1 局につき 1 回、次まで十分な間隔（少なくともホスト tap の後始末が終わるまで）あける。
2. 反復取得が必要なら、**直接 `t4s-tuner --duration <sec>`（正常終了）を使う**方が
   現状は安全（§16.7 で 3 連続合格）。
3. mirakc kill 経路（SIGTERM/SIGKILL）でも teardown を完走させるには、
   - SIGTERM 捕捉時に DriveGraph を明示 `close()` してから exit する、
   - もしくは mirakc 側の SIGTERM 猶予（`MIRAKC_COMMAND_PIPELINE_SIGTERM_GRACE_NANOS`）
     を長めに取る、
   などの追加対策が必要（**未実装・要検討**）。
4. Err 化したら API も `send` も止め、本体ボタン（+ wedged tap 解放の再起動）で復帰。

### 17.7 状態保全

- mirakc pid 989 / t4s-remote pid 993 稼働。製品は `MIXER/Sink=false`, `si=141/45170`,
  `discover=Err`（§10 失敗状態、緑復帰待ち）。
- 実施: API #1/#2 の 2 回のみ。0x6004・キー・電源/reboot は行っていない。
