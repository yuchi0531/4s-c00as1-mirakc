# BS4K と HDMI 無し運用の制約

## 結論

4S-C00AS1 の BS4K を mirakc から取り出すには、本体の **MMT_DEMUX が作る
ES リング**（video 20 MiB / audio 2 MiB）と、XCode6 の **REMUX** が必要です。
この 2 つは本体が実際に 4K サービスをデコードしている間だけ存在します。

本体は HDMI 出力先（TV / EDID を持つダミープラグ）を検出しないと
デコードパイプライン（`MIXER` / `Sink` スレッド）を開始しません。
LED が緑（または青）でも、HDMI 無しでは `discover()` が
`no live 4K ES pair` を返します。

## 実測（2026-11-13）

- HDMI 無し・LED 緑: `MIXER`/`buf_buf`/`Sink` スレッド 0、ES リング無し。
- `Tune = 0` / `3` / `7` の ADSB 選局メッセージを送っても、HDMI 無しでは
  デコードは開始されない。`Tune=7` は SI をクリアしたまま停止した。
- 2K の FED 選局（`BS15_0`）は HDMI 無しでも動作し、MULTI2 復号まで成功。
- 2K 選局で LNB 給電が有効になった直後に ADBS FED を直接選局すると、
  復調器は lock（`0x48`）まで到達する。しかし TLV パーサ（XCode6 device 0x24）
  は本体が構成していないため、TLV リングにはデータが流れない。

## 取れる選択肢

1. **HDMI ダミープラグ（EDID エミュレータ）を付ける**
   画面は出ません。本体は HDMI シンクがあると判断してデコードを開始します。
   この状態で `t4s-tuner --type BS4K` が REMUX 経路で動きます。
2. **一度だけ TV に接続して 4K を選局し、緑状態にする**
   そのまま HDMI を外しても、デコードが続く間は取得できます。
   本体がスタンバイに戻ると ES リングは消えます。
3. **TLV リングを直接読む（未実装・要検証）**
   本体の TLV リングは host アパーチャ外（`0x1df...`）にあり、`xc_map` では
   読めません。`connect_input` はリングの `m_max_reader` が埋まっているため
   `-1035` で失敗します。製品の MMT リーダーを一時的に外し、自前の
   TS_RECODER を接続する案は、製品の EPG 経路を壊すリスクがあります。

## 現在の実装

`t4s-tuner` の BS4K 経路は 1/2 を前提にしています。
ES リングが無い場合は次のエラーで終了します。

```
t4s-tuner: 4K selection not confirmed ... / no live 4K ES pair
```

mirakc 側では BS4K チャンネルの `tlv-decoder` に `cat -` を指定しています。
これは `t4s-tuner` が REMUX で MPEG-TS を出すためで、TLV のままではありません。

## Phase 1 静的解析の追記

`adsb` の `wrap_demuxInit`（0x4e308）が TLV demux(type 0x24) と
4 本の 4MiB リングを作り、MMT demux(type 0x25) が各リングを読む。
TLV リングの max_reader は 1 で製品が消費済み。外部タップは
type 0x24 の追加 `connect_output`（8 output ports）が正しい。
`TS_RECODER(0x13)` は TLV 経路では使わない。

duplicate が 0 バイトなのは、TLV demux に **選局入力/ロックが無い**ため。
`bAdsbInitialized`（abs のレポート値 0x02027184）は実マップ外で
書き込み不可。`abs` を止めても TLV は出ず、製品ウォッチドッグで
再起動した。HDMI シンク無しで TLV を出すには、`adsb` の初期化を
製品自身に完了させる（= HDMI/UI が必要）か、TLV demux の入力を
自前で構成する Phase 2 の実装が必要。


## Phase 1 追加試験: TSI からの TLV 直接取得

CS と同様に physical TSI0 を **130 バイト (ME_PACKET_130)** に設定し、
TS_RECODER(type 0x13) の出力を 130 バイト RING に接続する試験を行った。
`connect_output` は **os error 1025 (XCE_INVAL_PARAM)** で拒否された。
TS_RECODER は 188 バイト TS 専用で、TLV の 130 バイトリングは受け付けない。

