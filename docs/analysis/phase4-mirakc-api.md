# Phase 4 — mirakc API 経由の BS4K 配信検証

対象: SHARP 4S-C00AS1（青ランプ、製品 UI が 4K を視聴中）
日時: 2026-11-13 22:11–22:50 JST
制約: ストレージ削除なし。power OFF 0x021 / 0x6001/2 / ptrace / abs 停止は不使用。

---

## 0. 結論

`/api/channels/BS4K/45170/stream?decode=1` は **HTTP 200 で 4K TS を配信できた**。
ffprobe は HEVC Main 10 3840x2160 / HLG / AAC-LATM 48 kHz 2ch。
`decode=0` も同様に成功（BS4K の decode-filter は `t4s-cas` 素通しのため中身は同じ）。

`45328` (BSP4K) の API 検証は、製品 UI を BSP4K に再選局できず
（「選局・再生に失敗しました」）、その後の検証中に **実機がネットワークから消失**したため未完了。
ただし 45328 自体の REMUX 取得は 21:45 に成功済み（31,725,752 bytes、
`docs/analysis/phase4-bsp4k.md`）。

---

## 1. 設定の確認

`/mnt/t4s/opt/t4s/config.yml`:

```yaml
- name: NHK-BS4K
  type: BS4K
  channel: '45328'
  services: [101]
- name: BS-NTV-4K
  type: BS4K
  channel: '45170'
  services: [141]
- name: BS-Asahi-4K
  type: BS4K
  channel: '45168'
  services: [151]
- name: BS-TBS-4K
  type: BS4K
  channel: '45329'
  services: [161]
- name: BS-Tokyo-4K
  type: BS4K
  channel: '45169'
  services: [171]
- name: BS-Fuji-4K
  type: BS4K
  channel: '45330'
  services: [181]

tuners:
  - command: >-
      /opt/t4s/bin/t4s-tuner
      --type {{{channel_type}}} --channel {{{channel}}} --duration {{{duration}}} -
    tlv-decoder: cat -
filters:
  decode-filter:
    command: >-
      /opt/t4s/bin/t4s-cas
      --type {{{channel_type}}} --channel {{{channel}}}
```

- BS4K は `t4s-tuner` が REMUX で MPEG-TS を出すため `tlv-decoder: cat -`。
- `t4s-cas` は BS4K では素通し（2K のみ MULTI2 復号）。

---

## 2. 実測（45170 / BS日テレ4K）

製品 SI が `service 141 / tlv_stream_id 45170`、`MIXER/Sink/buf_buf = true` の状態で実行。

### 2.1 decode=1

```
curl -sS --max-time 15 \
  'http://192.168.1.154:40772/api/channels/BS4K/45170/stream?decode=1' \
  -o bs4k-api-45170.ts
```

- **HTTP 200**, 46,137,344 bytes（15 秒でタイムアウトしたが受信は継続していた）
- 先頭 4 バイト `47 40 00 10`（MPEG-TS sync）
- ffprobe:
  - video: **hevc / Main 10 / 3840x2160 / arib-std-b67 (HLG) / bt2020nc**
  - audio: **aac_latm / LC / 48 kHz / 2ch**
  - duration 12.815 s / bit_rate 28.80 Mbps

再試行（22:49）:

- **HTTP 200**, **53,837,824 bytes**
- duration 14.915 s / bit_rate 28.88 Mbps
- 同じく HEVC Main10 4K HLG + AAC-LATM 2ch

mirakc log:

```
13:48:54 INFO mirakc_core::tuner: Streaming started channel=BS4K/45170 ... agent="curl/8.18.0"
```

### 2.2 decode=0

- 1 回目は tuner 割当の過渡状態で `Connection timed out`（0 bytes）。
- 再試行で **HTTP 200**, **16,830,136 bytes**、duration 6.827 s、
  HEVC Main10 4K + AAC-LATM 48 kHz 2ch。
- BS4K の decode-filter は `t4s-cas` 素通しなので、decode=1 と同じ TS。

---

## 3. 45328 (BSP4K) の状況

- 製品 UI の `4K 0x13B → 1 → 0 → 1 → ENTER` マクロは
  **「選局・再生に失敗しました。」** となり、MIXER/Sink が false に落ちた。
