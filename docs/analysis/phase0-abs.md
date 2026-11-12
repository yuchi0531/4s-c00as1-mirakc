# Phase 0 — 衛星 FED 制御の静的解析（abs / xcdrv.ko）

対象: SHARP 4S-C00AS1（ViXS XCode6, ARMv7）。**実機アクセスなし**。ホスト上の取得物のみ。
参照リポジトリ `Karin-Laboratory/mkra_4S-C00AS1` の固定アドレスは**別ビルドのため流用していない**。

| ファイル | 種別 | md5 | 備考 |
|---|---|---|---|
| `.tmp/oem/abs` | ELF32 ARM EXEC, 17,591,960 B | `9fc3e99e628ba2893d3192d0319153e8` | 参照既知 `def4436239553ede33f0febf86eab318` と不一致 |
| `.tmp/oem/xcdrv.ko` | ELF32 ARM ET_REL, 2,078,892 B | `25bc580fb594958c9a7de4d475a4061d` | 参照既知 `3db75958a29ecbb880c5a658584556d3` と不一致 |
| `.tmp/oem/tuner` | ELF32 ARM EXEC, 178,920 B | `8d2ef1dc9bb8af7f92efefdd152e9362` | クロスチェック用。詳細は `phase0-tuner.md` |

## 手法とアドレス規約

- 最終根拠は **REA 6.4.0 (Ghidra 12.1.4)**。`xcdrv.ko` は REL のシンボルと `.rel.data` を併用し、
  `tuner` は既存スナップショット `.tmp/rea-snap-tuner` の関数・疑似コードで照合した。
  `abs` は 17.6 MB のため 1 回の REA import に約 40 分かかる。トリアージ（関数当たり付け）は
  `llvm-objdump --triple=thumbv7-linux-gnueabihf` と PIC オフセット解決で行い、REA の
  バイナリ概要（41,849 procedures / 42,351 strings）でアドレス空間を固定した。
  主要関数 `BhvTvAdsbTuneChangeChannel`（`0xc66a84`）は REA `function` でも疑似コードを取得し、
  §1.2 に要旨を掲載している。
- アドレスはすべて **VA**。file offset は次で換算する。
  - `abs`: `file = VA - 0x8000`（`.text` = `0x49c28`/file `0x41c28`、`.rodata` = `0xce2f20`/file `0xcdaf20`）
  - `tuner`: `.text`/`.rodata` は `file = VA - 0x8000`、`.data` は `file = VA - 0x10000`
  - `xcdrv.ko`: `.text` は `file = VA + 0x34`、`.data` は `file = VA + 0x17b300`（セクション相対 VA）
- 根拠タグ: `[REA]` Ghidra 疑似コード / `[DIS]` objdump / `[REL]` ELF relocation / `[STR]` strings /
  `[M]` 参照資料の実測（本 Phase では再測定していない）。

> **ビルド差の扱い**: 参照 `abs` の `0x56de60` / `0x5c178` / `0x4a9e4` などは本ビルドに存在しない。
> 本ドキュメントのアドレスはすべて本ビルドで取得したもの。参照資料の関数名と本ビルドの関数は、
> 文字列・メッセージ ID・呼び出し形で対応付けた。断定できない箇所は「未確定」と明記する。

---

## 1. 4K 選局の高レベル列（abs → tuner）

### 1.1 全体像

本ビルドでは 4K 選局は **abs の UI/ADSB 層**と **tuner の FEM/FED 層**に分かれている。
abs は「10 word の選局要求」を SysV メッセージで送るだけであり、RF/復調の実 I/O は tuner が行う。

```
[UI/BhvTv]  BhvTvAdsbTuneChangeChannel            abs:0xc66a84
   └─ 要求を global に park + timer 0x12096/100
   └─ 成立時: 10 word body を作る
        └─ vEtmMain+0x308                        abs:0x457858
             └─ eSysSendMessage(qid=0x36F, msg=0x6004, body, 0x28, -1)
                  │
                  ▼  (ADSB comm task; 受信側関数は未確定)
[ADSB/FDC]  eAdsbCommTuneChangeChannel            (文字列のみ。関数未確定)
   └─ fdc_chmCancelSelectChannel / DisConnectTsUnits / SelectChannel …  **未確定**
                  │
                  ▼
[tuner FEM] vFedmMain                             tuner:0xaf28
   ├─ msg 0x2 (ChangeDevice)   … FED へ path 割当
   ├─ msg 0x4 (DisconnectDevice) … path 解放
   └─ msg 0xd (PowerOff)       … FED へ転送
                  │
                  ▼
[tuner FED] vFedAdbsLocalMain                     tuner:0xebe0  (T_FED_ADBS1/2/3)
   ├─ msg 0xc (PowerOn)  → log "TUNER_ADBS1 ON"
   ├─ msg 0xd (PowerOff) → log "TUNER_ADBS1 OFF"
   │     ├─ fFedSetRfStandby(dev,1)               tuner:0x1a310
   │     │     └─ Tune_ISDB_S_RfStandby            tuner:0x20550 (sem 0x3c5)
   │     ├─ fFedSetDemodStandby(dev,1)            tuner:0x1a36c
   │     │     └─ Tune_ISDB_S_OfdmStandby          tuner:0x206b4 (sem 0x3c5)
   │     └─ tuning param clear → eSysSendMessage(0x2a0, …)
   └─ msg 0x2 (SetTuningStart) → vFedAdbsSetTuningStart  tuner:0x10004
         1. fFedIntTaskProgress 待ち（eSysSleepTask(1) ×最大 0x65）
         2. fFedDemodulatorSetTs                    tuner:0x17610
              └─ Tune_ISDB_S3_SetTsid                tuner:0x20b2c  (mode=0xd)
         3. fFedTunerSetFreq                        tuner:0x17ab8
              └─ Tune_ISDB_S3_SetFreq                tuner:0x20f9c
                   └─ eFed_ISDB_S3_TunerSetChannel   tuner:0x1ae50
                        └─ x_tune_isdbs (PLL)        tuner:0x1c500  + usleep 10 ms
         4. fFedTunerSeqStart                       tuner:0x17b68
              └─ Tune_ISDB_S3_SeqStart               tuner:0x21030
         5. status=2 / mask=1 / eSysExSendMessage(0x1c7, 0x2a, path, 0x14, -1)
                  │
                  ▼
[lock 待ち] vFedAdbsLocalMain の割込み/ポーリング
   ├─ Tune_ISDB_S3_IsTLVLocked                       tuner:0x21304  (status 4)
   ├─ eSysSleepTask(10) ×最大 0xc / eSysSleepTask(0x32) ×最大 0x14
   └─ eStmExSetTimerAfter(…, 2000)  → 失効時 "[FEM] ADBS Tuning Timeout"
```

### 1.2 stage 表

