# Phase 2b — BS4K TLV 入力の TSin routing（静的解析）

対象: SHARP 4S-C00AS1（ViXS XCode6, ARMv7）。
**実機アクセス・書込み・再起動は行っていない。ホスト上のバイナリ（`.tmp/oem/abs`, `.tmp/oem/adsb`, `.tmp/oem/tuner`）のみ。コード変更なし（doc のみ）。**

> REA の `abs:0x1a63a8` は provider_timeout（330 s）で失敗したため、abs 側は
> `llvm-objdump`（`.tmp/abs.dis`）と r2 の関数境界・文字列 xref を主根拠にした。
> adsb 側は `.tmp/adsb.dis` + r2 + GOT/リテラル解決で確定した。

根拠タグ: `[D]` llvm-objdump/r2 逆アセンブリ、`[GOT]` GOT スロット実値、
`[STR]` 文字列、`[R]` 関数境界/呼び出し関係、`[INF]` 推論、`[未]` 未確定。

---

## 0. 結論サマリ

1. **直接 `0x6004` が bank2 を変えない主因は、abs の `bAdsbInitialized` ではなく
   adsb 側の 2 段の「初期化ゲート」である。**
   - `vAdsbCommMain`（adsb:0xf8c9）は起動直後、`eSysReceiveMessage(0x36F)` の
     **初期化ループ**にいる。ここでは `0x5014` と `0x6001` 以外の msgId を
     **受信して捨てる**。`0x5014` を受けるまで `0x6004` は `0x117c4` に届かない。
   - `vAdsbTumMain`（adsb:0x13df4）も起動直後、`eSysReceiveMessage(0x371)` の
     **初期化ループ**にいる。ここでは `0x5053` / `0x41010001` / `0x5001` 以外を
     **受信して捨てる**。`0x5053` を受けるまで `0x5004` は処理されない。
   - `0x5014` は TUM が `0x5053` を受けたときに qid `0x36F` へ返す。
     したがって **abs が `0x6001` を送り、TUM が `0x5001` を処理し、SFM が
     `0x5053` を返すまで、直接 `0x6004` は無効**。
   - さらに TUM の `0x5004` は **状態/操作コード表**（adsb `.bss` `0x395f60`、
     `state*5 + op` の 55 バイト）で許可判定される。この表は BSS ゼロ初期値なので、
     初期化前は全エントリ 0 = 不許可。`0x5004` は
     `[ADSB-TUM] tummain.c invalid state/ope` で捨てられる。

2. **`0x6004` の 10 word body は TUM の選局パラメータそのものではない。**
   `0x117c4` は body を解析してログし、`0x10860` → `0x1f1ac` で
   `eSysExSendMessage(0x371, 0x5004, body=[0x395dd4+dev*0x168+4], 0x18, ...)`
   を送る。ここで送る body は **adsb の per-device ブロック**であり、
   `0x6004` の body ではない。`0x6004` のチャンネル値はログと
   ex 引数にしか使われない（少なくとも `0x10860` は per-device ブロックを更新しない）。
   TUM の `0x5004` 実処理（`0x17fa0`）は、この 24 バイトブロックの
   `+0x14`（Tune）で分岐し、`SetTSPort` を呼ぶ。

3. **4K の TSin routing は `Sfd_TsdSetTsIn` ではなく `SetTSPort`（adsb:0x186e4）である。**
   `Sfd_TsdSetTsIn`（abs:0x1a63a8）は qid `0x52`（SFD タスク）へ msg `0x1e` を送る
   **SFD/TSD プレイバック系**のコマンドで、呼び出し元は
   `0x5d05cc`（ES テーブル/プレイバック設定）と `0x347832`/`0x3478ea`（TPly 系の (1,1,1)）だけ。
   4K TUM 経路からは呼ばれない。
   `SetTSPort` は `fdc_chm*` 系を順に呼び、`fdc_chmConnectTsUnits` で
   TSD 入力ソースを切り替える。

4. **製品を止めずに TSin を設定する最小列は「TUM を初期化してから 0x6004」**である。
   TSin 設定用の独立メッセージは存在しない。`0x6004` → `0x5004` →
   `0x17fa0` → `0x1855c` → `SetTSPort` の副作用として設定される。
   既に TUM が初期化済みなら `0x6004` だけでよいが、その場合も
   `0x5004` の 24 バイトブロックが現在の選局内容と一致している必要がある。

5. **`0xcc=0x40` と TLV ring の存在だけでは bank2 は変わらない。**
   bank2 が `0x6080` のままなら、TUM は選局要求を処理していない
   （ゲート未通過 / state-op 不許可 / ブロック不一致）。復調側の TLV 出力設定
   （`DMD_set_tsout`）は TUM が FED 選局を完了した後の話である。

---

## 1. qid / msqid の前提

実 msqid は起動ごとに変わる。`/tmp/bdh_out_msgh.lst`（`<qid>-<msqid>` 形式）で解決する。
本 doc の qid は **product 内 qid**。

| qid | dec | 所有者 / 受信関数 | 根拠 |
|---:|---:|---|---|
| `0x36F` | 879 | adsb COMM。`vAdsbCommMain`（`0xf8c9`）が `eSysReceiveMessage(0x36F)` | [D] `0xf8e8`, `0xf984` |
| `0x370` | 880 | abs 応答（`0x6002` が `eSysReceiveMessage(0x370)` で待つ） | [D] abs `0x457804` |
| `0x371` | 881 | adsb TUM。`vAdsbTumMain`（`0x13df4`）が `eSysReceiveMessage(0x371)` | [D] `0x13e22`, `0x13e98` |
| `0x52` | 82 | abs SFD。`vSfdMainTask`（`0x1aa5e0`）が `eSysReceiveMessage(0x52)` | [D] `0x1aa5f2` |
| `0x1C7` | 455 | tuner FEM（既存 `tuner.rs`） | 既存資料 |

- 旧メモの「0x36F → msqid 1605681、receiver adsb thread (PID 286)」は、
  本 doc の `vAdsbCommMain` と整合する [INF]。
