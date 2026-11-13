# Phase 4 — xcdrv から ARC へ渡せる入口と、TLV 入力の残り課題

対象: 4S-C00AS1 (`xcdrv.ko`, `xc6800_arc700.bin`, `adsb`, `libdtv_cmn.so`)
方針: 実機は read-only。ストレージ削除なし。コード変更は本 doc のみ。
解析日: 2026-11-13

---

## 0. 結論（先に要点）

| 目的 | 結果 |
|---|---|
| 1. 既存 ioctl で ARC に 32B の FDC/TSin メッセージを渡す | **不可**。汎用 32B パススルーは無い。`ioctl 0x78` は 12B を載せるが、`general_ctrl_cmd` が `word2` を物理バッファとして `flush_dcache` するため、ユーザが用意した任意アドレスでは **kernel oops**（実機で 3 回確認）。`0x50/0x52/0x53` は `general_cmd` 経由で ARC に届く（構造化ペイロード）。 |
| 2. ARC700 raw FW の dispatcher 特定 | **未達**。FW は base 0 の ARCompact。`ConnectTsUnits` / `0x10010010` という文字列・即値は FW に存在しない。`ConnectInBuf` は AVD(ES) の入力バッファ接続で、TLV 入力とは別物。ARC 用の実用的な逆アセンブラが無く、cmd 0x78 のハンドラは未特定。 |
| 3. type 0x24 に明示 input を接続する別経路 | **現状無し**。type 0x24 は `n_in=0` で `connect_input` 対象外。crossbar (`cmd 0x53` → `0x61f0`) が候補だが、書込み主体（kernel `config_tsi` か ARC か）は未確定。 |
| 4. t4s-probe へ追加できる具体仕様 | **あり**。§5 に ioctl 0x78 の 12B 仕様、§6 に FDC の正しい送信仕様（ただし abs 宛）、§7 に TSI crossbar 仕様を記載。 |

最重要の訂正:

- **FDC `0x10010010` は ARC 行きではない。** これは `adsb` → `abs.T_ADSB_TUM` の
  ホスト内メッセージ（SysV queue key=1）であり、ARC のコマンド体系
  (`s_dev_com`, cmd 0x50/0x53/0x78 ...) とは別物。
- これまで `qid 0x371` に送っていた `0x10010010` は、TUM の eSys キューに
  誰も読まないメッセージを積んでいただけ（`lspid/lrpid=0` の理由）。
- ARC へ何かを渡す正しい入口は **`/dev/vixs/xcodedrv` の ioctl** であり、
  SysV queue ではない。

---

## 1. xcdrv の ioctl ディスパッチ（確定）

`xcdrv.ko` は not stripped。シンボルと REL から次の 3 段階が確定する。

### 1.1 第 1 段: `xc_ioctl` → `check_ioctl`

- `xc_ioctl` (`0x21138`) は約 0x90c バイトの内部 request を確保し、
  ユーザ引数をコピーする前に `check_ioctl` (`0x17fe8`) を呼ぶ
  （`kmem_cache_alloc_trace(cache, GFP=0xd0, size=0x90c)`）。
- `check_ioctl` は `cmd = u16(ioctl)` を `cmd_check_dispatch`（`.data:0x1e0`）
  で索引し、`*_func(req)` を呼ぶ。各 `*_func` の仕事は
  **request サイズ (`req+0x8b8`) と out サイズ/フラグ (`req+0x8cc`, `0x8d8`)
  を書くことだけ**。ここでは ARC へは何も送らない。
- 実測した `cmd_check_dispatch` の主な slot（slot = (reloc - 0x1e0)/4）:

| cmd | handler | req size |
|---:|---|---:|
| 0x28 | `xc_sdev_create_func` | 可変 |
| 0x2a | `xc_sdev_ctrl_func` | class 別 |
| 0x2b | `xc_sdev_info_func` | class 別 |
| 0x2d | `xc_sdev_create_ex_func` | 可変 |
| 0x32 | `xc_sbuf_create_func` | 可変 |
| 0x46 | `xc_pipeline_create_func` | 可変 |
| 0x50 | `xc_tsi_config_func` | **40** |
| 0x51 | `xc_tsi_status_func` | **36** |
| 0x52 | `xc_rmx_config_func` | 可変 |
| 0x53 | `xc_tsi_config_ex_func` | **44** |
| 0x78 | `xc_cust_msg_cmd_func` | **12** |
| 0x7a | `xc_mem_map_func` | 12 |
| 0x86 | `xc_get_graph_func` | 32 |

（0x2a/0x2b の class 別サイズは `devN_ctrl_table` / `devN_info_table` で確定。
例えば class 26 = TLV は `dev26_ctrl_table`:
idx0 `mmt_demux_config`(0 bytes), idx1 `tlv_add_pid_filter`(64 bytes + out 1),
idx7 `mmtp_filtering`(16), idx9 `tlv_dev_config`(8)。）