| # | stage | 関数 (VA) | 引数 | 実装 / 順序 | delay / timeout | evidence |
|---|---|---|---|---|---|---|
| A1 | UI 要求 | `BhvTvAdsbTuneChangeChannel` abs:`0xc66a84` | `r0`=path/global name, `r1`=Tune (`param_2`), `r2`=`*param` (0x5c B) | `bAdsbInitialized == 0` のときだけ park（`memcpy 0x5c`）し timer `0x12096`/`100` を張って返す。初期化後は Dev を `Tune==6` なら現デバイス、それ以外は `FUN_c6de7c` で求め、`0x200→9, 0x400→10, 0x800→11` に写像して body を組む。 | timer 100（単位未確定） | `[REA] abs:0xc66a84`, `[DIS] 0xc66a84–0xc66d14` |
| A2 | ADSB 送信 | `vEtmMain+0x308` abs:`0x457858` | `r0`=word0(0), `r1`=`*param` | body[0..9] を組み `eSysSendMessage(0x36F, 0x6004, body, 0x28, -1)` | なし | `[DIS]` `0x457858–0x4578f4` |
| A3 | ADSB 受信 | `eAdsbCommTuneChangeChannel` (文字列 abs:`0xddbf8c`) | — | **未確定**（関数未特定） | — | `[STR]` |
| T1 | FEM 振分 | `vFedmMain` tuner:`0xaf28` | msgId / path / body | `msgTuneRequestFemChangeDevice` / `msgTuneRequestFemDisconnectDevice` を参照。msg `0x2a` で 4K timeout timer `0x3b` (2000 ms) を張る。 | 2000 ms | `[REA]` / `[STR]` `0x28b9c`, `0x28bbc` |
| T2 | FED 振分 | `vFedAdbsLocalMain` tuner:`0xebe0` | task 名で unit 0..2 | `T_FED_ADBS1/2/3` を `strcmp`。msg `0xc`=ON log、`0xd`=OFF、`0x2`=TuningStart。 | — | `[REA]` |
| T3 | OFF / standby | `fFedSetRfStandby` tuner:`0x1a310` → `Tune_ISDB_S_RfStandby` tuner:`0x20550` | `(dev, 1)` | sem `0x3c5` → RF IC `x_fin_isdbs` の 7 列 → dev 別 unit 0/1/2 | 明示 delay なし | `[REA]` / `phase0-tuner.md §2.3` |
| T4 | OFF / demod | `fFedSetDemodStandby` tuner:`0x1a36c` → `Tune_ISDB_S_OfdmStandby` tuner:`0x206b4` | `(dev, 1)` | sem `0x3c5` → `ctx+4 = 1`（ISDB-S mode） | 明示 delay なし | `[REA]` |
| T5 | SetTsid | `vFedAdbsSetTuningStart` tuner:`0x10004` → `fFedDemodulatorSetTs` tuner:`0x17610` → `Tune_ISDB_S3_SetTsid` tuner:`0x20b2c` | `(tsid, dev)` | まず `fFedIntTaskProgress` を待つ（最大 `0x65`）。外側の FED sem は `FUN_0001105c(path)` が返す動的 ID、内側は demod sem `0x3c5`。mode=`0xd`。初回のみ `set_system`（regset_ISDBS3 + PSEQ_ISDBS3）。`0x31` mask、`0x2f/0x30` に TSID。 | 待ち `1` tick ×101 | `[REA]` |
| T6 | SetFreq | `fFedTunerSetFreq` tuner:`0x17ab8` → `Tune_ISDB_S3_SetFreq` tuner:`0x20f9c` → `FUN_1ae50` → `x_tune_isdbs` tuner:`0x1c500` | `(if_khz, dev)` | clamp 950,000..3,300,000 kHz。RF IC 15 列。`0x10` 書込み後に **10 ms**。 | `usleep(10*1000)` | `[REA]` / `[DIS]` |
| T7 | SeqStart | `fFedTunerSeqStart` tuner:`0x17b68` → `Tune_ISDB_S3_SeqStart` tuner:`0x21030` | `(dev)` | sem `0x3c5` → `FUN_22880`（demod sequencer） | なし | `[REA]` |
| T8 | 応答 | `vFedAdbsSetTuningStart` | `(path, result)` | `eSysExSendMessage(0x1c7, 0x2a, path, body, 0x14, -1)` | — | `[REA]` |
| T9 | lock | `Tune_ISDB_S3_IsTLVLocked` tuner:`0x21304` | `dev` | `FUN_22eb4(ctx,4)` → `ctx+0x38`（lock flag） | poll 10/50 tick、timer 2000 ms | `[REA]` |

**REA 関数確認（abs:`0xc66a84`, `FUN_00c66a84`）**

REA の疑似コードは次を返した（要旨）。

```
undefined4 FUN_00c66a84(undefined4 path, uint tune, void *param) {
  if (bAdsbInitialized == 0) {
    eGlobalPathName = path; eGlobalTuneType = tune;
    memcpy(tGlobaltTuningInfo, param, 0x5c);
    eStmSetTimerAfter(0x7f, 0x12096, 0, 0, -1, 100);
    eDbgPrintf(0, "[msgRequestUimAdsbTuneChangeChannel]\n");
    return 1;
  }
  ...
  if (tune == 6) { FUN_00500298(1,&dev); map 0x200→9 / 0x400→10 / 0x800→11 }
  else { dev = FUN_00c6de7c(path, *(u32*)(param+0xc), *(u16*)(param+0x18)); }
  ...
  local_38 = dev; local_34 = tune; local_30 = FUN_00c5a098(); // OID
  local_2e = *(u16*)(param+0x14); local_2c = *(u16*)(param+0x18);
  local_28 = *(u32*)(param+0x1c); local_2a = *(u16*)(param+0x20);
  local_24 = FUN_00c69bb0(); // NotDecode
  local_2f = (FUN_004bd978(...) == 1) ? local_c8 : 0; // BID
  return FUN_00457858(0, &local_38); // = vEtmMain+0x308
}
```
`local_38` から始まるスタック構造体を、`vEtmMain+0x308` が 10 word に**再配置**する。
word1 は `+8` の byte（OID）、word2 は `+0` の word（Dev）、word3 は `+4` の word（Tune）、
word4 は `+0xa`（NID）、word5 は `+0xc`（SID）、word6 は `+0x10`（RF）、word7 は `+0xe`（TSID）、
word8 は `+0x14`（NotDecode）、word9 は `+9`（BID）。§1.3 の表と一致する。

### 1.3 `0x6004` body（10 word）の確定レイアウト

`vEtmMain+0x308` の `[DIS]` から確定。`len=0x28`（10 word）。参照 `ABI.md §7.4` と一致する。

| word | body offset | 意味 | 本ビルドでの生成元 | evidence |
|---:|---:|---|---|---|
| 0 | `+0x00` | 常に 0 | `r0` 引数（`BhvTv…` は 0 を渡す） | `[DIS] 0x457876` |
| 1 | `+0x04` | OID（操作連番、8-bit） | `param+0x08` byte（`ldrb`。`FUN_c5a098` の戻り） | `[DIS] 0x45787e` |
| 2 | `+0x08` | Dev（`9`/`0xa`/`0xb`） | `param+0x00` word。`0x200→9, 0x400→10, 0x800→11` | `[DIS] 0x457888`, `0xc66b78–0xc66b90` |
| 3 | `+0x0c` | Tune（`0`/`1`/`3`/`7`） | `param+0x04` word = 元の `r1` | `[DIS] 0x457894`, `0xc66bfc` |
| 4 | `+0x10` | NID（4K は `0x0b`） | `param+0x0a` u16 | `[DIS] 0x45789e`, `0xc66c02` |
| 5 | `+0x14` | SID | `param+0x0c` u16 | `[DIS] 0x4578a6`, `0xc66c0a` |
| 6 | `+0x18` | RF kHz | `param+0x10` word | `[DIS] 0x4578b0`, `0xc66c12` |
| 7 | `+0x1c` | TSID（TLV stream id） | `param+0x0e` u16 | `[DIS] 0x4578ba`, `0xc66c38` |
| 8 | `+0x20` | NotDecode | `param+0x14` word（`FUN_c69bb0` の戻り） | `[DIS] 0x4578c4`, `0xc66c46` |
| 9 | `+0x24` | BID（リモコンキー） | `param+0x09` byte（channel 表比較の戻り） | `[DIS] 0x4578d0`, `0xc66c6c` |