- `0x371` と `0x52` の実 msqid は同じファイルから読む。**ハードコード禁止**。

---

## 2. `0x6004` の adsb 内処理（確定）

### 2.1 `vAdsbCommMain` の初期化ゲート（最重要）

`adsb:0xf8c9 vAdsbCommMain` [D]:

```
0xf8e0  eSysReceiveMessage(0x36F, &msgId, &body, -1)
0xf8f8  if (msgId == 0x5014) {
0xf914      vAdsbComm... 3 関数呼び出し
0xf930      flag = 1;                 // ← 初期化ループ脱出
0xf934      goto 0xf938
        } else if (msgId == 0x6001) {
0xf90a      FUN_0x11754(body);        // → 0x5001（TUM init）
0xf912      goto 0xf938              // ← flag は立てない
        }
0xf938  if (flag == 0) goto 0xf8e0   // それ以外の msgId は捨てる
0xf97e  eSysExReceiveMessage(0x371... ではなく 0x36F, ...) // 本ループ
```

- 本ループに入るまで `0x6004` は **受信されるが捨てられる**。
- `0x5014` を送るのは TUM の `0x5053` ハンドラ（`0x13e62`–`0x13e7c`）。
- `0x6001` は「TUM 初期化要求」で、`0x11754` → `0x107a8` → `0x1f12c` が
  `eSysSendMessage(0x371, 0x5001, body, ...)` を送る [D] `0x1f12c`–`0x1f152`。
- `0x5014` の 3 関数（`0x11ac4`, `0x13430`, `0x12900`）のうち
  `0x11ac4` は `0x10be0` を呼び、初期化完了報告系に進む [R]。
  abs の `bAdsbInitialized=0` は、この `0x5014` が未処理であることの
  **下流の症状**であり、`0x6004` を捨てている直接のゲートではない。

### 2.2 `vAdsbTumMain` の初期化ゲート

`adsb:0x13df4 vAdsbTumMain` [D]:

```
0x13e1a  eSysReceiveMessage(0x371, &msgId, &body, -1)
0x13e34  if (msgId == 0x5053) {
0x13e62      eSysSendMessage(0x36F, 0x5014, 1, len=-1)
0x13e7a      flag = 1;              // ← 初期化ループ脱出
        } else if (msgId == 0x41010001) { ... }
        else if (msgId == 0x5001) { FUN_0x15c6c(body); } // TUM init
0x13e80  if (flag == 0) goto 0x13e1a  // 0x5004 は捨てる
0x13e9c  eSysExReceiveMessage(0x371, ...) // 本ループ
```

- `0x5053` は `vAdsbSfmMain`（`0x20405`）が FDC/CHM の状態 3 を検出したときに
  qid `0x371` へ送る（`adsb:0x20508`–`0x20514`）[D]。
- したがって **`0x5001` → TUM init → SFM が `0x5053` → TUM が `0x5014`**
  の順が成立しない限り、`0x5004` は TUM に届いても捨てられる。

### 2.3 `0x6004` のディスパッチと解析

`vAdsbCommMain` 本ループ [D]:

| msgId | 分岐 | ハンドラ |
|---:|---|---|
| `0x6001` | `0x1010c` | `0x11754` → `0x5001` |
| `0x6002` | `0x10122` | `0x11778` → `0x5002` |
| `0x6004` | `0x1012c` | **`0x117c4`** |
| `0x6005` | `0x10136` | `0x11790` → `0x5003` |
| `0x6006` | `0x10140` | `0x11850` → `0x5005` |
| `0x6007` | `0x1014a` | `0x11890` → `0x5006` |
| `0x6008` | `0x10154` | `0x118c0` → `0x5007` |
| `0x6009` | `0x1015e` | `0x118f4` → `0x5008` |

`0x117c4` の body 解析（0x28 bytes）[D] `0x117d8`–`0x11832`:

| body offset | 型 | 用途（adsb 内） |
|---:|---|---|
| `+0x00` | u32 | **devIdx**（`0x1f1ac` の per-device 添字） |
| `+0x04` | u8 | OID |
| `+0x08` | u32 | Dev（製品は 9） |
| `+0x0c` | u32 | Tune |
| `+0x10` | u16 | NID（製品は `0x0b`） |
| `+0x14` | u16 | SID |
| `+0x18` | u32 | RF kHz |
| `+0x1c` | u16 | TLV stream id (TSID) |
| `+0x20` | u32 | NotDecode |
| `+0x24` | u8 | BID |

> 既存 `tuner.rs::ChangeChannelRequest::body()` の 10 word と一致する。
> 注意: `body[0]` は「0」ではなく **devIdx**。製品 UI は 0 を入れるが、
> adsb はこれを per-device 配列の添字に使う。

`0x10860` は上記をログし、`0x1f1ac(devIdx, &parsed)` を呼ぶ [D] `0x108d0`–`0x108d4`。

`0x1f1ac` [D] `0x1f1ac`–`0x1f1f4`:

```
body = *(u32 *)(0x395dd4 + devIdx*0x168 + 4)
eSysExSendMessage(0x371, 0x5004, body, ex=parsed, len=0x18, timeout=-1)
```

- GOT スロット `0x119408` → グローバル `0x395dd4`（adsb `.bss`）[GOT]。
- `body` は **per-device ブロックの +4 が指す 24 バイト**。
  `0x6004` の 10 word はここには入らない。`0x10860` は per-device ブロックを
  一切更新しない [D] `0x10860`–`0x108d4`。

### 2.4 TUM `0x5004` の状態/操作コードゲート（最重要）

`vAdsbTumMain` の本ループで `0x5004` は明示ケースに無く、**default 分岐**に入る
[D] `0x13e86`–`0x1447c`:

