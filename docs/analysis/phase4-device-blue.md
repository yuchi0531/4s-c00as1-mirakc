# Phase 4 — 青ランプ（電源ON/HDMI接続）実機試験

対象: SHARP 4S-C00AS1 (ViXS XCode6, ARMv7, Linux 3.10.24)
日時: 2026-11-13 20:27 JST ブート（青ランプ）
制約: ストレージ削除なし。書込みは USB `/mnt/t4s` と Q_UIM/UIM メッセージのみ。
`t4s-remote` / `t4s-arc` は他エージェントのもので編集しない。

---

## 0. 結論（この時点）

| 項目 | 結果 |
|---|---|
| 青状態の AV/UI | abs に `T_HDMI`, `T_AVM`, `T_OUTD`, `TID_VDEC`, `TID_AUD`, `HevcFactory` が存在。MIXER/Sink/buf_buf は無し |
| 4K ES リング | **無し**。`remux4k::discover` は Err（20MiB VIDEO_ES + 2MiB AUDIO_ES が見つからない） |
| 製品 UI | 初期設定ウィザードから通常ホーム画面まで到達。キー注入で操作可能 |
| 4K チャンネルサーチ | **BS4K/110度CS4K は 0 件**（製品 UI が「見つかりませんでした」と表示） |
| ADBS 直接 + 自前 TLV (`tlvcap3`) | **0 バイト**。TLV bit は立てた (`cc 0x80→0x40`) がデータ無し |
| Q_UIM キー経路 | **msgId `0x12044` が正**（`0x2044` は受信側ディスパッチに一致せず捨てられる） |

---

## 1. read-only スナップショット

### 1.1 プロセス/スレッド（青ブート直後）

- `abs` PID 279: 281 threads。`abs.T_UIM` 476、`abs.T_FPM` 392、
  `abs.T_HDMI` 424、`abs.T_AVM` 423、`abs.T_OUTD` 420、`TID_VDEC` 523、
  `TID_AUD` 596。
- `adsb` PID 264: `T_ADSB_COMM` 286、`abs.T_ADSB_TUM` 287 ほか多数。
- `HevcFactory` PID 280（1 thread、待機）。
- `tuner` PID 263、`mirakc` は再マウント後に起動（PID 1613）。
- **`MIXER` / `Sink` / `buf_buf` スレッドは存在しない**（HDMI 接続だけでは生成されない）。

### 1.2 xcdrv グラフ（`t4s-probe graph`、変化なし）

```
devices count=11
  dev[6] handle=0x7 type=0x24 pipe=0x0   # 製品 TLV demux
  dev[7..10] handle=0x8..0xb type=0x25   # MMT_DEMUX 4 個
buffers 4 個（TLV 用 ring、20MiB/2MiB の ES ring は無し）
mmr6000=0x100 mmr6008=0x1000000 mmr600c=0x300 mmr61f0=0x10
```

### 1.3 SI / i2c

- `/tmp/adsb/AdsbSiMpt.dat` は常に 8193 バイトで中身が更新されない。
  `si::live_service` は `None`。
- `read_output_regs`（i2c）は製品 UI 稼働中に **2 秒で完了しない**ことが多い
  （`t4s-remote` の `/api/status` も `I2C read did not finish in 2s`）。

### 1.4 `tlvcap3`（青状態、ADBS FED 直接 + 自前 TLV）

```
quiesce 0x80->0x40
before lock=0x48 bank2=[64, 241, 0] cc=0x40
tlvcap3 dev=0xc total=0 first=[] tlv_chain=0
```

- `bank2=[0x40,0xF1,0x00]` は直前に 2K FED で合わせた TSID 16625 (0x40F1) の残り。
- ADBS tune (`advanced_bs(0xB072, 11_842_560)`) 後も bank2 は更新されず、
  TLV demux には 1 バイトも流れない。Phase 2/4-arc の「ARC がホストメッセージを
  消費しない」問題と一致。

---

## 2. 正しい UIM キー経路（重要な訂正）

### 2.1 msgId は `0x12044`、`0x2044` ではない

`abs` の受信ディスパッチ（`0x70dde2` 付近）は:

```
70dde2: movw r2, #0x2044
70dde6: movt r2, #0x1      ; r2 = 0x00012044
70ddea: cmp  r3, r2
```

FPM → UIM の送信側 (`0x513664`) も同様に `movw r1,#0x2044; movt r1,#0x1`
= **0x12044** を送っている。したがって `msgId 0x2044` のメッセージは
ディスパッチに一致せず捨てられる（`prev key` も更新されない）。

別系統 `0x115C` も実際は **0x1115C**（`0x708d5e` で `movt r2,#0x1`）。
`0x1115C` ハンドラ `0x715F04` はキー名ログ・ホワイトリスト・実処理を含む。

- `0x12044` → `0x715DD4`（正規化 → `0x743B34` フィルタ → 受理時 main loop が `0x7185CC`/`0x723218` を実行）
- `0x1115C` → `0x715F04`（上と同等以上のキー処理）