### 1.2 第 2 段: `kdrv_dev_ioctl`

`xc_ioctl` は `check_ioctl` の後、`kdrv_dev_ioctl` (`0x5600`) を呼ぶ。
この関数は `cmd-11` を jump table（`.text:0x5640`, cmd 11..139）で引く。

明示ハンドラを持つのは次のみ:

| cmd | 遷移先 | 意味 |
|---:|---|---|
| 0x28, 0x2d | `kdrv_dev_create` | device create |
| 0x29 | `kdrv_dev_destroy` | device destroy |
| 0x2a | `kdrv_dev_ctrl` | sdev class control |
| 0x2b | `kdrv_dev_info` | sdev class info |
| 0x70-0x73, 0x76 | `kdrv_acb_if` | ACB |
| 0x82 | `0x5b64` | (内部) |
| 0x83 | `0x59ec` | (内部) |
| 0x87 | `0x5b50` | (内部) |
| 0x88-0x8b | host device if | |

**それ以外（0x50, 0x52, 0x53, 0x78 等）は default `0x587c` に落ち、
`mvn r0,#2` = `0xfffffffd` = **-3** を返す。**
`xc_ioctl` は戻り値 -3 を見ると `process_ioctl_request` (`0x2fd2c`) を呼ぶ。
つまり「汎用コマンド」はここから先が本体。

### 1.3 第 3 段: `process_ioctl_request`

`process_ioctl_request` は `cmd-1` を jump table（`.text:0x2fd98`, cmd 1..151）
で引く。主な遷移:

| cmd | 遷移先 | 実体 |
|---:|---|---|
| 0x50, 0x53 | `0x30220` → `0x30234` | `general_cmd` |
| 0x51 | `0x30208` | `get_tsi_status` |
| 0x52, 0x78 | `0x30570` → `0x30234` | `general_cmd` |
| 0x64 | `0x300f4` | 温度 |
| 0x68 | `0x300e8` | cancel ioctls |
| 0x7a | `0x30a2c` | mem map |
| 0x86 | default | graph は別経路 |

`0x30220`/`0x30570` は `check_fw_status` を通してから `0x30234` に合流し、
`general_cmd(handle, req)` (`0x22608`) を呼ぶ。

### 1.4 ARC 行きメッセージの実体（`general_cmd` → `general_ctrl_cmd`）

`general_cmd`:

1. `req+0x8c8..cb` に初期値 `0xfffffd18` (-744) を書く（タイムアウト用）。
2. `cmd == 2` だけ特別扱い（`req[0] == 1` のときだけ送る）。
3. それ以外は `general_ctrl_cmd(handle, req)`。
4. 成功なら `req+0x8c8..cb = 0`。失敗なら `0xfffffd17` (-745)。
5. `xc_ioctl` はこの 4 バイトをそのまま ioctl の戻り値にする。

`general_ctrl_cmd` (`0x221dc`):

- `get_mips_com_buffer(handle, 0)` で 1216 バイトの com slot を取得。
  slot base = `[xcode+0x463ac] + slot*0x4c0`、ペイロード域は `slot+0x60`。
- `cmd == 3` はバッファ物理アドレス変換の特殊系。
- `cmd == 0x78` は `req+0x8..0xb` の値を `get_buf_phy_addr` に通して
  書き戻し、`com+0x30 = 1` を立てる。ただし `get_buf_phy_addr(handle, va, 0)`
  は **va をそのまま返す**（`r2==0` のとき）ので、実質は素通し。
- それ以外は以下を書いて `submit_mips_com_buffer` を呼ぶ（`r6` = com slot）:

| offset | 書く値（観測） | 推定 |
|---:|---|---|
| +0x00 | （`get_mips_com_buffer` が初期化） | |
| +0x04 | （バッファ有無で `payload_pa+0x60` を書く分岐あり） | |
| +0x08 | `req_size + 96` | `s_dev_com` 全長 |
| +0x0c | 1 | version/flags |
| +0x10 | `u16(cmd)` | **ARC command** |
| +0x14 | `u16(cmd >> 16)` | **sub command** |
| +0x18 | 2 | |
| +0x1c | slot 依存 | |
| +0x20 | slot index | |
| +0x24 | 0 | |
| +0x28 | `req+0x8c0` の u32 | copy フラグ |
| +0x2c | 0 | |
| +0x30 | `req+0x8c0`? / 0x78 では 1 | mode |
| +0x34 | 0 | |
| +0x48 | u16 1 | |
| +0x50 | 0 | |
| +0x54 | u16 `req+0x30`? | |
| +0x56 | u16 `req_size+96` | length again |
| +0x58 | ペイロード物理アドレス | DMA 転送先 |
| +0x60.. | `req` の先頭 `req_size` バイト | **payload** |