`BhvTvAdsbTuneChangeChannel` の `param+` 側は次のとおり（`arg2` 構造体）。

| offset | 意味 | evidence |
|---:|---|---|
| `+0x14` | NID (u16) | `[DIS] 0xc66c02` |
| `+0x18` | SID (u16) | `[DIS] 0xc66c0a` |
| `+0x1c` | RF kHz (u32) | `[DIS] 0xc66c12` |
| `+0x20` | TSID (u16) | `[DIS] 0xc66c38` |

### 1.4 fdc_chm / FemDisconnectDevice ×3 の扱い

参照 `ABI.md §7.1` の `fdc_chmCancelSelectChannel` / `fdc_chmDisConnectTsUnits` /
`fdc_chmConnectTsUnits` / `fdc_chmSelectChannel` / `FemChangeDevice` / `FemDisconnectDevice ×3` は、
本ビルドの `abs` / `tuner` の dynsym・文字列・REA 関数一覧から**関数を特定できなかった**。
したがって本ドキュメントでは「未確定」とする（§6）。一方、`TUNER_ADBS1 OFF → RfStandby →
OfdmStandby` と `SetTsid → SetFreq → SeqStart → lock` は tuner 側で確認できた。

---

## 2. `xcdrv.ko` の ioctl dispatch 表

### 2.1 二層ディスパッチ（重要）

`/dev/vixs/xcodedrv` の ioctl は 1 層ではない。

1. **要求整形層** `check_ioctl` (`xcdrv:0x17fe8` / REA `0x28554`)
   ```
   cmd = *(u16 *)(req + 0x8bc);
   if (cmd < 0x98 && cmd_check_dispatch[cmd]) return cmd_check_dispatch[cmd](...);
   return -0x3fd; // XCE_NO_SUPPORT
   ```
   `cmd_check_dispatch` は `.data+0x1e0` の 0x98 個（4 B/entry）の関数ポインタ配列。
   各ハンドラは **要求サイズを `req+0x8b8` に `strb` で書き、ユーザ payload を `memcpy`** する。
   これが `docs/ABI.md` の「request length」の根拠である。
2. **実行層** `kdrv_dev_ioctl` (`xcdrv:0x5600`) が `cmd = *(u32 *)(req+0x470)` の下位 16 bit で
   `0x28/0x29/0x2a/0x2b/0x2c/0x68/0x70–0x76/0x82–0x8b` を実行する。
   `0x2a`/`0x2b` は上位 16 bit のサブコマンド（`class*100+index`）で
   `dc_cmd_check_dispatch` / `di_cmd_check_dispatch` に展開される。
3. バッファ/reader/pipeline 系（`0x32` 以降）の要求整形は 1 の層、実行は ARC 側の
   `process_ioctl_request` / `post_process_ioctl` に渡る。

**値渡し**（要求配列ではなく ioctl の第 3 引数そのもの）: `0x29`（device 破棄）、`0x34`（buffer 破棄）、
`0x38`（reader 破棄）は、ハンドラが `*req = arg` を格納して `req+0x8b8 = 4` を書く
（`[REA]` `xc_sdev_destroy_func`, `xc_sbuf_destroy_func`, `xc_sbuf_destroy_read_func`）。
`0x35` の sub 1（set_ready）/ sub 3（force_reset）も値渡しである。

> 既存 `rust/t4s-core/src/xcdrv.rs` の値は**すべて本ビルドと一致**した（下記「一致」列）。
> `xcdrv.rs` の「handler が +0x8b8 にサイズを書く」というコメントも本ビルドで確認できる。

### 2.2 全テーブル（`cmd_check_dispatch`、`.data+0x1e0` / file `0x17b4e0`）

サイズはハンドラの `strb` 即値（`[REA]` で数例を疑似コード確認、残りは `[DIS]`）。
`—` は null スロット。`sub` はサブコマンド表（§2.3）。