```
0x13d90(payload, &devIdx)          // payload から per-device を引く
base = 0x395dd4 + devIdx*0x168
0x14398  if (base->state_0x28 > 0xa) goto invalid
0x143a2  if (base->op_0x2c   > 0x4) goto invalid
0x143b0  table = *(u32 *)0x395f60  // 55 bytes, state*5 + op
if (table[state*5 + op] == 0) goto invalid
else dispatch by op:
   op 0 → 0x17fa0 ; op 1 → 0x18f48 ; op 2 → 0x19358 ;
   op 3 → 0x19de4 ; op 4 → 0x1bbb4
invalid:
  print "[ADSB-TUM] tummain.c invalid state = %d"
       / "[ADSB-TUM] tummain.c invalid ope = %d, state = %d"
```

- 表 `0x395f60` は `.bss`。ファイル上はゼロ。**初期化前は全エントリ 0 = 不許可** [確定]。
- 表を書くのは `fcn.0001dc98` と `fcn.0001dfc4`（r2 `axt @ 0x395f60`）[D]。
  これらは TUM の `0x5012` / `0x4105...` 系イベント処理から呼ばれる
  （`0x1413c`, `0x14342`）[D]。
- したがって直接 `0x6004` を送っても、TUM が初期化され state/op 表が
  埋まっていなければ `0x5004` は **無言で捨てられる**（ログだけ出る）。

### 2.5 `0x5004` の実処理（`0x17fa0`）

`0x17fa0` は `vAdsbTumMain` default の op 0 から呼ばれる [D] `0x143f8`。
`0x5004` ケース [D] `0x1802e`–`0x18038` → `0x18142`:

```
copy 24 bytes from body to base+0x10
log "msgAdsbCommRequestTumChangeChannel[%d][%d][%d]"
    (devIdx, body+4, body+0x14)
log "%d, %x, %d, %ld, %x %d %x" (body+0x10, +0x1a, +0x1c, +0x20, +0x1e, +0x24, +0x19)
switch (*(u32 *)(body+0x14)) {   // Tune
  case 0..7: base->state_0x28 = 2/3/1/7...;
             FUN_0x1855c(devIdx, body, extra)
}
```

- `0x1855c` が最後に `SetTSPort(devIdx)` を呼ぶ [D] `0x186ca`。
- つまり **TSin routing は 0x5004 の Tune 分岐の副作用**。独立した TSin メッセージは無い。

### 2.6 `0x6004` が効かない理由の確定度

| # | 理由 | 確定度 | 根拠 |
|---:|---|---|---|
| 1 | adsb COMM 初期化ループが `0x6004` を捨てる（`0x5014` 未受信） | **確定**（コード上） | `0xf8f8`–`0xf93c` |
| 2 | TUM 初期化ループが `0x5004` を捨てる（`0x5053` 未受信） | **確定**（コード上） | `0x13e34`–`0x13e84` |
| 3 | state/op 表 `0x395f60` が BSS ゼロで不許可 | **確定**（表がゼロなら） | `0x14398`–`0x143c2` |
| 4 | `0x5004` の body が per-device ブロックで、`0x6004` の値ではない | **確定**（`0x10860` は更新しない） | `0x1f1ac`, `0x10860` |
| 5 | per-device ブロックが古い/未設定 | **有力**（初期化前はゼロ） | `0x395dd4`, `0x395f3c` |
| 6 | ex 引数（`0x1f1ac` の `arg1`）を TUM がどう扱うか | **未確定**（IPC ex ABI） | §10 |

> 実機で bank2 が `0x6080` のままだった事実は、1–3 のいずれか（または複数）が
> 成立していることと整合する。特に `bAdsbInitialized=0` は 1 が成立している
> 強い傍証。

---

## 3. `Sfd_TsdSetTsIn`（abs:0x1a63a8）の完全形

### 3.1 シグネチャと body

`abs:0x1a63a8` [D] `0x1a63a8`–`0x1a63fa`:

```
bool Sfd_TsdSetTsIn(u32 a, u32 b, u32 c)
{
    u8 body[0x1c];
    *(u32 *)(body+0x00) = a;
    *(u32 *)(body+0x04) = b;
    *(u32 *)(body+0x08) = c;
    // body+0x0c .. +0x1b は未初期化（この関数は書かない）
    rc = eSysSendMessage(0x52, 0x1e, body, 0x1c, -1);
    if (rc != 1) {
        eDbgPrintf(2, "Sfd_TsdSetTsIn : can not send messace");
        return 0;
    }
    return 1;
}
```

- **送信長は 0x1c = 28 バイト**。ただし初期化されるのは先頭 12 バイトだけ。
  残り 16 バイトはスタックの未初期化値 [確定]。
- 文字列 `Sfd_TsdSetTsIn : can not send messace` は abs VA `0xd1af20` [STR]。
- qid `0x52` / msg `0x1e` / len `0x1c` は §1 のとおり。

### 3.2 受信側（qid 0x52 → msg 0x1e）

`abs:0x1aa5e0 vSfdMainTask` [D]:

```
0x1aa5f2  eSysReceiveMessage(0x52, &msgId, &body, -1)
0x1aa60c  jump table[msgId]  (msgId <= 0x67)
0x1aa61c  table base（4 バイト×0x68 エントリ）
msg 0x1e → 0x1aa945 → 0x1a7f70(body, param)
```

`0x1a7f70` [D] `0x1a7f70`–`0x1a7fb4`:

```
0x1a7f80  check(body)                       // 0x500434
0x1a7f92  a = body[0]
0x1a7f96  b = body[1]
0x1a7f9a  c = body[2]
0x1a7fa2  0x1aed08(a, b, c)
```

`0x1aed08` [D] `0x1aed08`–`0x1af790`:

```
if (c > 7) return 2;          // body[2]
if (b > 7) return 2;          // body[1]
if (a > 0x20) return 2;       // body[0]
print "[TSD]SetTsIn[%d:%d]"  (c, a)   // 文字列 VA 0xd14524
... 32 個の分岐で TSD 入力ソースへ変換 ...
```

- **受信側が読むのは body[0..2] の 3 word だけ** [確定]。
  したがって `0x1c` バイトのうち末尾 16 バイトの未初期化値は
  受信側では使われない（製品の未初期化送信は実害なし）。
- ログの引数順は `(body[2], body[0])`。`[%d:%d]` の意味から
  `body[2]=tsdNo (0..7)`, `body[0]=tsInNo (0..0x20)`,
  `body[1]=補助ソース/ポート (0..7)` と推定 [INF]。
  正確なフィールド名は §10。