- `submit_mips_com_buffer` (`0x2becc`) は上記を `write_check` で書き、
  `req+0x8b8 != 0` ならペイロードを DMA/コピーし、
  `_xc_mq_enqueue(handle, slot+0x40, 0)` と `set_system_event` で ARC を kick する。
- つまり ARC が受け取るのは **96 バイトの `s_dev_com` ヘッダ + payload**。
  FW 内の文字列 `sizeof(s_dev_com)=%d`（`0x1bff94`）と整合する。

### 1.5 cmd 0x78 = 12B 生パススルー（Goal 1 の答え）

`xc_cust_msg_cmd_func` (`0x16aec`) は 12 バイトをコピーし、
`req+0x8b8 = 12`、`req+0x8cc = 1` を立て、`req[0..3]` を `req+0x8c4..c7`
にも複製する（後者は `general_cmd` では未使用）。

その後 default → `process_ioctl_request` の 0x78 ケース → `general_cmd` →
`general_ctrl_cmd` → 12 バイトのまま `s_dev_com` の payload に載る。

ユーザ側の request は 12 バイト:

```c
struct cust_msg {
    u32 word0;   /* ARC payload +0x00 */
    u32 word1;   /* ARC payload +0x04 */
    u32 word2;   /* ARC payload +0x08。0x78 分岐で get_buf_phy_addr を通るが、
                    通常は素通し（buffer handle が 0 のため） */
};
ioctl(fd, (sub << 16) | 0x78, &req, 12);
```

- `sub` は `s_dev_com` の +0x14 に入る。0 が無難。
- 戻り値は **ARC の応答ではなく、キュー投入の成否**
  （0 = 投入成功、`0xfffffd18` = timeout、`0xfffffd17` = エラー）。
- **重大な注意**: `general_ctrl_cmd` の 0x78 分岐は `word2` を物理アドレス、
  `word1` を長さとして `flush_dcache` に渡す。`word2=0` は
  `v7_flush_kern_dcache_area` で kernel oops する（実機で確認）。
  XCode アパーチャの SBUF リング PA でも oops した。
  **ユーザ空間から任意の 12B を安全に送る手段としては使えない。**
  詳細は §12。
- ARC 側の cmd 0x78 ハンドラの意味は未確定（§4）。
- 12 バイトを超えるペイロードは **この ioctl では送れない**。
  `req+0x8b8` は handler が 12 に固定するため、ユーザが大きくても切り捨て。

---

## 2. FDC の正体（重要な訂正）

### 2.1 `fdc_sys*` は ARC ではなくホスト内 IPC

`adsb` の `fdc_sysSendMessage` は `libdtv_cmn.so`（md5 `b3e2f9a9...`）にある。

- `fdc_sysSendMessage(queue_id, msg, len)` (`libdtv_cmn:0x6248`):
  - `len <= 0x80`
  - `group = (queue_id >> 20) & 0xff`
  - FDC 共有テーブルから group の `msqid` を引き、`msgsnd(msqid, {mtype=queue_id, data}, len, IPC_NOWAIT)`。
- `fdc_sysFindMessageQueue(id, &out)` は共有テーブルを引き、`out = id` を返すだけ。
- `fdc_sysCreateMessageQueue` はグループ表を共有メモリに作り、
  SysV queue を `msgget` で用意する。実機 `/proc/sysvipc/msg` には
  key 1..21 の queue が同時刻（ctime 1483196402）に存在した
  （perms 666）。key と group の対応は **有力だが未確定**。
- したがって `fdc_sys*` は **adsb ↔ abs（T_ADSB_* スレッド）** の
  ホスト内メッセージバスであり、ARC への DMA/メッセージではない。

### 2.2 `0x10010010 ConnectTsUnits` の実送信

`adsb:0x79838`（`fdc_chmConnectTsUnits`）を再逆アセンブルした結果:

```
queue = fdc_sysFindMessageQueue(0x10100001, &send_qid)   ; out=0x10100001
reply = fdc_sysFindMessageQueue(handle + 0x10181002, &reply_qid)
sem   = fdc_sysFindSemaphore(handle + 0x20181002)
fdc_sysTakeSemaphoreWait(sem)

payload (32B):
 +0x00 u32 0x10010010
 +0x04 u16 0
 +0x06 u16 0x18
 +0x08 u32 reply_qid
 +0x0c u32 handle
 +0x10 u32 source        ; eTsInNo 10->2, 11->3, else 1
 +0x14 u32 tsdNo
 +0x18 u32 op            ; SetTSPort は 5
 +0x1c u32 0

fdc_sysSendMessage(send_qid, payload, 0x20)
r = fdc_sysReceiveMessageWait(reply_qid, buf, 0x80)
if (buf[0] == 0x20010010) result = buf[0x14]
```

