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