### 3.3 呼び出し元

| 呼び出し元 | 引数 | 用途 | 根拠 |
|---|---|---|---|
| `0x1aec9a`（wrapper `0x1aec88`） | 3 引数をそのまま | `0x5d05cc` から。ES テーブル/プレイバック設定 | [D] `0x1aec9a`, `0x5d05cc` |
| `0x347832` | `(1,1,1)` | TPly/Ifo 系の固定設定 | [D] `0x34782c`–`0x347832` |
| `0x3478ea` | `(1,1,1)` | 別の TPly 系分岐 | [D] `0x3478e4`–`0x3478ea` |

- **4K TLV / adsb TUM 経路からの呼び出しは無い** [確定]（grep 全 xref）。
- `0x5d05cc` は `ePlmGetEsTableAssignWithPmtExp` 系の ES テーブル処理で、
  引数は `0x52a4b4(1/2)` の戻り値と `ctx+8`。プレイバック/2K の TSD 設定。
- 結論: `Sfd_TsdSetTsIn` は **SFD/TSD プレイバック系**。4K TLV の
  `SetTSPort` とは別レイヤ。4K 用にこれを送っても TUM の TLV 入力選択は変わらない
  可能性が高い [有力]。

---

## 4. `SetTSPort`（adsb:0x186e4）と fdc_chm 列

### 4.1 シグネチャ

`adsb:0x186e4 SetTSPort(u8 devIdx)` [D] `0x186e4`–`0x189ba`:

- 引数は 1 個（`devIdx`、0..2 程度）。
- ログ `[FSDTV] DBG SetTSPort[%d]`（VA `0xd9b0c`）[STR]。
- `base = 0x395dd4 + devIdx*0x168`（GOT `0x119408`→`0x395dd4`）[GOT]/[D]。
  `base+0x8` は fdc ハンドル、`base+0x10` は eTsInNo、
  `base+0xc` は tsdNo/port、`base+0x14` は状態。

### 4.2 実行順（確定）

| # | アドレス | 呼び出し | 意味（文字列から確定） |
|---:|---|---|---|
| 1 | `0x18716` | `if (base->0x140 == 1) 0x7a2f8(base->0x8)` | `fdc_chmCancelSelectChannel`（前回選局の取消） |
| 2 | `0x1874c` | `base->0x140 = 0` | 取消完了 |
| 3 | `0x18754` | `if (g_tune_mode != 0xff) base->0xc = g_tune_mode` | tsdNo/port 更新 |
| 4 | `0x1878c` | `0x7a484(base->0x8)` | `fdc_chmDisConnectTsUnits`（内部で Cancel も呼ぶ） |
| 5 | `0x187bc` | `src = 0x18f14(base->0x10)` | eTsInNo → FDC source（10→2, 11→3, else 1） |
| 6 | `0x187ce` | `0x79838(base->0x8, src, base->0xc, 5)` | **`fdc_chmConnectTsUnits`**（TSin routing 本体） |
| 7 | `0x18800` | `0x7fa98(base->0x8, g_ptr)` | `fdc_chmSetDefaultAudioSetting` |
| 8 | `0x18836` | `if (base->0x14 <= 3 && base->0x24 == 1) 0x7b8dc(base->0x8,3,g,0)` | `fdc_chmStopDecode` |
| 9 | `0x18882` | `else if (base->0x14 <= 3 && base->0x144 == 0) 0x7b734(...,3,g,0)` | `fdc_chmStartDecode` |
| 10 | `0x188c2` | `0x794e0(base->0x8,0,0)` | `fdc_chmSetParentalrate` 系のクリア |

関数名の根拠（文字列）[STR]:

| 関数 | 文字列 | VA |
|---|---|---|
| `0x7a2f8` | `fdc_chmCancelSelectChannel` | `0xef13c` |
| `0x7a484` | `fdc_chmDisConnectTsUnits` | `0xef158` |
| `0x79838` | `fdc_chmConnectTsUnits` | `0xef078` |
| `0x7fa98` | `fdc_chmSetDefaultAudioSetting` | `0xef5a0` |
| `0x7b734` | `fdc_chmStartDecode` | `0xef240` |
| `0x7b8dc` | `fdc_chmStopDecode` | `0xef254` |
| `0x794e0` | `fdc_chmSetParentalrate` 系 | `0xef048` |

### 4.3 `0x7a2f8` と `0x79838` の引数

`0x7a2f8`（`fdc_chmCancelSelectChannel`）[D]:

- 引数 1 個: fdc ハンドル（`base+0x8`）。
- FDC キュー `0x10101001` を検索/作成し、semaphore `0x20181002` を取る。
- メッセージ `0x10100005`、len 0x10、payload `{handle?, ...}` を送り、
  `0x20010005` 応答を待つ。
- queue/sem/メッセージ ID は ARC 側の意味が未取得。ホスト取得物には
  `0x10100005` の名前は無い [未]。

`0x79838`（`fdc_chmConnectTsUnits`）[D] `0x79838`–`0x799d2`:

- 引数 4 個: `(handle, source, tsdNo, op)`。
- 呼び出し例（SetTSPort）: `(handle, 0x18f14(eTsInNo), base+0xc, 5)`。
- メッセージ `0x10100010`、len 0x20、payload:
  ```
  { u32 msg=0x10100010, u16 0, u16 0x18,
    u32 queue_id, u32 handle, u32 source, u32 tsdNo, u32 op }
  ```
- `op=5` が「connect/start」を意味すると推定 [INF]。`0x18f14` の変換は:
  `10 → 2`, `11 → 3`, その他 → `1`（[D] `0x18f14`–`0x18f44`）。

`0x7a484`（`fdc_chmDisConnectTsUnits`）[D]:

- 先に `0x7a2f8(handle)` を呼び、その後 `0x10100010` を
  payload ゼロ（source=0, tsdNo=0, op=0）で送る。
- SetTSPort では「切断 → 接続」の順を保証する。