（`phase2b-tsin.md` の「`0x10100010`, len 0x20, payload 5 word」は
`movs #0x10; movt #0x1001` の読み取りミス。正しくは **`0x10010010`**。
queue id も `0x10100001`。`0x10101001` は `phase2b` の別関数の値で、
この ConnectTsUnits では使われない。）

### 2.3 どこまで届くか

- queue `0x10100001` の group は `(0x10100001 >> 20) & 0xff = 1`。
  実機の key=1 の msqid は読み取り時 **17367562**（再起動で変わる。
  ハードコード禁止）。ただし key=group は推定であり、実送信前に
  `lspid/lrpid` が adsb/abs であることを確認する。
- この queue の受信者は `abs.T_ADSB_TUM`（pid 287）であり、ARC ではない。
- しかも payload の `handle` は adsb の per-device fdc handle
  (`0x395dd4 + dev*0x168 + 8`) で、abs 側の状態と一致していなければ
  `fdc_sysFindMessageQueue(handle + 0x10181002)` が失敗し、応答は返らない。
- よって **外から `0x10010010` を投げて ARC の TLV 入力を開くことはできない**。
  これは abs の TSin ルーティング（`SetTSPort`）を起動するメッセージで、
  その先の ARC 設定は abs が xcdrv 経由で行う。

### 2.4 今回の実機所見との整合

- `qid 0x371`（msqid 16679421）に `0x10010010` を送っても
  `lspid/lrpid=0` のまま = TUM は読んでいない。
  `0x371` は TUM の **eSys** キューであり、FDC の key=1 queue ではない。
- 既存 `t4s-probe` の `tlvcap3` は `qid 0x371` に送っているため、
  ARC にも abs にも届かない。これが「0 bytes」の一因。

---

## 3. ARC700 raw FW の現状

### 3.1 ファイル

- `/lib/firmware/xc6800_arc700.bin` 2,216,076 バイト。
- 先頭はヘッダ無しの raw。base 0 でロードされているとみなせる
  根拠: `0x20` 付近の `mov r1, 0x000f0100` など、I/O レジスタの
  絶対低位アドレスがそのまま埋まっている。
- ARC は ARCompact（16/32 ビット混在）。`rasm2 -a arc -b 16` は
  16 ビット命令と一部の long 命令をデコードできるが、
  `mov rX, limm` の 32 ビット即値を 16 ビットとして表示するなど
  **long-immediate の扱いが壊れている**。そのままでは実用不可。
- Ghidra 12.1.4（`.rea-tools`）に ARC プロセッサは入っていない。
  r2 の ARC も 32 ビットモードでは 0x40 バイトずつ進むなど破綻。
  → **実用的な ARC 逆アセンブラが無い**のが最大のブロッカー。

### 3.2 文字列と xref

- `0x1aeb78 "[%d] ConnectInBuf"`, `0x1aeb8c "avd_connect_in_buf"`,
  `0x1aeb20/4c "CONNECT_INPUT 1/2"` は **AVD（ES バッファ）** の接続。
  TLV/TS 入力の `ConnectTsUnits` ではない。
- `0x1ba0d8 "T%u MMT_IN:0x%p: UFL@..."`, `0x1ba10c "mmtp_get_read"`,
  `0x1ba8c8 "TLVDROP"`, `0x1bd538 "TLV_STSD"` は ADS-B TLV/MMT 側。
- `ConnectTsUnits`, `ConnectTsUnit`, `TsUnits` は **FW に存在しない**
  （`strings -a` で 0 件）。
- `0x10010010` の即値も FW に存在しない（LE/BE/2 バイト境界すべて 0 件）。
  → FW のホストコマンドは `s_dev_com` の `cmd/sub` 体系で、
  adsb の FDC ID 体系とは別。
- 文字列への絶対ポインタ（`base + offset`）は 0 件。
  ARC は `gp` 相対で rodata を参照している可能性が高い。
  `gp` の復元が必要。

### 3.3 見つかった手掛かり

- `0x34f8`: `mov r20, 0x10010000`（リテラル `0x10010000`）。
  直後に `bl 0x2850`（トレース/状態更新系）と `bl 0x21e0`。
  FW 内で唯一の `0x1001xxxx` ベース即値。TLV/装置イベントの
  通知 ID を組み立てている可能性があるが、`0x10` を足すコードは未確認。
- `0x56956` 付近は `mov r16, <imm>; b ...` の表（値は
  `0x000dec00, 0x001bd800, 0x002f7600, ...`）。コードポインタではなく
  バッファ/ウィンドウ基底の表とみられる。
- `0x7010` 付近は `0x000d1000, 0x000e1004, ...` の表。ISR/ウィンドウ表。
- FW 内に `sizeof(s_dev_com)=%d`（`0x1bff94`）があり、
  ホスト側 `submit_mips_com_buffer` の 96 バイトヘッダと一致する。
  dispatcher はこの構造体の +0x10 (cmd) を見ているはずだが、
  gp 相対のため xref をまだ切れていない。

