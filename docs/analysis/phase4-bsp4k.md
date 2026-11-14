# Phase 4 — 青状態で BSP4K (NHK BS4K) の TS 取得と 4K 全局検証

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