### 4.4 `SetTSPort` の呼び出し元

- `0x1855c`（TUM tune-sub helper）の末尾 `0x186ca` からのみ [D]。
- `0x1855c` は `0x17fa0` の `0x5004` ハンドラから、Tune 値に応じて
  `0x181e6` / `0x18200` / `0x1821a` / `0x18234` で呼ばれる [D]。
- **`0x6004` を送らなければ `SetTSPort` は走らない**。
  逆に、`0x5004` が TUM で処理されれば `SetTSPort` が走る。

---

## 5. TUM 24 バイト tune body のレイアウト

`0x17fa0` が `0x5004` で受け取る body（`0x5003` が書く 24 バイト）[D]:

| offset | 型 | 用途（ログ/分岐から） |
|---:|---|---|
| `+0x00` | u32 | **eTsInNo**（`SetTSPort` が `0x18f14` で source に変換） |
| `+0x04` | u32 | 第 2 引数（`msg...ChangeChannel[%d][...]` の 1 個目） |
| `+0x08` | u32 | 未ログ（コピーされる） |
| `+0x0c` | u32 | 未ログ（コピーされる） |
| `+0x10` | u32 | ログ `%d` |
| `+0x14` | u32 | **Tune**（`0x181a4` の switch 0..7） |
| `+0x18` | u32 | 未ログ（コピーされる） |
| `+0x19` | u8 | ログ `%x` |
| `+0x1a` | u16 | ログ `%x` |
| `+0x1c` | u16 | ログ `%d` |
| `+0x1e` | u16 | ログ `%x` |
| `+0x20` | u32 | ログ `%ld` |
| `+0x24` | u32 | ログ `%d` |

- `0x5003`（`0x16688`）は、受け取った 24 バイトを
  `0x395f3c + idx*?` の 0/4/8 と 0xc/0x10/0x14 に書く [D]。
- `0x5004` の Tune 値 0..7 は、`0x1855c` の `extra` 引数と
  `base->state_0x28`（2/3/1/7 等）を切り替える。
  製品の `0x6004` body の Tune（`+0x0c`）とは **別物** [確定]。

> 重要: `0x6004` の `+0x0c`（Tune）は `0x10860` のログにしか使われず、
> `0x5004` の Tune（24 バイト body の `+0x14`）には変換されない。
> したがって「0x6004 の Tune=0/3 を変えても bank2 が動かない」ことは、
> この構造と整合する。

---

## 6. 製品を止めずに TSin を設定する最小メッセージ列

### 6.1 推奨（製品互換・初期化込み）

TUM が未初期化（`bAdsbInitialized=0`、`0x6004` 無効）の場合:

| # | qid | msgId | body | len | 目的 |
|---:|---:|---:|---|---:|---|
| 1 | `0x36F` | `0x6001` | `{u32 mode}`（0=RECODER, 1=TUNER BOX） | 4 | adsb COMM の初期化分岐を通し、TUM へ `0x5001` を送る |
| 2 | — | — | — | — | 2–5 s 待つ。TUM が `0x5001` 処理 → SFM が `0x5053` → TUM が `0x5014` を `0x36F` へ返す |
| 3 | `0x36F` | `0x6002` | なし | 0 | SFM/CHM 初期化。abs 側は qid `0x370` で応答待ちするため、**別スレッド**で送る |
| 4 | `0x36F` | `0x6004` | 製品 10 word（§2.3） | `0x28` | TUM `0x5004` → `0x17fa0` → `SetTSPort` |

- 1 の body は abs `0x4a618` が `r0=0/1` を渡し、`0x4577b0` が
  `eSysSendMessage(0x36F,0x6001,body,size=-1)` する [D]。
  `eSysSendMessage` の第 3 引数が「値」か「ポインタ」かは ABI 未確定 [未]。
  実装側は **4 バイトの `{mode}` バッファ**として送るのが安全 [INF]。
- 2 の完了判定（read-only）:
  - `abs` の `bAdsbInitialized`（VA `0x02027184`）が 1 になる。
  - adsb `.bss` の state/op 表 `0x395f60` が非ゼロになる。
  - adsb `.bss` の per-device `0x395f3c + 0x28/0x2c` が妥当値になる。
- 3 は abs 側が `eSysReceiveMessage(0x370)` でブロックするため、
  呼び出し側もブロックする。タイムアウトを設け、失敗時は 4 を送らない。
- 4 の後に TSin routing が自動で走る。追加の TSin メッセージは不要。

### 6.2 TUM が既に初期化済みの場合

- `0x6004` のみ。ただし `0x5004` の 24 バイトブロックが現在の選局と
  一致している必要がある。
- `0x5004` が処理されたかは、bank2 TSID の変化と
  `SetTSPort` のログ `[FSDTV] DBG SetTSPort[%d]` で判定する。
- 変化しない場合は §6.3 の read-only 確認へ。

### 6.3 直接 TUM を叩く実験列（ゲート通過のみ・非推奨）

| # | qid | msgId | body | len | 効果 |
|---:|---:|---:|---|---:|---|
| 1 | `0x371` | `0x5053` | なし | 0 | TUM 初期化ループを抜け、`0x5014` を `0x36F` へ送る（COMM ゲートも開く） |
| 2 | `0x36F` | `0x6004` | 10 word | `0x28` | `0x117c4` → `0x5004` |

- これだけでは **state/op 表がゼロのまま**なので、`0x5004` は
  `invalid state/ope` で捨てられる可能性が高い [有力]。
- `0x5053` は TUM の「初期化完了」フラグを立てるだけで、
  `0x5001` が行うファイルシステム/CHM 初期化は行わない。
  **実選局には不足** [確定]（`0x15c6c` と `0x13e62` の処理内容の差）。

### 6.4 `Sfd_TsdSetTsIn` を 4K に使う場合（非推奨・未確定）

- 形式: `qid 0x52, msg 0x1e, body {tsInNo, sub, tsdNo}, len 0x1c`。
- 4K の `eTsInNo` 候補は **10 または 11**（`SetTSPort` の `0x18f14` が
  10→source2, 11→source3 にするため）[INF]。