4K の TLV は製品の type 0x24 TLV デマルチプレクサが作る 4 本の
4MiB リングに依存する。外部タップは type 0x24 への追加
`connect_output`（8 output ports）だが、TLV demux に選局入力/ロックが
無いため 0 バイト。TLV demux の入力構成（0xA31002A の再現）と
`bAdsbInitialized` の正しい実アドレスが Phase 2 の課題。

## Phase 2 実機試験（2026-11-13 深夜）

`adsb` の TLV トポロジ（type 0x24、`0xA31002A`、4MiB RING/MMTP(9) 4本、
type 0x25 MMT demux）を `docs/analysis/phase2-tlv.md` に確定。

実機では次を試した:

1. `0xcc=0x40`（既に TLV 出力ビット）を確認。
2. qid 0x36F へ `0x6004`（service 141）を直接送信。
3. type 0x24 に **RING/SYSTEM/MMTP(9), 4MiB, max_reader=1** の
   自前リングを `connect_output` し、3秒 read。

結果: `tlv_chain=0`、0 バイト。送信後の復調は `lock=0x48` だが
bank2 は **0x6080（CS8 の TSID）** のままで、141 へ選局されていなかった。
すなわち直接 `0x6004` は adsb の TUM/TSin 経路を動かしていない。

次に必要なのは `Sfd_TsdSetTsIn`（qid 0x52 msg 0x1e）の body 特定、
または `bAdsbInitialized` を製品自身に立てさせる初期化列。
`bAdsbInitialized` の実アドレスは `abs:0x02027184`（RW LOAD 内）で
再解析済みだが、直接 `0x6004` を送るだけでは 4K 選局/TLV 入力に
到達しなかった。

## Phase 2b 実機試験（0x6001/0x6002 初期化）

Phase 2b の推奨列 `0x6001(mode=0) → 0x6002 → 0x6004(141)` を
qid 0x36F へ送った。結果:

- `0x6001` / `0x6002` は送信成功。
- `0x6004` 後も復調 bank2 は `0x6080`（CS8）のままで、141 へ選局されない。
- さらに **XCode6 の TLV/MMT グラフが解体**され、graph は devices count=5、
  type 0x24/0x25 と buffer の handle が 0 になった。
- `tlvcap2` の `connect_output` は `os error 1022`。
- ソフトウェア reboot で既知状態に戻した（現在 mirakc pid 786、API 200）。

したがって `mode=0` と `0x6002` の組み合わせは、TUM を初期化するのではなく
4K グラフを破棄する方向に働いた。再試験する前に、E0 の read-only ゲート判定
（`bAdsbInitialized`, adsb `.bss 0x395f60` の 55 バイト、`0x395dd4+dev*0x168+4`）
と `0x6001` の mode 値、`0x6002` の送信元/応答条件を確定する必要がある。

## Phase 2b E0（reboot 後の read-only）

reboot 後:
- `abs` `bAdsbInitialized` = **1**
- `adsb` の state/op 表 `0x395f60` は **非ゼロ**（TUM 初期化済み）
- per-device block `0x395dd4 + dev*0x168`:
  - dev0: `ff 00 00 00 01 00 00 00 00 00 00 00 64 00 00 00 09 00 00 00 00 00 00 00`
  - dev1: `01 00 00 00 01 00 00 00 00 00 00 00 05 00 00 00 05 00 00 00 09 00 00 00`

つまり TUM は初期化済み。しかし **0x6004 だけ**を送ると bank2 は
`0x6080`（CS8）のまま。0x6004 は per-device block を更新せず、
既存 block を `0x5004` として再送するため、**チャンネル変更にならない**。

4K 選局には per-device block の更新（`0x6005`/UI 経路）か、
`fdc_chmConnectTsUnits` の ARC メッセージ `0x10100010`
（payload `{queue_id, handle, source, tsdNo, op=5}`）が必要。
どちらも body/queue の意味が未確定で、製品停止/ARC 解析の領域。