---

## 4. ARC dispatcher を次に特定する方法（未確定）

1. **ARC 逆アセンブラを用意する**
   - Ghidra の ARC 拡張（`ghidra-arc` / ARCompact）を入れて
     base 0, 16/32 mixed でロード。
   - 代替: `arc-elf-objdump` を含む ARC GNU toolchain を用意。
   - r2 の `arc` は long-immediate が壊れるので、これ単体では不可。
2. **gp を復元する**
   - 起動コード（`0x0`–`0x100`）で `gp` に代入する箇所を探す。
     `mov gp, limm` / `lr gp,[...]` / `add gp,...`。
   - gp が分かれば、`add rX,gp,#imm16` の imm から rodata/文字列の
     xref を機械的に張れる。`s_dev_com` の cmd 読み出し
     （`ld rX,[rY,#0x10]`）と 0x78 比較を探す。
3. **実機の com buffer を読む（read-only）**
   - `submit_mips_com_buffer` の slot base は `[xcode+0x463ac] + slot*0x4c0`。
     この領域はホスト RAM 上で、ARC から DMA 可視。
   - `/proc/<adsb|abs>/pagemap` は PFN 0 で読めないことがあるが、
     `xcdrv` の `xc_mmr_base` / MMR 経由で ARC 側ウィンドウを read-only で
     覗ける可能性がある。まずは `0x63ac` の値を `xc_get_dev`/graph から取得。
4. **cmd 0x78 の応答を観測する**
   - 0x78 投入後、`dmesg`/`log2h` と ARC 側の `s_dev_com` ステータス
     （+0x18/+0x30 など）を read-only で確認。
   - ARC が cmd 0x78 を「未知コマンド」として捨てるなら、`s_dev_com` の
     完了フラグが立たない/エラーになる。ここで初めてハンドラの有無が分かる。

---

## 5. t4s-probe に追加する仕様（1）: `custmsg`（cmd 0x78）

### 5.1 API

```rust
// t4s-core/src/xcdrv.rs
pub const CUST_MSG: c_int = 0x78;

pub fn cust_msg(&self, word0: u32, word1: u32, word2: u32, sub: u16) -> io::Result<u32> {
    let req = [word0, word1, word2];       // 12 bytes
    // ioctl number = (sub as i32) << 16 | 0x78
    let cmd = ((sub as i32) << 16) | CUST_MSG;
    self.ioctl_raw(cmd, &req)              // 戻り値 = req+0x8c8 (status)
}
```

- `ioctl(fd, cmd, &req)` は 12 バイトの配列を渡す。
- 戻り値:
  - `0` = ARC の MIPS キューに投入成功（ARC の処理完了ではない）
  - `0xfffffd18` (-744) = timeout（`get_mips_com_buffer` 等）
  - `0xfffffd17` (-745) = エラー（`submit_mips_com_buffer` 失敗）
  - その他 = `check_fw_status` 等のエラー
- 失敗時は **ARC に何も渡っていない**。再送しない。

### 5.2 実機試験マトリクス（**実行しない**）

> 2026-11-13 の実機試験で `word2=0` は kernel oops した（§12）。
> 以下の表は「もしベンダ内部の有効なバッファハンドルが得られた場合」の
> 記録であり、現状の `t4s-arc` は `word2=0` を拒否してダミーページを
> 差し込むが、それでも oops する。再実行禁止。

`sub = 0` 固定、`word2 = 0` から開始。

| # | word0 | word1 | 期待/確認 |
|---|---:|---:|---|
| 1 | 0x00000000 | 0 | 戻り値 0 なら「cmd 0x78 が受理される」ことだけ確認 |
| 2 | 0x00000078 | 0 | cmd 自体を payload にも入れる |
| 3 | 0x10010010 | 0 | FDC ID を payload に入れる（FW 側で解釈されるか） |
| 4 | 0x10100001 | 0 | queue id を入れる |
| 5 | 0x00000001 | 0x10 | 引数パターン |
| 6 | 0x00000002 | 0x18 | 引数パターン |

判定:
- 戻り値 0 でも、ARC が未知コマンドなら `s_dev_com` の完了が返らない。
  `dmesg` に `xc`/`arc` のエラーが出ないか、TLV ring が 0 のままかで判断。
- t4s-probe 側で `tlvcap2`/`tlvcap3` の ring 読み取りを同時に走らせ、
  `total` が 0 から増えるかを確認する。
- **word2 にノンゼロを入れると `get_buf_phy_addr` の分岐に入る。**
  最初は必ず 0。必要になったら `xc_mem_alloc`/`mem_map` で得た
  バッファ VA を入れる。

### 5.3 注意