- ただし呼び出し元がプレイバック系のみで、TUM の TLV 入力選択とは
  別レイヤ。**4K TLV には `SetTSPort` 経由を推奨**。

---

## 7. 4K 選局 → TSin → TLV ring 出力の最小手順

### 7.1 read-only スナップショット（書込みなし）

1. `/tmp/bdh_out_msgh.lst` から `0x36F`, `0x371`, `0x52`, `0x1C7` の msqid を解決。
2. `abs` の `bAdsbInitialized`（VA `0x02027184`）を 4 バイト read。
   0 なら §6.1 の初期化が必要。
3. adsb `.bss`（ELF EXEC、VA 固定）を read-only:
   - `0x395dd4 + dev*0x168 + 4`（per-device body ポインタ）
   - `0x395f3c + dev*0x168 + 0x00..0x28`（tune block / state / op）
   - `0x395f60` の 55 バイト（state×op 許可表）
4. 復調 register read（既存 `gr_i2c::read_output_regs`）:
   - bank0 `0x0b,0x0c,0x13,0x16,0x18,0xcc`
   - bank7 `0x0a,0x72,0x73,0x74,0x75,0x76,0x7b,0x7d,0x8d`
   - `0xcc` は `0x40`（TLV only）を期待。`0xc0` なら 2K の bit7 が残っている。
5. 4 本の製品 TLV ring の `sbuf_status_raw(handle)` を read（wpos の増加を見る）。
6. `tsi_status` / `mmr_read32(0x6000/8/c/61f0)` は読むだけ。

### 7.2 書込み（復元可能）

1. §6.1 の `0x6001` → wait → `0x6002` → `0x6004`。
2. 15 s、200 ms 間隔で read-only:
   - bank2 TSID が目的の 4K TSID になるか（`0x6080` のままは未選局）。
   - lock（既存 `fed` の `lock & 0x20` と bank2 TSID 一致）。
   - `0xcc == 0x40`、bank7 が TLV 状態。
   - 製品 ring の wpos 増加。
3. 選局後に自前 tap:
   - `SBUF_CREATE(0x32)`: `{3,1,9,1,0x1000,0x400000,1,3,0}`（RING/SYSTEM/MMTP 4 MiB, flags=3）
   - `connect_output(0x8002A)`: `{tlv_handle,0,ring,0}`
   - `set_ready(0x10035, ring)`
   - `create_reader(0x37)`, `get_read(0x3B)`, `xc_map`, `commit_read(0x3C)`
   - `tlv_score`（0x7F 同期）で判定。
4. cleanup: `disconnect(0x9002A)` → `destroy_reader` → `destroy_buffer(0x34)`。
   - 失敗時は ring を destroy しない（使用中なら残す）。

### 7.3 復元

- `0x6001/0x6002/0x6004` は製品の通常選局と同じメッセージ。復元操作は不要。
  （製品が現在の選局状態を保持する）
- `0xcc` を書き換えた場合だけ、事前 read 値（`0xc0` 等）を 1 バイトで戻す。
- 自前 ring は必ず disconnect/destroy。
- 最終復旧は電源再投入（フラッシュ書込みはない）。

### 7.4 判定フローチャート

```
bank2 が 0x6080 のまま
 ├─ bAdsbInitialized == 0
 │   └─ adsb COMM ゲート未通過の可能性大 → §6.1 の 0x6001/0x6002
 ├─ bAdsbInitialized == 1 だが state/op 表 0x395f60 がゼロ
 │   └─ TUM 未初期化 → 0x6004 を送っても 0x5004 は捨てられる
 ├─ state/op 表は非ゼロ、per-device +0x28/0x2c も妥当
 │   └─ 0x5004 は処理済み。24 バイト tune block の Tune/eTsInNo を確認
 └─ 0x5004 処理済みだが bank2 不変
     └─ SetTSPort の fdc メッセージ（0x10100010）が ARC で拒否/未実行
         → adsb ログ [FSDTV] SetTSPort と fdc エラーを確認
```

---

## 8. 製品停止が必要な場合・復旧・watchdog

| 操作 | 停止が必要か | 理由 | 復旧 |
|---|---|---|---|
| メッセージ注入（`0x6001/2/4`, `0x5053`, `0x52/0x1e`） | **不要** | 製品の msgq に送るだけ | 通常選局で上書き |
| adsb `.bss` の直接書換（state/op 表、tune block） | **必要**（ptrace/proc mem） | プロセスの状態を壊す | adsb 再起動 or 電源再投入 |
| `0x24` TLV デバイス再生成 / `0xA31002A` 再実行 | **必要** | 製品の TLV デバイス/管理バイトを壊す | adsb 再起動 → 電源再投入 |
| `0x61f0` crossbar 書換 | **必要** | 製品 abs は書かない | 事前 read 値を `0x53` で戻す |
| `0xcc` I2C 1 バイト | **不要**（既存 `select_tlv_output_exclusive`） | 揮発・1 バイト | 事前値を書き戻す |

- **`abs` を SIGSTOP しない**。実測で約 1 分後に watchdog 再起動（既存資料）。
- `adsb` を停止すると 0x24/0x25/ring の fd が閉じ、グラフが破棄される。
  abs が不整合を検知して再起動する可能性が高い。
- 本 Phase の推奨は **メッセージ注入のみ**。`.bss` 書換は行わない。
- 停止した場合の復旧順:
  1. `0xcc`/bank7 の事前 read 値を戻す（書いていた場合）。
  2. `0x61f0` を identity `0x10` に戻す（書いていた場合）。
  3. `abs` → `adsb` → `tuner` を通常起動（製品 init に任せる）。
  4. 不整合なら電源再投入。
- **禁止**: 製品 ring の destroy、`destroy_reader`（slot 喪失の既知問題）、
  TSI 130 B 再設定、`0xA31002A` の再実行。

---

## 9. 根拠アドレス一覧

### 9.1 abs

