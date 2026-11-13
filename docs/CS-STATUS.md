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