| cmd | size | 要求整形ハンドラ | 既存 `xcdrv.rs` |
|---:|---:|---|---|
| `0x00` | — | null | — |
| `0x01` | 4 | `xc_halt_cpu_func` | — |
| `0x02` | 12 | `xc_internal_cmd_func` | — |
| `0x03` | 12 | `xc_get_secure_mem_func` | — |
| `0x04` | 40 | `xc_profiling_data_func` | — |
| `0x05` | 4 | `xc_kick_msg_queue_func` | — |
| `0x06`–`0x07` | — | null | — |
| `0x08` | 16 | `xc_log_info_func` | — |
| `0x09` | 0 | `xc_log_reload_func` | — |
| `0x0a` | 12 | `xc_get_dev_func` | — |
| `0x0b` | 68 | `xc_res_query_func` | — |
| `0x0c` | 12 | `xc_get_buf_func` | — |
| `0x0d` | 32 | `xc_mmfb_phy_alloc_func` | — |
| `0x0e` | 4 | `xc_mmfb_phy_free_func` | — |
| `0x0f`–`0x10` | 16 | `xc_mem_sync_func` | — |
| `0x11` | 4 | `xc_mmfb_base_func` | — |
| `0x12` | — | null | — |
| `0x13` | 96 | `xc_res_add_desc_func` | — |
| `0x14` | 4 | `xc_res_remove_desc_func` | — |
| `0x15` | 90 | `xc_xcode_info_func` | — |
| `0x16` | 0 | `xc_reset_func` | — |
| `0x17` | 4 | `xc_status_func` | — |
| `0x18` | 4 | `xc_mmfb_base_func` | — |
| `0x19` | 4 | `xc_mmr_base_func` | `MMR_BASE` 一致 |
| `0x1a` | 16 | `xc_mmfb_alloc_func` | — |
| `0x1b` | 4 | `xc_mmfb_free_func` | — |
| `0x1c` | 24 | `xc_mmfb_info_func` | — |
| `0x1d` | 16 | `xc_mem_alloc_func` | — |
| `0x1e` | 4 | `xc_mem_free_func` | — |
| `0x1f` | 28 | `xc_dma_mem_to_mem_func` | — |
| `0x20`–`0x21` | 16 | `xc_dma_in_func` | — |
| `0x22`–`0x24` | — | null | — |
| `0x25` | 15 | `xc_smartcard_util_func` | — |
| `0x26` | — | null | — |
| `0x27` | 9 | `xc_bit_pattern_search` | — |
| `0x28` | 24 | `xc_sdev_create_func` | `SDEV_CREATE` 一致 |
| `0x29` | 4 | `xc_sdev_destroy_func` | `SDEV_DESTROY` 一致 |
| `0x2a` | sub | `xc_sdev_ctrl_func` → `dc_cmd_check_dispatch` | `0x…002A` 群 一致 |
| `0x2b` | sub | `xc_sdev_info_func` → `di_cmd_check_dispatch` | `SDEV_GET_STATUS` 一致 |
| `0x2c` | 8 | `xc_cancel_func` | — |
| `0x2d` | 24 | `xc_sdev_create_ex_func` | — |
| `0x2e`–`0x31` | — | null | — |
| `0x32` | 36 | `xc_sbuf_create_func` | `SBUF_CREATE` 一致 |
| `0x33` | 28 | `xc_sbuf_create_umc_func` | — |
| `0x34` | 4 | `xc_sbuf_destroy_func` | `SBUF_DESTROY` 一致 |
| `0x35` | sub | `xc_sbuf_ctrl_func` → `bc_cmd_check_dispatch` | `SET_READY`/`FORCE_RESET` 一致 |
| `0x36` | sub | `xc_sbuf_info_func` → `bi_cmd_check_dispatch` | `SBUF_INFO` 一致 |
| `0x37` | 8 | `xc_sbuf_create_read_func` | `SBUF_CREATE_READ` 一致 |
| `0x38` | 4 | `xc_sbuf_destroy_read_func` | `SBUF_DESTROY_READ` 一致（値渡し） |
| `0x39` | 8 | `xc_sbuf_create_write_func` | — |
| `0x3a` | 4 | `xc_sbuf_destroy_write_func` | — |
| `0x3b` | 40 | `xc_sbuf_get_read_func` | `SBUF_GET_READ` 一致 |
| `0x3c` | 16 | `xc_sbuf_commit_read_func` | `SBUF_COMMIT_READ` 一致 |
| `0x3d` | 44 | `xc_sbuf_get_write_func` | — |
| `0x3e` | 24 | `xc_sbuf_commit_write_func` | — |
| `0x3f` | 8 | `xc_cancel_func` | `SBUF_CANCEL_GET` 一致 |
| `0x40` | 56 | `xc_sbuf_yuv_create_func` | — |
| `0x41`–`0x42` | 8 | `xc_sbuf_yuv_ref_func` | — |
| `0x43` | 72 | `xc_sbuf_yuv_create_ex_func` | — |
| `0x44` | 8 | `xc_sbuf_create_read_func` | — |
| `0x45` | 8 | `xc_sbuf_create_write_func` | — |
| `0x46` | 4 | `xc_pipeline_create_func` | `PIPELINE_CREATE` 一致 |
| `0x47` | 4 | `xc_pipeline_destroy_func` | `PIPELINE_DESTROY` 一致 |
| `0x48` | 12 | `xc_pipeline_time_func` | `PIPELINE_GET_TIME` 一致 |
| `0x49` | 24 | `xc_pipeline_adjust_func` | — |
| `0x4a` | 32 | `xc_pipeline_tolerance_func` | — |
| `0x4b` | 8 | `xc_pipeline_sync_func` | — |
| `0x4c` | 24 | `xc_pipeline_config_func` | — |
| `0x4d` | 68 | `xc_pipeline_coeff_func` | — |
| `0x4e` | 8 | `xc_pipeline_enable_func` | — |
| `0x4f` | — | null | — |
| `0x50` | 40 | `xc_tsi_config_func` | `TSI_CONFIG` 一致 |
| `0x51` | 36 | `xc_tsi_status_func` | `TSI_STATUS` 一致 |
| `0x52` | 4 | `xc_rmx_config_func` | — |
| `0x53` | 44 | `xc_tsi_config_ex_func` | — |
| `0x54`–`0x59` | — | null | — |
| `0x5a` | 76 | `xc_crypto_util_func` | — |
| `0x5b` | 不定 | `xc_crypto_dma_func` | — |
| `0x5c` | 16 | `xc_crypto_keystore_func` | — |
| `0x5d` | 4 | `xc_crypto_keystore_size_func` | — |
| `0x5e` | 28 | `xc_crypto_dma_s2p_register_func` | — |
| `0x5f` | 3 | `xc_crypto_dma_s2p_run_func` | — |
| `0x60` | 28 | `xc_crypto_dma_s2p_unregister_func` | — |
| `0x61`–`0x63` | — | null | — |
| `0x64` | 16 | `xc_temperature_func` | `GET_TEMPERATURE` 一致 |
| `0x65` | 12 | `xc_mod_power_switch_func` | — |
| `0x66` | 36 | `xc_sdev_open_res_func` | — |
| `0x67` | 4 | `xc_sdev_close_res_func` | — |
| `0x68` | 0 | `xc_cancel_ioctls_func` | — |
| `0x69`–`0x6d` | — | null | — |
| `0x6e` | 12 | `xc_otp_block_read_func` | — |
| `0x6f` | 40 | `xc_sbuf_create_ex_func` | — |
| `0x70` | 4 | `xc_acb_create_func` | — |
| `0x71` | 4 | `xc_acb_destroy_func` | — |
| `0x72` | 12 | `xc_acb_connect_input_func` | — |
| `0x73` | 16 | `xc_acb_disconnect_input_func` | — |
| `0x74`–`0x75` | — | null | — |
| `0x76` | 12 | `xc_acb_switch_func` | — |
| `0x77` | — | null | — |
| `0x78` | 12 | `xc_cust_msg_cmd_func` | — |
| `0x79` | — | null | — |
| `0x7a` | 12 | `xc_mem_map_func` | `MEM_MAP` 一致 |
| `0x7b` | 8 | `xc_mem_unmap_func` | `MEM_UNMAP` 一致 |
| `0x7c` | 12 | `xc_secure_mem_func` | — |
| `0x7d`–`0x81` | — | null | — |
| `0x82` | 36 | `xc_host_dev_get_device_type` | — |
| `0x83` | 40 | `xc_host_dev_put_device_event` | — |
| `0x84` | 20 | `xc_host_dev_put_device_done` | — |
| `0x85` | 76 | `xc_get_pending_queue_func` | — |
| `0x86` | 32 | `xc_get_graph_func` | `GET_GRAPH` 一致 |
| `0x87` | 36 | `xc_host_dev_add_device_name` | — |
| `0x88` | 4 | `xc_host_dev_del_device_type` | — |
| `0x89` | 36 | `xc_host_dev_get_device_callback` | — |
| `0x8a` | 36 | `xc_host_dev_put_device_callback` | — |
| `0x8b` | 8 | `xc_host_dev_set_device_cb_mask` | — |
| `0x8c` | 8 | `xc_pipeline_attach_func` | — |
| `0x8d` | 8 | `xc_pipeline_detach_func` | — |
| `0x8e`–`0x90` | 4 | `xc_pipeline_start_stop_pause_func` | `PIPELINE_STOP=0x8f` 一致 |
| `0x91` | 12 | `xc_pipeline_sync_ts_func` | — |
| `0x92` | 8 | `xc_pipeline_set_speed_func` | — |
| `0x93` | 8 | `xc_pipeline_get_speed_func` | — |
| `0x94` | 12 | `xc_pipeline_fb_register_func` | — |
| `0x95` | 12 | `xc_pipeline_fb_unregister_func` | — |
| `0x96` | 8 | `xc_pipeline_fb_reset_func` | — |
| `0x97` | 24 | `xc_pipeline_fb_ts_func` | — |

### 2.3 サブコマンド表

#### `0x2A` device ctrl（`class = (cmd >> 16) / 100`, `index = (cmd >> 16) % 100`）

class 0（generic device, sub `0x0000..0x0019`）:

