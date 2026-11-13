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
