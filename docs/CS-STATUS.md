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