| index | 実行ハンドラ | size | 意味 / 既存 |
|---:|---|---:|---|
| 0 | null | — | — |
| 1–3 | `xc_sdev_ctrl_set_state_func` | 12 | run/stop/pause（`0x0001` run, `0x0003` stop） |
| 4 | null | — | — |
| 5 | `xc_sdev_ctrl_deinit_func` | 4 | — |
| 6 | `xc_sdev_ctrl_drop_data_func` | 8 | — |
| 7–8 | `xc_sdev_ctrl_connect_func` | 16 | `0x0007` connect_input, `0x0008` connect_output |
| 9 | `xc_sdev_ctrl_disconnect_func` | 16 | `0x0009` disconnect |
| 10 | `xc_sdev_ctrl_attach_control_func` | 16 | — |
| 11 | `xc_sdev_ctrl_detach_control_func` | 16 | — |
| 12 | `xc_sdev_ctrl_set_pipeline_func` | 8 | `0x000C` set_pipeline |
| 17 | `xc_sdev_ctrl_queue_event_func` | 12 | — |
| 24 | `xc_sdev_ctrl_connect_crossbar_func` | — | — |
| 25 | `xc_sdev_ctrl_disconnect_crossbar_func` | — | — |

class 5（TSI / DEMUX, sub `0x01F4..`）:

| index | 実行ハンドラ | size | 既存 |
|---:|---|---:|---|
| 0 | `xc_sdev_ctrl_tsi_func` | — | `SDEV_CONFIG_TSI = 0x01F4` 一致 |
| 1 | `xc_sdev_ctrl_add_pid_filter_func` | 36 / 24 | `DEMUX_ADD_PID_FILTER = 0x01F5` 一致 |
| 2 | `xc_sdev_ctrl_remove_pid_filter_func` | 12 | `0x01F6` 一致 |
| 5 | `xc_sdev_ctrl_conf_demux_func` | 8 | — |
| 8 | `xc_sdev_ctrl_add_remap_pid_filter_func` | 52 | — |
| 9 | `xc_sdev_ctrl_add_pid_latch_func` | 24 | — |
| 10 | `xc_sdev_ctrl_add_pid_filter_ext_func` | 48 / 36 | — |
| 11 | `xc_sdev_ctrl_set_bcas_card_id_func` | 10 | — |

class 11（REMUX, sub `0x044C..0x0459`）— `ABI.md §2.1` と全一致:

| index | sub | 実行ハンドラ | size |
|---:|---:|---|---:|
| 0 | `0x044C` | `xc_sdev_ctrl_conf_remux_func` | 28 |
| 1 | `0x044D` | `xc_sdev_ctrl_remux_add_prog_func` | 44 |
| 2 | `0x044E` | `xc_sdev_ctrl_remux_del_prog_func` | 8 |
| 3 | `0x044F` | `xc_sdev_ctrl_remux_add_video_func` | 28 |
| 4 | `0x0450` | `xc_sdev_ctrl_remux_add_audio_func` | 28 |
| 5 | `0x0451` | `xc_sdev_ctrl_remux_add_data_func` | 32 |
| 6 | `0x0452` | `xc_sdev_ctrl_remux_add_psi_func` | 20 |
| 7 | `0x0453` | `xc_sdev_ctrl_remux_add_sys_func` | 24 |
| 8 | `0x0454` | `xc_sdev_ctrl_remux_del_pid_func` | 12 |
| 9 | `0x0455` | `xc_sdev_ctrl_remux_ats_base_func` | 16 |
| 10 | `0x0456` | `xc_sdev_ctrl_remux_config_video_func` | 36 |
| 11 | `0x0457` | `xc_sdev_ctrl_remux_config_audio_func` | 20 |
| 12 | `0x0458` | `xc_sdev_ctrl_remux_config_data_func` | 20 |
| 13 | `0x0459` | `xc_sdev_ctrl_remux_set_copy_permission` | 8 |

class 26（MMT_DEMUX, sub `0x0A28..`）:

| index | sub | 実行ハンドラ | size |
|---:|---:|---|---:|
| 0 | `0x0A28` | `xc_sdev_ctrl_mmt_demux_config_func` | 不定 |
| 1 | `0x0A29` | `xc_sdev_ctrl_tlv_add_pid_filter_func` | 64 |
| 2 | `0x0A2A` | `xc_sdev_ctrl_remove_pid_filter_func` | 12 |
| 3 | `0x0A2B` | `xc_sdev_ctrl_add_mmtp_filter_func` | 32 |
| 5 | `0x0A2D` | `xc_sdev_ctrl_set_mtp_packet_id_func` | 8 |
| 6 | `0x0A2E` | `xc_sdev_ctrl_si_filtering_func` | 12 |
| 7 | `0x0A2F` | `xc_sdev_ctrl_mmtp_filtering_func` | 16 |
| 8 | `0x0A30` | `xc_sdev_ctrl_adsb_set_keystore_func` | 12 |
| 9 | `0x0A31` | `xc_sdev_ctrl_tlv_dev_config_func` | 8 |

#### `0x2B` device info（`di_cmd_check_dispatch`, class 5 など）

| class | index | 実行ハンドラ | size |
|---:|---:|---|---:|
| 0 | 0 | null | — |
| 0 | 1 | `xc_sdev_info_state_func` | 8 |
| 0 | 2 | `xc_sdev_info_connect_func` | — |
| 0 | 3 | `xc_sdev_info_get_event_func` | — |
| 5 | 0 | `xc_sdev_info_tsi_func` | 16 |
| 5 | 1 | `xc_sdev_info_pid_filter_func` | 16 |
| 5 | 2 | `xc_sdev_info_tlv_pid_filter_func` | 16 |
| 5 | 3 | `xc_sdev_info_mmt_pid_filter_func` | 16 |
| 5 | 4 | `xc_sdev_info_leap_second_func` | 16 |

`0x0001002B` = class 0 / index 1 = `xc_sdev_info_state_func`（`SDEV_GET_STATUS` と一致）。

#### `0x35` buffer ctrl（`bc_cmd_check_dispatch`, 7 entries）

| index | ハンドラ | size | 既存 |
|---:|---|---:|---|
| 0 | null | — | — |
| 1 | `xc_sbuf_ctrl_ready_func` | 4 | `0x00010035` SET_READY 一致 |
| 2 | `xc_sbuf_ctrl_reset_func` | 8 | `0x00030035` FORCE_RESET 一致 |
| 3 | `xc_sbuf_ctrl_reset_func` | 8 | — |
| 4 | `xc_sdev_ctrl_queue_event_func` | 12 | — |
| 5 | `xc_sbuf_ctrl_set_threshold_func` | 16 | — |
| 6 | `xc_sbuf_ctrl_set_limit_func` | 20 | — |

#### `0x36` buffer info（`bi_cmd_check_dispatch`, 5 entries）

| index | ハンドラ | size | 既存 |
|---:|---|---:|---|
| 0 | null | — | — |
| 1 | `xc_sbuf_info_get_status_func` | 48 | `0x00010036` GET_STATUS 一致 |
| 2 | `xc_sbuf_info_get_reader_func` | 8+4n（可変） | `0x00020036` GET_READER。**`access_ok` なし** |
| 3 | `xc_sbuf_info_get_event_func` | 28 | — |
| 4 | `xc_sbuf_info_get_last_released_chunk_func` | — | — |

### 2.4 `0x2A` の上位 16 bit