- cmd 0x78 は `check_fw_status` を通る。FW が落ちている/ロード中は
  -744/-745 で返る。連打しない。
- 12 バイトを超えるデータは入らない。32 バイトの FDC を送る用途には
  使えない（§2）。
- `sub` を変えると `s_dev_com+0x14` が変わる。FW が sub を見るかは未確定。
  まず sub=0。

---

## 6. t4s-probe に追加する仕様（2）: FDC（abs 宛・参考）

ARC には届かないが、`abs` の `SetTSPort` を外から起動できる可能性があるため
正しい形を記録する。**handle が必要なため、現状の probe では成功しない。**

```rust
// SysV msg (libc)
// group = (0x10100001 >> 20) & 0xff = 1
// msqid = msgget(key = 1, 0);   // 実機読み取り時 17367562。固定値禁止
// mtype = 0x10100001
let payload: [u32; 8] = [
    0x1001_0010,          // +0x00
    0x0018_0000,          // +0x04 u16 0, +0x06 u16 0x18
    reply_qid,            // +0x08  (handle + 0x10181002 を abs が解決)
    handle,               // +0x0c
    source,               // +0x10  (10->2, 11->3, else 1)
    tsd_no,               // +0x14
    op,                   // +0x18  (5 = connect, 0 = disconnect)
    0,                    // +0x1c
];
msgsnd(msqid, &{mtype: 0x10100001, payload}, 0x20, IPC_NOWAIT);
```

- `handle` は adsb の `0x395dd4 + dev*0x168 + 8`。
  probe からは読めない（abs/adsb の `.bss`、PFN 0）。
- `reply_qid` を自分で用意しても、abs 側が
  `fdc_sysFindMessageQueue(reply_qid)` に失敗すれば応答は来ない。
- したがって **この経路は「abs の内部状態を読めるようになるまで保留」**。
- `qid 0x371` へ送るのは誤り（TUM の eSys キューで、FDC ではない）。

---

## 7. t4s-probe に追加する仕様（3）: TSI crossbar（cmd 0x53）

TLV type 0x24 に input を作る代わりに、**物理 TSI の crossbar を
TLV 側へ向ける**唯一の既存経路。`phase1-cs4k.md` §6 の手順を
そのまま `custmsg` と同じ `general_cmd` 系で送れる。

```c
struct tsi_ex {           /* 44 bytes */
    u32 port;             /* physical TSI 0/1 */
    u32 me_packet;        /* 0=188,1=192,2=204,3=130 */
    u32 parallel;
    u32 enable;
    u32 use_sync;
    u32 disable_auto_reset;
    u32 active_high;
    u32 bpc_sel;
    u32 use_neg_edge;
    u32 drop_null;
    u32 asi_port;         /* crossbar 0x61f0 の行き先。0..? */
};
ioctl(fd, (0u16 << 16) | 0x53, &cfg, 44);
```

- 0x53 は `process_ioctl_request` → `general_cmd` → ARC に届く。
- `phase1` は kernel の `config_tsi` (`0x3d8d4`) が `0x61f0` の nibble を
  書くと報告しているが、`config_tsi` への静的 xref は無く（`xc_tsi_config_func`
  はサイズ設定のみ）、**呼び出し元が kernel か ARC かは未確定**。
  どちらにせよ `0x61f0` は事前保存し、失敗時に書き戻す。
- **書く前に `mmr_read32(0x61f0)` を保存**。失敗時に書き戻す。
- `0x6000/0x6008/0x600c` も保存（`phase1` §6.6）。
- 製品 abs は 0x53 を呼ばないので、書くと製品の TSI 経路と衝突しうる。
  試験は 1 回、値は最小限（`port=0`, `asi_port` 候補 2/3）。

---

## 8. 失敗時の復旧 / watchdog

| 操作 | リスク | 復旧 |
|---|---|---|
| ioctl 0x78（未知 cmd） | ARC が assert / hang する可能性 | 送信を止め、`dmesg` を確認。ARC が応答しなければ `check_fw_status` が失敗し続ける。プロセスは止めない。 |
| ioctl 0x53 | crossbar `0x61f0` 書換で 2K/CS 経路が壊れる | 事前保存した `0x61f0`/`0x6000`/`0x6008`/`0x600c` を書き戻す。戻らなければ電源再投入（`0x61f0` は電源復帰で identity `0x10`）。 |
| FDC queue 直叩き | abs の FDC 状態機械を壊す | 送らない。どうしても送った場合は abs を再起動しない（watchdog ~60s で全体が落ちる）。電源再投入。 |
| `xc_halt_cpu` (cmd 0x01) | **絶対に送らない** | — |

- `abs` を SIGSTOP/kill すると watchdog で 60 秒後に再起動ループ。
  Phase 4 の操作はすべて abs を生かしたまま行う。