- その後 `t4s-probe send 141 0 0`（0x6004 直接選局）で **141 の 4K 視聴状態へ復帰成功**。
  - `si=Some((141, 45170))`
  - `discover=Ok((1, 2, 8))`（video_readers=1, audio_readers=2）
  - `/api/status` の MIXER/Sink/buf_buf が true に戻った。
- `t4s-probe send 101 0 0` は service 101 → TSID `0xB110` (45328) を送る実装
  （`rust/t4s-core/src/channels.rs` の `ADVBS` 表）。
  45328 へ切り替えた後の API 検証は、直後に実機がネットワークから消失したため未実施。

---

## 4. 復旧時の OSD 読み取り（重要な知見）

再起動後、`t4s-probe osd` の既定アドレス `0x3C884000` は **全画素 alpha=0（透明）**で、
実際の UI は別サーフェスに描かれていた。

- 再起動後の DirectFB は pool offset が変わり、UI 面は **phys `0x3A390000`**。
- `0x3A390000` を読むと「チューナー 番組表 [BS4K]」（BS日テレ4K/BS朝日4K/…の番組表）が
  表示されていた（時刻ヘッダが 10:43→10:45 と更新＝UI は生存）。
- `0x3C884000` は透明オーバーレイ、`0x3F814000` は背景（グレー）。
- 再起動後はサーフェスを再探索する必要がある（fusion の pool offset を確認）。

---

## 5. 障害と未完了事項

- 22:50 頃、`decode=0` の再試行直後に **40772 が connection refused**、
  続いて `192.168.1.154` が **ping 不通 / no route to host** となった。
  デバイスはハングまたは電源断の可能性があり、遠隔からの復旧は不可能。
  （ユーザーによる電源再投入が必要）
- そのため以下は未実施:
  - `/api/channels/BS4K/45328/stream?decode=1`
  - 45328 選局後の `si` / `discover` / mirakc log
- 既に成功済みの 45328 取得（REMUX、31,725,752 bytes）は
  `docs/analysis/phase4-bsp4k.md` §4 を参照。

---

## 6. 中止（2026-11-14 00:22 JST、ユーザー指示）

4K 局の連続選局で製品 UI が「選局・再生に失敗しました」の失敗状態になり、
MIXER/Sink=false のまま復帰不能になったため、**ユーザー指示により検証を即時中止**した。
実機はこれから電源再投入される。以後、telnet / 0x6004 / API ストリーム取得は行っていない。

### 6.1 中止時点までの追記

- 45170 の再確認（00:10 JST、decode=1）:
  - **HTTP 200**, **29,786,112 bytes**, 8.334 s, 28.59 Mbps
  - ffprobe: HEVC Main 10 3840x2160 / arib-std-b67 (HLG) / AAC-LATM 48 kHz 2ch
- 45328 (BSP4K):
  - `t4s-probe send 101 0 0`（0x6004、service 101 → TSID 45328）を 1 回実行。
  - 製品 SI は **141 / 45170 のまま**、`discover` は Err（ES ring なし）。
  - 4K 局の連続選局で UI が失敗状態となり、**`/api/channels/BS4K/45328/stream` は未実施**。
- 45168 / 45329 / 45169 / 45330: **未実施**。
- 中止理由: 連続選局で製品 UI が復帰不能になり、ユーザーが電源再投入を決定。

### 6.2 状態

- 45328 以降の mirakc API 検証は **未検証（電源断で中止）**。
- 45170 の API 結果（§2）のみ有効。
- 電源復帰後は、ユーザーの許可があるまで API ストリーム取得を行わない。
  まず `/api/status` で `MIXER/Sink=true`, `SI=141/45170` を確認するだけに留める。

---

## 7. 次に必要なこと

1. 実機を電源再投入し、`ondevice-up.sh` で mirakc を起動。
2. 製品を 4K 視聴状態へ戻す（UI が不安定なら `t4s-probe send 141 0 0` が有効）。
3. `t4s-probe send 101 0 0` で BSP4K (45328) に切り替え、
   `si=Some((101,45328))` を確認してから
   `/api/channels/BS4K/45328/stream?decode=1` を検証。
4. 高負荷時の mirakc/デバイスクラッシュの原因切り分け
   （API ストリーム中の load average 30+、i2c/xcdrv 競合の可能性）。