| ラベル | VA | file | 根拠 |
|---|---:|---:|---|
| `Sfd_TsdSetTsIn` | `0x1a63a8` | `0x19e3a8` | [D] |
| 3 word 格納 | `0x1a63b4`–`0x1a63be` | `0x19e3b4`–`0x19e3be` | [D] |
| `eSysSendMessage(0x52,0x1e,0x1c)` | `0x1a63ca`–`0x1a63d2` | `0x19e3ca`–`0x19e3d2` | [D] |
| エラー文字列 | `0xd1af20` | `0xd12f20` | [STR] |
| `vSfdMainTask` | `0x1aa5e0` | `0x1a25e0` | [SYM]/[D] |
| qid 0x52 受信 | `0x1aa5f2` | `0x1a25f2` | [D] |
| msg 0x1e jump table | `0x1aa61c` / case `0x1aa945` | — | [D] |
| msg 0x1e handler | `0x1a7f70` | `0x19ff70` | [D] |
| `0x1aed08`（TSD 変換） | `0x1aed08` | `0x1a6d08` | [D] |
| `[TSD]SetTsIn[%d:%d]` | `0xd14524` | `0xd0c524` | [STR] |
| callers | `0x1aec9a`, `0x347832`, `0x3478ea` | — | [D] |
| `0x6001` sender | `0x4577b0` | `0x44f7b0` | [D] |
| `0x6002` sender | `0x4577e4` | `0x44f7e4` | [D] |
| `0x6004` sender | `0x457858` | `0x44f858` | [D] |
| `0x6005` sender | `0x4578fc` | `0x44f8fc` | [D] |
| `0x6004` caller | `0xc66cf0` | `0xc5ecf0` | [D] |
| `bAdsbInitialized` | `0x02027184` | —（.bss） | [GOT] |

### 9.2 adsb

| ラベル | VA | file | 根拠 |
|---|---:|---:|---|
| `vAdsbCommMain` | `0xf8c9` | `0x78c9` | [SYM]/[D] |
| COMM 初期化ループ | `0xf8e0`–`0xf93c` | `0x78e0`–`0x793c` | [D] |
| `0x5014` 分岐 | `0xf8f8` | `0x78f8` | [D] |
| `0x6001` 分岐 | `0xf902` | `0x7902` | [D] |
| COMM 本ループ | `0xf984` | `0x7984` | [D] |
| `vAdsbTumMain` | `0x13df4` | `0xddf4` | [SYM]/[D] |
| TUM 初期化ループ | `0x13e1a`–`0x13e84` | `0xddfa`–`0xde64` | [D] |
| `0x5053` 分岐 | `0x13e34` | `0xde34` | [D] |
| `0x5001` 分岐 | `0x13e46` | `0xde46` | [D] |
| TUM 本ループ | `0x13e9c` | `0xde9c` | [D] |
| `0x6004` dispatch | `0xfd8e`–`0xfd94` → `0x1012c` | — | [D] |
| `eAdsbCommTuneChangeChannel` | `0x117c4` | `0x97c4` | [D] |
| body 解析 | `0x117d8`–`0x11832` | `0x97d8`–`0x9832` | [D] |
| `0x10860` | `0x10860` | `0x8860` | [D] |
| `0x1f1ac`（0x5004 送信） | `0x1f1ac` | `0x171ac` | [D] |
| GOT slot → `0x395dd4` | `0x119408` | `0x109408` | [GOT] |
| GOT slot → `0x395f3c` | `0x119440` | `0x109440` | [GOT] |
| state/op 表 | `0x395f60` | —（.bss） | [D]/[GOT] |
| `0x5004` default 分岐 | `0x14378`–`0x1447c` | — | [D] |
| state/op 判定 | `0x14398`–`0x143c2` | — | [D] |
| op jump table | `0x143ce`–`0x143d6` | — | [D] |
| `0x17fa0`（0x5004 実処理） | `0x17fa0` | `0x10fa0` | [D] |
| body コピー | `0x18142`–`0x18154` | — | [D] |
| Tune switch | `0x181a4`–`0x181b2` | — | [D] |
| `SetTSPort` | `0x186e4` | `0x106e4` | [D] |
| `fdc_chmCancelSelectChannel` | `0x7a2f8` | `0x722f8` | [D]/[STR] |
| `fdc_chmDisConnectTsUnits` | `0x7a484` | `0x72484` | [D]/[STR] |
| `fdc_chmConnectTsUnits` | `0x79838` | `0x71838` | [D]/[STR] |
| `fdc_chmSetDefaultAudioSetting` | `0x7fa98` | `0x77a98` | [D]/[STR] |
| `fdc_chmStartDecode` | `0x7b734` | `0x73734` | [D]/[STR] |
| `fdc_chmStopDecode` | `0x7b8dc` | `0x738dc` | [D]/[STR] |
| eTsInNo 変換 | `0x18f14` | `0x10f14` | [D] |
| `0x5001` handler | `0x15c6c` | `0xdc6c` | [D] |
| `0x5002` handler | `0x160b0` | `0xe0b0` | [D] |
| `0x5003` handler | `0x16688` | `0xe688` | [D] |
| `0x5005` handler | `0x16c58` | `0xec58` | [D] |
| `0x5053` sender (`vAdsbSfmMain`) | `0x20405` / `0x20508` | — | [D] |
| `0x5014` handler 3 関数 | `0x11ac4`, `0x13430`, `0x12900` | — | [D] |

### 9.3 文字列（adsb、VA = file + 0x8000）

| 文字列 | VA |
|---|---:|
| `[ADSB-COMM] RCV msgAdsbCommReportInitialized` | `0xd5dc4` |
| `[ADSB-TUM] RCV msgAdsbCommRequestTumInitialize` | `0xd7cf4` |
| `[ADSB-TUM] tummain.c invalid state = %d` | `0xd7d24` |
| `[ADSB-TUM] tummain.c invalid ope = %d, state = %d` | `0xd7d50` |
| `msgAdsbCommRequestTumChangeChannel[%d][%d][%d]` | `0xd99bc` |
| `[FSDTV] DBG SetTSPort[%d]` | `0xd9b0c` |
| `fdc_chmCancelSelectChannel` | `0xef13c` |
| `fdc_chmDisConnectTsUnits` | `0xef158` |
| `fdc_chmConnectTsUnits` | `0xef078` |
| `fdc_chmSetDefaultAudioSetting` | `0xef5a0` |
| `fdc_chmStartDecode` | `0xef240` |
| `fdc_chmStopDecode` | `0xef254` |