`xc_sdev_ctrl_func` の疑似コード（`[REA]`）:
```
sub = *(u32 *)(p+0x8bc) >> 16;
class = sub / 100; index = sub % 100;
if (class < 0x1b && index < dc[class].count && dc[class].handlers[index])
    return dc[class].handlers[index](p, ...);
return -0x3fd;
```
`dc_cmd_check_dispatch` は 27 entry の `{count, handlers*}`。`di` も同じ。`bc`/`bi` は
単純な関数ポインタ配列（`bc` 7、`bi` 5）。

---

## 3. I2C 書き込みヘルパ（tuner 側）

本ビルドの衛星 I2C は **tuner** が持つ。`abs` も `libi2c.so` の `I2c_Init/Write/Read` を
import し、`i2c write Error(%02X/%02X/%02X)`（abs VA `0xd63694` / file `0xd5b694`）などの
文字列を持つが、それは別用途（パネル/周辺 IC）と推定され、衛星 RF/復調の列ではない。
衛星 RF/復調のレジスタ列は tuner の以下の関数にある。詳細な全レジスタ列は
`docs/analysis/phase0-tuner.md §1–§3` を参照（本節は要点と根拠のみ）。

### 3.1 トランスポート

| 対象 | 7-bit slave | tuner が `I2c_Write` に渡す値 | 根拠 |
|---|---|---|---|
| RF IC (CXD2868) unit0/1/2 | `0x60`/`0x61`/`0x62` | **8-bit** `0xc0`/`0xc2`/`0xc4` | `FUN_0001c964` tuner:`0x1c964` → `FUN_0001ca3c` `0x1ca3c` |
| SC1501A page 0..8 | `0x10`..`0x18` | 7-bit を `<<1` して 8-bit | `FUN_0001e020` tuner:`0x1e020` / `FUN_0001dfc4` `0x1dfc4` |
| unit offset | unit1 `+0x20`, unit2 `+0x40` | `(unit&7)*0x20 + 0x10 + page` | `FUN_00025f60` `0x25f60` |

- RF: `FUN_0001ca3c` は `I2c_Write(bus=0, slave=param2, reg=param3, nsub=1, data, len)`。
  `param2` は `0xc0/0xc2/0xc4`。第1引数 `0xdc` は未使用（デッド）。
- 復調: `FUN_0001e020` は `addr7 << 1` を `I2c_Write` に渡す。maskwrite は
  `new = (old | (value & mask)) & (value | ~mask)`（`FUN_0001e084` `0x1e084`）。
- **page/bank 切替**は bank レジスタ書込みではなく、**I2C slave アドレス `0x10+page` の切替**である。
  `regset_*` の triple `(page, reg, value)` を `0x10+page` に書く（`FUN_00025f60`）。

### 3.2 書き込みの分割

- `FUN_0001c934` は 1 byte、`FUN_0001c964` は 2/4/6 byte を **1 回の `I2c_Write`** で送る。
  byte 単位に分割するコードはない。最大は RF PLL の 6 byte（`0x06`）と
  復調 regset の 3 byte（`0x92` 等は 1 byte）。
- TSID は `0x2f`（high）と `0x30`（low）の 2 回、別々の `write`（`FUN_00022d4c` case `0x2d`）。

### 3.3 待ち時間 / poll の根拠

| 箇所 | 値 | 根拠 |
|---|---|---|
| RF PLL `0x10` 書込み後 | `FUN_0001cba0(10)` = `usleep(10*1000)` = **10 ms** | `[REA]` `0x1cba0`、呼び出し `0x1c760` |
| ISDB-T power-on `0x9b` 後 | 10 ms（`FUN_0001df6c(10)`） | `phase0-tuner.md §5` |
| SetTsid / SetFreq / SeqStart | semaphore `0x3c5`、明示 delay なし | `[REA]` `0x1ef5a`, `0x1f1ae`, `0x1f25c` |
| RF PLL | semaphore `0x3c6` | `[REA]` `0x1c50c` |
| lock poll | `eSysSleepTask(10)` ×最大 `0xc`、または `eSysSleepTask(0x32)` ×最大 `0x14` | `[REA]` `vFedAdbsLocalMain` の lock 分岐 |
| 4K tune timeout | `eStmExSetTimerAfter(…, 2000)` | `[REA]` `vFedmMain` / `vFedAdbsLocalMain` |
| FEM 進捗待ち | `eSysSleepTask(1)` ×最大 `0x65` | `[REA]` `vFedAdbsSetTuningStart` `0x10004` |

`eSysSleepTask` の tick→ms は **未確定**（libsysapi.so 未取得）。`2000` は timer 引数が ms である前提。

---

## 4. TSID / mode 設定順序と定数

### 4.1 mode

`Tune_ISDB_S3_SetTsid` / `Tune_ISDB_S_SetTsid` は復調 ctx の `+0x04` に system を書く。

| mode | 意味 | 使用関数 | 根拠 |
|---:|---|---|---|
| `1` | ISDB-S (2K) | `Tune_ISDB_S_SetTsid` tuner:`0x1ef4c` | `[REA]` |
| `0xd` | ISDB-S3 / TLV (4K) | `Tune_ISDB_S3_SetTsid` tuner:`0x20b2c` | `[REA]` |

`FUN_00022434`（set_system）は system 1 で `regset_ISDBS`、system `0xd` で
`regset_ISDBS3` + `regset_PSEQ_ISDBS3` を適用する（`[REA]`）。初回だけ
`regset_Recommend_DIFF` も適用する。

### 4.2 SetTsid の順序（4K）

```
Tune_ISDB_S3_SetTsid(dev, tsid):
  1. take sem 0x3c5
  2. ctx+4 = 0xD
  3. (初回) set_system(ISDBS3)  … regset_ISDBS3 + PSEQ_ISDBS3
  4. ctx[unit][0x30a] = 0
  5. DMD_set_tsout(system=0xD)
  6. Demod set2(cmd=0x18, tsid):
      0x31 mask: bit3=0  (絶対 TSID 選択)
      0x2f = tsid >> 8
      0x30 = tsid & 0xff
  7. give sem 0x3c5
```
`FUN_00022d4c` の case `0x2d` は 2K 用（`0x31` mask, `0x2f`, `0x30`）。本ビルドの 4K は
`FUN_00022be4(local_18, 0x18, tsid)` を呼ぶ（`[REA]` `0x20b2c`）。

### 4.3 定数

| 定数 | 値 | 根拠 |
|---|---:|---|
| IF clamp (ISDB-S3) | 950,000 .. 3,300,000 kHz | `[REA]` `FUN_0001ae50` |
| IF clamp (ISDB-S 2K) | 950,000 .. 2,150,000 kHz (`0x20ce70`) | `[REA]` `FUN_0001ad80` |
| LNB LO | 10,678,000 kHz | `[M]` / `phase0-tuner.md §6` |
| ADC clock | 25,000,000 Hz（delay ではない） | `phase0-tuner.md §5` |
| RF PLL delay | 10 ms | `[REA]` `0x1cba0` |
| 4K tune timeout | 2000 ms | `[REA]` `vFedmMain` |
| lock poll | 10 tick ×12 / 50 tick ×20 | `[REA]` `vFedAdbsLocalMain` |
| FEM progress wait | 1 tick ×101 | `[REA]` `0x10004` |
| sem (demod) | `0x3c5` | `[REA]` |
| sem (RF) | `0x3c6` | `[REA]` |

### 4.4 リトライ / タイムアウト