- ARC の FW 再ロードは `xcdrv` の reload か電源再投入のみ。
  我々の ioctl は FW を再ロードしない。
- 0x78 の戻り値が -744 のままなら、ARC のキューが詰まっている。
  それ以上送らず、`/proc/sysvipc/msg` と `dmesg` を読む。

---

## 9. 未確定事項

1. **ARC cmd 0x78 のハンドラ**。12B payload の word0/word1 の意味、
   sub の有無、応答の返し方。FW の gp 復元と dispatcher 特定が前提。
2. **`0x10010000` 即値（FW `0x34f8`）の用途**。TLV/装置イベント ID か。
3. **ARC が TLV type 0x24 の入力をどう選ぶか**。`A31002A` の mode 1 が
   「tuner box」で、FED の TS 出力を暗黙に拾うのか、crossbar 必要か。
4. **`0x6004`/`0x5004` の TUM state/op 表**（`0x395f60`）を外から埋める方法。
   `0x5012`/`0x4105...` のイベントを送る必要がある（phase2b §2.4）。
5. **FDC handle** を read-only で取得する方法（abs `.bss` の PFN 0 問題）。
6. **type 0x24 に `connect_input` を許す FW バージョンがあるか**。
   現 FW は `n_in=0` 固定の可能性が高い。

---

## 10. 根拠（ファイル/アドレス）

- `xcdrv.ko`: `nm`, `readelf -r`, `llvm-objdump -dr --triple=armv7-linux-gnueabi`
  - `xc_ioctl` 0x21138, `check_ioctl` 0x17fe8, `cmd_check_dispatch` .data:0x1e0
  - `kdrv_dev_ioctl` 0x5600, table .text:0x5640 (cmd 11..139, default 0x587c = -3)
  - `process_ioctl_request` 0x2fd2c, table .text:0x2fd98 (cmd 1..151)
  - `general_cmd` 0x22608, `general_ctrl_cmd` 0x221dc
  - `xc_cust_msg_cmd_func` 0x16aec, `xc_tsi_config_func` 0x14d08,
    `xc_tsi_config_ex_func` 0x14c18
  - `get_mips_com_buffer` 0x2b934, `submit_mips_com_buffer` 0x2becc
  - `dev26_ctrl_table` .data:0x4f0（TLV class 26）
- `adsb` (stripped, Thumb-2)
  - `fdc_chmConnectTsUnits` 0x79838–0x799d2（payload 32B、reply 0x20010010）
  - `SetTSPort` 0x186e4、`fdc_chm*` 0x7a2f8/0x7a484/0x7fa98
  - FDC ハンドラ表 `.data:0x11a1a8`（`{id, handler}`、`0x10010010 → 0xa0551`）
  - 0x10010010 の受信応答 0xa0550（24B、`0x20010010`）
- `libdtv_cmn.so` md5 `b3e2f9a9cec06db96ffe64a13471aa25`
  - `fdc_sysSendMessage` 0x6248（`msgsnd`, mtype=queue_id, len<=0x80）
  - `fdc_sysFindMessageQueue` 0x7178（共有テーブル、out=id）
  - `fdc_sysCreateMessageQueue` 0x5204（group 表、SysV queue）
- `libsysapi.so` md5 `1fa9644297c7c10cd06ddf07d96e0521`（eSys 側、参考）
- 実機 read-only:
  - `/proc/sysvipc/msg`: key=1 → msqid 17367562（group 1）
  - `/tmp/bdh_out_msgh.lst`: qid 0x371 → msqid 16679421（TUM eSys）
  - `ps`: 263 adsb, 279 abs, 287 abs.T_ADSB_TUM, 289 abs.T_ADSB_SFM
- FW: `xc6800_arc700.bin` 2,216,076 B
  - `0x1aeb78 ConnectInBuf` (AVD), `0x1ba0d8 MMT_IN`, `0x1ba10c mmtp_get_read`,
    `0x1ba8c8 TLVDROP`, `0x1bff94 sizeof(s_dev_com)`
  - `0x34f8 mov r20,0x10010000`; `0x20 mov r1,0x000f0100`
  - `ConnectTsUnits`/`0x10010010` は 0 件

---

## 11. 次の実機試験（read-only 原則）

1. `custmsg` の戻り値だけを確認（word0=0, sub=0, word2=0）。
   → cmd 0x78 が `general_cmd` で受理されるか。
2. `custmsg` word0=0x10010010 を 1 回。TLV ring が増えるか、
   `dmesg` に ARC エラーが出ないか。
3. 変化が無ければ、`0x53` で `0x61f0` を TLV 側へ 1 段だけ動かし、
   `mmr_read32(0x61f0)` と `bank2`/`0xcc` を前後比較。
4. それでも 0 なら、TUM の state/op 表を埋める経路
   （`0x5012`/`0x4105...`）を phase2b の続きとして解析する。