**実測**: `prev` key = abs VA `0x1D2AD48`（pagemap 経由 phys）を devmem で読む。

| 送信 | prev の値 |
|---|---|
| `msg 0x2044` key 0x13 | 0x00000000（**捨てられた**） |
| `msg 0x12044` key 0x13 | 0x00000013（受理） |
| `msg 0x12044` key 0x10 type 0 → type 2 | 0x00000010 → 0xFFFFFFEF |

`type=2` は `key = ~key` として渡るため、primer として prev を非 0 にできる。

### 2.2 実装

`rust/t4s-probe` の `uimkey` を修正:

- `uimkey <key> <type> [msgid] [x]`
- `msgid` 省略時 **0x12044**。`0x1115c` も指定可能。
- 第 5 引数があれば送信のみ（i2c 読み取りをスキップ）。
- 追加コマンド `osd <out.ppm> [phys] [step]`（`/dev/mem` mmap で OSD 面を PPM 保存）
- 追加コマンド `putfile <ip:port> <path>`（TCP でホストへ転送。`ftpput` は port 指定不可のため）

---

## 3. 製品 UI の初期設定を抜けるキー列（実測）

全キーは `msg 0x1115C`（または `0x12044`）で送信。同じキーを連続で送る場合は
`type=2` を挟む（prev フィルタ対策）。

起動直後は「チューナー 初期設定」画面:

1. `ENTER 0x015` … アンテナ接続確認の「次へ」
2. `RIGHT 0x019` → `ENTER 0x015` … 「初期設定を進める」
3. ネットワーク接続確認: `DOWN 0x017`（×2、間に release）→ `ENTER 0x015` … 「しない」
4. 確認画面: `ENTER 0x015`（直前に `type=2` で prev をリセット）
5. → ホーム画面（I-O DATA、上部に「NHK BS4K」表示）

ホーム画面のフォーカス:

- `UP` で黒い映像ウィンドウへ。`ENTER` でライブ視聴へ戻る。
- 映像ウィンドウで `RIGHT` → 右上「設定」アイコン。
- 設定タブは `RIGHT` で切替:
  1. かんたん設定
  2. 録画・再生設定
  3. 映像・音声設定
  4. 本体設定
  5. **放送視聴設定**（チャンネル設定 / アンテナ設定 / 4K放送視聴設定）

### 3.1 チャンネルサーチ実行

`放送視聴設定` → `チャンネル設定` → `チャンネルサーチ` →（確認）`LEFT` で「する」→ `ENTER`。

結果画面（実測、フル解像度 OSD を `osd` で取得して判読）:

> **「BS4K・110度CS4K放送のチャンネルが見つかりませんでした。**
> **・アンテナ線を確認して再度チャンネルサーチをしてください。**
> （アンテナ線を確認する場合は、「1階層前になっています」）
> ・アンテナ線に切れや抜けがないか、接続状態が悪いところをご確認ください。
> ・アンテナの向きが変わっている場合は、アンテナの向きを調整してください。
> ・接続に問題がないときは、アンテナの電源設定を確認してください。」

- 2K BS/CS のチャンネルリスト `/tmp/adsb/AdsbBsSlist.dat` / `AdsbCsSlist.dat` は
  20:51/20:52 に更新（2K 側は受信できている）。
- 4K サーチ後も Slist の mtime は更新されず、**4K は 0 件**。
- サーチ中の `Q_ADSB_TUM` (msqid 884763) は 21:19 頃に送受信あり。

---

## 4. 再起動前の異常（復旧が必要になった状態）

4K サーチ後、ホーム画面で「映像ウィンドウ → ENTER（ライブ視聴）」を送った直後から:

- `abs.T_UIM` (476) が **`do_msgsnd` でブロック**（受信ループ停止）。
- Q_UIM (98307) の `qnum` が 11 → 12 と増加し、`lrpid=476` のまま `rtime` が進まない。
- telnetd 応答が数十秒単位で遅延。load average 26〜27。
- `xcfb_event` が CPU を多く消費。
- これは UI 側のデッドロックと判断。ユーザー許可に基づき **reboot で復旧**する。

（`t4s-remote` の `/api/status` は HTTP で応答し、`qnum` と `last_key` を返す。）

---

## 5. 次に試すこと（reboot 後）

1. USB ext4 イメージ再マウント + mirakc 起動（`/media/usb/ondevice-up.sh`）。
2. 初期設定が出たら §3 のキー列でホームまで抜ける。
3. **アンテナ設定**（放送視聴設定 → アンテナ設定）でアンテナ電源の設定を確認。
4. 4K 選局:
   - ライブ視聴で `4K 0x13B` → `1,0,1` → `ENTER`（NHK BS4K 101）
   - `CH UP 0x00C` を 1 回ずつ、`si`/bank2 を確認
   - それでも駄目なら `0x6004` 直接（`t4s-probe send 141 0 0`）→ bank2/lock
5. 4K TSID/SI が得られたら `t4s-probe discover` → `t4s-tuner --type BS4K`、
   失敗時は `tlvcap3`。取得 bytes / `tlv_chain` / ffprobe を記録。