| 対象 | 値 | 根拠 |
|---|---|---|
| 2K `SetFreq` 失敗時 | `eSysSleepTask(0x1e)` 後に **1 回だけ再試行** | `phase0-tuner.md §4`（tuner:`0x148a8–0x148b4`） |
| BS サーチ再選局 | timer **100 ms** | `phase0-tuner.md §4`（`vFedLocalMain` msg `0x39`） |
| 4K ADBS tune | timer **2000 ms**。失効時 `[FEM] ADBS Tuning Timeout` | `[REA]` `vFedmMain` / `vFedAdbsLocalMain` |
| 4K の SetTsid/SetFreq 再試行 | **未確定**（4K 側に 2K と同じ明示再試行は見つからない） | `[REA]` |
| abs の Dev==0 失敗時 | `FUN_c60e2c(path,1,0x13c)` と `FUN_c34150(path,2,0x1a0)` を呼ぶ。`0x13c`/`0x1a0` の意味（timer/msg）は未確定 | `[REA]` `abs:0xc66bce–0xc66be2` |
| lock poll | `10` tick ×最大 `0xc` / `0x32` tick ×最大 `0x14` | `[REA]` `vFedAdbsLocalMain` |

### 4.5 abs 側の要求定数

| 定数 | 値 | 根拠 |
|---|---:|---|
| ADSB 通信 qid | `0x36F` (879) | `[DIS] 0x4578da` |
| 選局要求 msgId | `0x6004` | `[DIS] 0x4578de` |
| body 長 | `0x28` (10 word) | `[DIS] 0x4578e4` |
| 関連 msgId | `0x6005` (3 word), `0x6006` (2 word) | `[DIS] 0x457934`, `0x457976` |
| timer event | `0x12096` / 100 | `[DIS] 0xc66ad2`, `0xc66aca` |
| park 長 | `0x5c` | `[DIS] 0xc66abc` |
| FED 応答 qid | `0x1C7` (455) | `[REA]` `0x10004` 末尾 |

---

## 5. 根拠アドレス一覧

### 5.1 abs

| ラベル | VA | file offset | 根拠 |
|---|---:|---:|---|
| `BhvTvAdsbTuneChangeChannel` | `0xc66a84` | `0xc5ea84` | `[DIS]` function start `push {r4,r5,r6,r7,lr}` / `sub sp,#0xfc` |
| `vEtmMain` | `0x457550` | `0x44f550` | `.dynsym` (`readelf -sW`) |
| `vEtmMain+0x308`（0x6004 送信） | `0x457858` | `0x44f858` | `[DIS]` `eSysSendMessage` call at `0x4578e6` |
| 文字列 `[BhvTv]BhvTvAdsbTuneChangeChannel(...)` | `0xf90208` | `0xe88208` | `[STR]` |
| 文字列 `BhvTvAdsbTuneChangeChannel Err[%d][%d]` | `0xf9023c` | `0xe8823c` | `[STR]` |
| 文字列 `...OID(...) Dev(...) Tune(...)` | `0xf90264` | `0xe88264` | `[STR]` |
| 文字列 `eAdsbCommTuneChangeChannel` | `0xddbf8c` | `0xdd3f8c` | `[STR]`（関数未特定） |
| 文字列 `[msgRequestUimAdsbTuneChangeChannel]` | `0xf901e0` | `0xe881e0` | `[STR]` |
| 文字列 `gAdsbCommApi` | `0x25efc` | `0x1defc` | `[STR]`（`.bss` の API 構造体名。実体 VA は別） |
| 変数 `bAdsbInitialized` | `0x02027184` | —（`.bss`） | `[DYN]` / `[REA]` `BhvTvAdsbTuneChangeChannel` の分岐 |

### 5.2 tuner（クロスチェック）

| ラベル | VA | file offset | 根拠 |
|---|---:|---:|---|
| `vFedmMain` | `0xaf28` | `0x2f28` | `[REA]` / `.dynsym` |
| `vFedAdbsLocalMain` | `0xebe0` | `0x6be0` | `[REA]` |
| `vFedAdbsSetTuningStart` (`FUN_00010004`) | `0x10004` | `0x8004` | `[REA]` |
| `fFedDemodulatorSetTs` (`0x17610`) | `0x17610` | `0xf610` | `[REA]` |
| `Tune_ISDB_S3_SetTsid` | `0x20b2c` | `0x18b2c` | `[REA]` |
| `Tune_ISDB_S_SetTsid` | `0x1ef4c` | `0x16f4c` | `[REA]` |
| `fFedTunerSetFreq` | `0x17ab8` | `0xfab8` | `[REA]` |
| `Tune_ISDB_S3_SetFreq` | `0x20f9c` | `0x18f9c` | `[REA]` |
| `eFed_ISDB_S3_TunerSetChannel` | `0x1ae50` | `0x12e50` | `[REA]` |
| `x_tune_isdbs`（PLL） | `0x1c500` | `0x14500` | `[REA]` |
| `write_tuner`（I2C） | `0x1ca3c` | `0x14a3c` | `[REA]` |
| RF unit→addr | `0x1c964` | `0x14964` | `[REA]` |
| `usleep(ms)` | `0x1cba0` | `0x14ba0` | `[REA]` |
| `fFedTunerSeqStart` | `0x17b68` | `0xfb68` | `[REA]` |
| `Tune_ISDB_S3_SeqStart` | `0x21030` | `0x19030` | `[REA]` |
| `Tune_ISDB_S_RfStandby` | `0x20550` | `0x18550` | `[REA]` |
| `Tune_ISDB_S_OfdmStandby` | `0x206b4` | `0x186b4` | `[REA]` |
| `fFedSetRfStandby` | `0x1a310` | `0x12310` | `[REA]` |
| `fFedSetDemodStandby` | `0x1a36c` | `0x1236c` | `[REA]` |
| `Tune_ISDB_S3_IsTLVLocked` | `0x21304` | `0x19304` | `[REA]` |
| `regset_ISDBS3` | `0x36e6c` | `0x26e6c` | `[REL]/[STR]` `.data` symbol |
| `regset_PSEQ_ISDBS3` | `0x38cf0` | `0x28cf0` | 同上 |
| `regset_ISDBS` | `0x36c70` | `0x26c70` | 同上 |
| 文字列 `[FEDADBS]TUNER_ADBS1 ON` | `0x29374` | `0x21374` | `[STR]` |
| 文字列 `[FEDADBS]TUNER_ADBS1 OFF` | `0x293ec` | `0x213ec` | `[STR]`。xref は関数内に解決できず |
| 文字列 `[FEM] ADBS Tuning Timeout` | `0x28fdc` | `0x20fdc` | `[STR]`。`vFedmMain` 内 xref `0xe12a` |
| 文字列 `msgTuneRequestFemChangeDevice` | `0x28b9c` | `0x20b9c` | `[STR]`。xref `vFedmMain 0xb784` |
| 文字列 `msgTuneRequestFemDisconnectDevice` | `0x28bbc` | `0x20bbc` | 同上 |

### 5.3 xcdrv.ko

