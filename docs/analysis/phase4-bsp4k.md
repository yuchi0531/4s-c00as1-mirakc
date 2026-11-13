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