---

## 10. 未確定事項

1. **`eSysExSendMessage` / `eSysExReceiveMessage` の ex 引数 ABI。**
   `0x1f1ac` は 6 引数で `body` と `parsed` を送るが、TUM 側の
   `eSysExReceiveMessage` は 5 引数。`[r7+0x8]` と `[r7+0xc]` の
   どちらが body / ex か、また `0x13d90` が何を per-device と比較するかが
   まだ完全には一致しない。`0x6004` のチャンネル値が ex 経由で TUM に届く
   可能性は **完全には否定できない**。
   → 次: `libsysapi.so` の `eSysExSendMessage` 実装、または adsb の
   `eSysExReceiveMessage` PLT 呼び出し規約を取得。
2. **per-device `+4` の実体。** ポインタか、デバイス番号（1/2）か。
   `0x15664` は `+4` に 1/2 を書くが、`0x1f1ac` は `+4` をポインタとして
   `body` に使う。両者が同じグローバルか、別配列か未確定。
3. **state/op 表 `0x395f60` の初期化タイミング。** `0x1dc98` / `0x1dfc4` が
   どのメッセージで呼ばれ、どの state/op を許可するか。
4. **24 バイト tune body の全フィールド名。** `+0x14` が Tune なのは確定。
   `+0x00` が eTsInNo なのは `SetTSPort` の使用から有力。
5. **4K の `eTsInNo` 実値。** `0x18f14` が 10→2, 11→3 に変換するが、
   4K TLV が 10 か 11 かは未確定。
6. **`Sfd_TsdSetTsIn` の body[1] の意味。** （0..7 の補助ソース/ポートと推定）
7. **`0x10100005` / `0x10100010` の ARC 側の意味。** ホスト取得物に名前が無い。
8. **`0x6001` body の ABI。** abs は `r0=0/1` を渡すが、
   `eSysSendMessage` の第 3 引数が値かポインタか未確定。
9. **`0x6002` の応答 qid `0x370` の送信元。** adsb 内の `qid 0x370` 送信は
   `0xf06a` / `0x10c44` / `0x115f8` / `0x1358e` にあるが、`0x5015` との
   対応は未確定。
10. **`0x6005` の body 長の不一致。** abs `0x4578fc` は 12 バイト（3 word）
    しか作らないが、adsb `0x11790` は 24 バイトをコピーし、`0x1f180` は
    `0x5003` を len `0x18` で送る。24 バイトの残り 12 バイトの出所が未確定。
    （`0x457754` の bump allocator から読む隣接領域の可能性 [INF]）
10. **`0xcc=0x40` でも TLV ring が 0 バイトになる別要因。**
    TUM が選局を完了しても、TLV デバイスの 5 番目 output が ARC に
   サービスされない可能性（既存 Phase 2 §13）。

---

## 11. 次の実機試験（read-only 優先・停止不要）

### E0: ゲートの read-only 判定

1. `/tmp/bdh_out_msgh.lst` から `0x36F/0x371/0x52` の msqid を取得。
2. `abs` `0x02027184` を read（`bAdsbInitialized`）。
3. adsb `.bss` を read:
   - `0x395f3c + dev*0x168` の `+0x28`, `+0x2c`, `+0x00..0x24`
   - `0x395f60` の 55 バイト
   - `0x395dd4 + dev*0x168 + 4`
4. bank2 TSID / 0xcc / bank7 を read。
5. 製品 4 ring の `sbuf_status_raw` を read（wpos）。

判定:
- `bAdsbInitialized=0` または表がゼロ → ゲート未通過。E1 へ。
- 表が非ゼロ・state/op 妥当 → TUM は初期化済み。E2 へ。

### E1: 初期化込みの 0x6004

1. `0x6001` を `0x36F` へ送る（body `{mode}` 4 バイト、len 4）。
2. 2–5 s 待ち、E0 の 2–3 を再読。
3. 表が非ゼロになったら `0x6002` を別スレッドで送り、qid `0x370` 応答を待つ。
4. `0x6004`（製品 body、len 0x28）を送る。
5. 15 s、200 ms で bank2 / 0xcc / bank7 / ring wpos を read。

- ここで bank2 が目的の 4K TSID になれば、TSin も `SetTSPort` で設定済み。
- ならない場合、E0 の表/ブロックを再読し、どのゲートで止まったかを記録。

### E2: 既に初期化済みの個体

1. `0x6004` のみ送る。
2. bank2 が変わらないなら、adsb `.bss` の 24 バイトブロック
   （`0x395f3c + 0x10`）を read し、Tune/eTsInNo が要求と一致するか確認。
3. 不一致なら、`0x6005`（`0x5003`）で 24 バイトブロックを更新する実験。
   body レイアウトは §5。**ただし 0x6005 の abs 側 body は 12 バイトしか
   作らないため、24 バイトの出所を先に確定する必要がある** [未]。

### E3: TLV tap

- E1/E2 で bank2 が変わった後、既存 `SBUF_CREATE` + `connect_output` で
  `{3,1,9,1,0x1000,0x400000,1,3,0}` を作り、`tlv_score` で判定。
- 0 バイトなら、製品 ring の wpos 増加と `0xcc`/bank7 を再確認。
  製品 ring が進んでいれば TLV デバイス自体は動作しており、
  5 番目 output の ARC サービス問題（Phase 2 §13）を疑う。

### E4: どうしても TUM が動かない場合

- `0x5053` を `0x371` へ直接送り、`0x6004` を再試行（§6.3）。
- それでも state/op 表がゼロなら、`0x5012` 系のイベントを直接送る必要がある。
  body が未確定のため、**この段階では実機試験を止め、追加の静的解析
  （`0x1dc98` / `0x1dfc4` の引数）を先に行う**。