| ラベル | VA | file offset | 根拠 |
|---|---:|---:|---|
| `cmd_check_dispatch` (0x98 × 4 B) | `0x1e0` | `0x17b4e0` | `.rel.data` + `.data` |
| `dc_cmd_check_dispatch` (27 × 8 B) | `0x108` | `0x17b408` | `.rel.data` |
| `di_cmd_check_dispatch` | `0x30` | `0x17b330` | `.rel.data` |
| `bc_cmd_check_dispatch` (7 × 4 B) | `0x14` | `0x17b314` | `.rel.data` |
| `bi_cmd_check_dispatch` (5 × 4 B) | `0x0` | `0x17b300` | `.rel.data` |
| `check_ioctl` | `0x17fe8` | `0x1801c` | `[REA]`（REA addr `0x28554`） |
| `kdrv_dev_ioctl` | `0x5600` | `0x5634` | `[REA]`（REA addr `0x15b6c`） |
| `xcode_ioctl` | `0x3149c` | `0x314d0` | `[REA]` / `[DIS]` |
| `xc_sdev_create_func` | `0x17c98` | `0x17ccc` | `[REA]`（size 0x18） |
| `xc_sbuf_create_func` | `0x15288` | `0x152bc` | `[REA]`（size 0x24） |
| `xc_sbuf_get_read_func` | `0x17b2c` | `0x17b60` | `[REA]`（size 0x28） |
| `xc_sdev_ctrl_func` | `0x15500` | `0x15534` | `[REA]` |
| `xc_sdev_info_func` | `0x1538c` | `0x153c0` | `[REA]` |
| `xc_sbuf_ctrl_func` | `0x150c8` | `0x150fc` | `[REA]` |
| `xc_sbuf_info_func` | `0x15014` | `0x15048` | `[REA]` |

> `xcdrv` の REA アドレスは本ビルドのセクション相対 VA に `+0x1056c` した値で表示される
> （例: `check_ioctl` VA `0x17fe8` → REA `0x28554`）。ドキュメントでは REA 表示と VA を併記した。

---

## 6. 未確定項目

1. **`fdc_chmCancelSelectChannel` / `fdc_chmDisConnectTsUnits` / `fdc_chmConnectTsUnits` /
   `fdc_chmSelectChannel` の関数**。本ビルドの `abs`/`tuner` の dynsym・文字列・REA 関数一覧に
   該当名がない。参照 `ABI.md §7.1` の列は、別バイナリ（`libtuner.so`/`libadsb` 等）または
   name-stripped の内部関数である可能性が高い。
2. **`FemDisconnectDevice ×3` の実装と 3 回の順序**。`vFedmMain` が
   `msgTuneRequestFemDisconnectDevice` を参照することは確認したが、3 回呼ぶ制御列は未特定。
3. **abs `0x6004` の受信側**。`0x36F` のキュー所有者（abs 内 ADSB task か、別プロセスか）と、
   受信後の `fdc_chm*` への分配は未確定。`eAdsbCommTuneChangeChannel` は文字列のみ。
4. **`bAdsbInitialized` の初期化タイミング**。`bAdsbInitialized == 0` の間は要求を park して
   timer `0x12096`/`100` を張るだけで、`0x6004` を送らない。誰が `bAdsbInitialized` を 1 にするか
   （ADSB 初期化完了通知か）は未確定。送信トリガ自体は「初期化後」で確定した（§1.2 の REA 疑似コード）。
5. **timer `0x12096` / `100` の単位**。`eStmSetTimerAfter` の引数規約（ms か tick か）は
   libsysapi.so 未取得のため未確定。`100` は ms と推定。
6. **`eSysSleepTask` の tick→ms**。tuner 側の `0x1`/`0x32`/`0x65`/`0x1e` は tick と確認したが、
   実時間は未確定。
7. **mode `0xd` の `DMD_set_tsout` / `SeqStart` の全レジスタ**。`regset_ISDBS3` と
   `PSEQ_ISDBS3` の triple は取得済みだが、4K の `DMD_set_tsout` と `SeqStart` の
   条件分岐（system `0xd`）の全列は `phase0-tuner.md` の 2K 側と別に未抽出。
8. **`abs` の `FUN_c5a098` / `FUN_c69bb0` の意味**。OID と NotDecode/BID の生成元。
   関数本体は未解析（Phase 1 で `FUN` 単位の REA が必要）。
9. **`gAdsbCommApi` の実体**。`.bss` の 84 B 構造体で、初期化は実行時。関数ポインタ表の
   静的抽出はできない。
10. **`0x6005` / `0x6006` の用途**。`vEtmMain+0x308` の近傍で送信されるが、4K 選局列での
    使用有無は未確定。
11. **TUNER_ADBS1 OFF 後の `eSysSendMessage(0x2a0, …)`** の宛先 qid `0x2a0` の所有者。
12. **本ビルドの `abs` が full strip か**。`.dynsym` に一部の関数名が残る（`vEtmMain`,
    `vFedmMain` 等）。完全 stripped ではないため、記号由来の断定は dynsym の範囲に限る。

---

## 7. Phase 1 前に必要な動的計測（実機 I/O は Phase 1 以降）

1. **msqid 解決**: `/tmp/bdh_out_msgh.lst` から `0x36F`(879)、`0x1C7`(455)、`0x2a0`(672) の
   実 msqid を取得。起動ごとに変わるため毎回読む。
2. **`0x6004` の受信タスク特定**: `msgrcv` の parked 引数（`/proc/<tid>/syscall`）と
   受信バッファ（`/proc/<pid>/mem`）で 10 word を採取し、body の word2 (Dev) / word3 (Tune) /
   word4..9 が §1.3 と一致するか確認。
3. **送信トリガ**: `BhvTvAdsbTuneChangeChannel` を 1 回目/2 回目で呼び、timer `0x12096` の
   発火で `0x6004` が出るか、UI の再要求で出るかを切り分ける。
4. **`0x6004` から tuner msg `0x2` への変換**: abs 内で `0x6004` を受けた後、
   `eSysSendMessage(0x1c7, 0x2, …)` が出るか（qid/msgId）をトレース。
   ここが埋まると `fdc_chm*` の位置も絞れる。
5. **`fdc_chm*` の探索**: `abs` の ADSB task（`vAdsb*`）の関数一覧から、
   TS unit 接続/切断を行う関数を `eSysSendMessage`/`xc_*` 呼び出しで絞る。
   `0x6004` 受信関数の call graph を Phase 1 で REA 解析する。
6. **timer 単位**: `eStmSetTimerAfter`/`eStmExSetTimerAfter` の引数を
   `strace` ではなく製品ログ（`eDbgPrintf`）の経過時刻で測り、`0x12096` の 100 が ms か tick か確定。
7. **tick 実時間**: `eSysSleepTask(0x32)` 周期の処理（`vFedLocalSubAPoll`）をログ時刻で測る。
8. **lock 時間**: `Tune_ISDB_S3_IsTLVLocked` の `ctx+0x38` を read-only で 5–10 ms 間隔に
   monitor し、SeqStart から lock までの分布と 2000 ms timeout の余裕を確認。
9. **OFF 列の実測**: `msg 0xd` 送信後に RF `0x60` の `x_fin_isdbs` 7 列と
   復調 `0x10..0x18` の `OfdmStandby` が観測されるか。`FemDisconnectDevice ×3` の順序もここで確認。
10. **HDMI/デコード依存**: 4K は `MMT_DEMUX` の ES リングが HDMI 接続時のみ作られる
    （`docs/4K-NO-HDMI.md`）。Phase 0 の選局列が「選局成立」と「ES リング生成」の
    どちらまでを保証するかを切り分ける。