5. 並行して Ghidra ARC 拡張を用意し、FW の `s_dev_com` dispatcher と
   cmd 0x78 ハンドラを特定する。

各試験の前後で保存するもの:
`0x6000`, `0x6008`, `0x600c`, `0x61f0`, `/proc/sysvipc/msg`,
`/tmp/bdh_out_msgh.lst`, `dmesg`, `tlvcap2` の `first`/`total`。

---

## 12. Phase 4 実機追記（2026-11-13）

実装: `rust/t4s-arc`（workspace member、`t4s-core` 依存）。armv7 musl static。
配置: `/mnt/t4s/opt/t4s/bin/t4s-arc`。
サブコマンド: `custmsg`, `tsi-read`, `tsi-ex`, `destroy-buffer`。

### 12.1 `custmsg` は安全に使えない（kernel oops）

`0x78` の 12B は「生パススルー」ではなかった。`general_ctrl_cmd` は
`word2` を物理アドレス、`word1` を長さとして `flush_dcache` に渡す。
実機では次の 3 通りすべてで `v7_flush_kern_dcache_area` が oops し、
`t4s-arc` は SIGSEGV (139) で終了した。

| # | word2 | 結果 |
|---:|---|---|
| 1 | `0x00000000` | oops（`flush_dcache+0x28` → `v7_flush_kern_dcache_area+0x20`） |
| 2 | SBUF ring PA `0x28e59000`（XCode アパーチャ） | 同じ oops |
| 3 | `/proc/self/pagemap` の匿名ページ PA `0x6853e000`（System RAM） | 同じ oops |

- いずれも `ioctl` の戻り値は得られず、`dmesg` に
  `---[ end trace e740039385d7eab3 ]---`（同 `ab5`, `ab6`）が残った。
- 3 回の oops 後もカーネル・`mirakc`（pid 1613）・`abs`/`adsb` は生存。
  reboot は不要だった。
- `word0=0x10010010` は **送っていない**。送れば同じ oops になる。
- `word2` にベンダ内部の正しいバッファ記述（`xcode_handle+0x4c0` 経由の
  ハンドル）が無い限り、この cmd はユーザ空間から使えない。
  `general_cmd` の設計意図は「ドライバ所有バッファの DMA 指定」であり、
  汎用メッセージ入口ではない。
- 最終版の `t4s-arc` は `word2=0` を **ioctl 前に拒否** する（fail-closed）。
  実行結果: `cmd 0x78 requires a driver-owned physical buffer in word2; ... Refusing to send.`（rc=1）。
  これ以上の 0x78 試行は行っていない。
- 試験中に作った SBUF ring `0x28d66d00`（`0x1000`）は
  `destroy-buffer 0x28d66d00` で `force_reset_buffer` + `destroy_buffer` に
  成功し、graph の `buf[4]` は `h=0x0` に戻った。

### 12.2 `tsi-ex` は成功（1 段変更 → 完全復元）

`t4s-arc tsi-ex 0 0 1`（logical 0 → physical 1、packet 0）:

```
before: 0x6000=0x00000100 0x6008=0x01000000 0x600c=0x00000300 0x61f0=0x00000010
ioctl 0x53 req=[0,0,0,1,0,0,1,0,1,0,1]
ioctl 0x53 rc=0 status=0
after:  0x6000=0x02000100 0x6008=0x03000200 0x600c=0x00000100 0x61f0=0x00000011
restore: 0x53 port1/asi1, then 0x53 port0/asi0
restored: 0x6000=0x00000100 0x6008=0x01000000 0x600c=0x00000300 0x61f0=0x00000010
```

- `0x61f0`: `0x10` → `0x11`（nibble0 が 0→1）→ `0x10`。
- `0x6000/8/c` も保存値へ完全一致。`restore verified` を出力。
- 復元は kernel の `config_tsi` 相当を 0x53 で 2 回（port1/asi1, port0/asi0）
  呼ぶ方式。MMR への直接 write は行っていない。
- `tsi-ex` 実行後の `t4s-probe graph`: `mmr6000=0x100 mmr6008=0x1000000
  mmr600c=0x300 mmr61f0=0x10`、device 11 個（type 0x24 が 2 個、
  type 0x25 が 4 個）は実行前と同じ。TLV ring の追加・消失は無し。
- TLV の実データは取得できていない。4K 選局（TUM `0x6004`）は
  `state/op` 表 `0x395f60` のゲートが閉じたままで、`tlvcap3` は
  タスク指示により使わないため。

### 12.3 実機状態（試験後）

- `mirakc` pid 1613 は生存。`t4s-probe graph` は正常応答。
- `0x61f0` は `0x10`、`0x6000/8/c` は試験前値。
- reboot / mirakc 再起動は行っていない（必要なかった）。
- 0x01 (halt), 0x6001/2, ptrace, abs 停止は行っていない。
