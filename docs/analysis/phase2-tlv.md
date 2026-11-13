# Phase 2 — BS4K/TLV 受信経路の静的解析（REA）

対象: SHARP 4S-C00AS1（ViXS XCode6, ARMv7）。
**実機アクセス・書込み・再起動は行っていない。ホスト上の取得物のみ。コード変更なし（doc のみ）。**

| ファイル | md5 | 用途 |
|---|---|---|
| `.tmp/oem/adsb` | `49e8d2c4…` | TLV demux/MMT demux グラフ構築の中枢 |
| `.tmp/oem/abs` | `9fc3e99e…` | UI/TSD/SFD・`bAdsbInitialized`・`0x6004` |
| `.tmp/oem/tuner` | `8d2ef1dc…` | SC1501A 復調の TLV 出力設定・lock |
| `.tmp/oem/xcdrv.ko` | `25bc580f…` | ioctl サブコマンド表・要求長・check 関数 |
| `.tmp/oem/libvixs.so` | `80f28d4a…` | デバイス種別表・media/subtype 表・ioctl 符号化 |
| `.tmp/oem/libxcodeapi.so` | `493a14…` | `xc_mini_device_*` の薄いラッパ |

アドレス規約（本ビルドで実測）:

- `abs`: `.text`/`.rodata` は **file = VA − 0x8000**、`.got`/`.data`/`.bss` は **file = VA − 0x10000**。
- `adsb`: `.text`/`.rodata` は **file = VA − 0x8000**、RW 側は **file = VA − 0x10000**。
- `tuner`: `.text` は **file = VA − 0x8000**、RW 側は **file = VA − 0x10000**。
- `libvixs.so`: `.text`/`.rodata` は **file = VA**、RW 側は **file = VA − 0x8000**。
- `xcdrv.ko` (ET_REL): `.text` は **file = VA + 0x34**、`.data` は **file = 0x17b300 + offset**。

根拠タグ: `[R]` REA/Ghidra 擬似コード、`[D]` llvm-objdump、`[SYM]` シンボル、
`[GOT]` .got+reloc、`[TAB]` libvixs の表、`[STR]` 文字列、`[INF]` 推論。

---

## 0. 結論サマリ

1. **`wrap_demuxInit`（adsb:0x4e308）の ADSB 分岐は「TLV デバイス（type 0x24）を
   入力 0・出力 8 で生成 → `0xA31002A` で TLV dev config → 4 MiB ring を 4 本
   → 各 ring を個別の MMT_DEMUX（type 0x25, 入力 1・出力 256）へ connect_input
   → ring set_ready → MMT に queue_event → 最後に TLV に set_clock_owner」**である。
   4 本の ring は 1 個の MMT demux の 4 出力ではなく、**4 個の独立した MMT demux**。
2. TLV デバイスは **入力ポート 0 個**で生成される。`connect_input` は TLV には
   一切発行されない（adsb 全体で TLV→MMT の 1 回だけ）。したがって TLV デバイスは
   **バッファ/TSI 入力ではなく、チップ内 TLV キャプチャ（4K 復調の 1-bit serial TLV）を
   暗黙の入力とする source デバイス**である。`0xA31002A` の引数にも入力選択は無い。
3. `0xA31002A` は **SDEV_CTRL の class 26 / index 9**（`sub = 26*100+9 = 0xA31`）。
   要求は **8 バイト `{u32 tlv_handle, u32 recorder_mode}`**。
   mode は `psfc_fed_AdsbGetRecorderMode()` の戻りで、`0 = RECODER`、`1 = TUNER BOX`。
   kernel 側 check 関数は要求長 8 を設定し、管理バイト（0x8b9–0x8bb / 0x8d8–0x8db）を
   クリアするだけで、実処理は ARC ファームウェア側（クリア対象の意味は未確定）。
4. **`bAdsbInitialized` の実アドレスは `0x02027184` で正しい。**
   `abs:0xc669c6` のストア（関数 `abs:0xc669b4`）と `abs:0xc66a9e` の比較は同じ GOT スロット
   `0xff7f5c`（値 `0x02027184`）を引く。このアドレスは abs の RW LOAD
   `[0xfd1000, 0x11403ba0)` 内（`.bss` 内）であり、「マップ外」ではない。
5. **`0x6004` が park されるのは abs の UI 関数 `BhvTvAdsbTuneChangeChannel`
   （0xc66a84）内の `*bAdsbInitialized == 0` のときだけ。**
   一方 adsb の `0x6004` ハンドラ（`vAdsbCommMain` → 0x117c4 → 0x10860）は
   このフラグを見ない。**0x36F に直接 `msgsnd` すれば park を迂回できる。**
6. 4K 選局時の復調側 TLV 出力は `tuner` の `DMD_set_tsout`（0x26d74）が
   system 0xD 分岐で **`regset_TLV_SPI_1BIT`（0x3b3cc）＋ bank0 0xcc の bit6** を
   設定する。ただし 2K 側は同じ 0xcc の **bit7** を設定し、互いにクリアしない。
   **2K 選局歴のある個体では 0xcc が 0xc0（TS+TLV 同時）になり得る**。
   既存の `gr_i2c::select_tlv_output_exclusive()` はこの bit7 を落として 0x40 にする。
7. HDMI 無しでも TLV は取得できる。`wrap_demuxInit` は MMT demux の **出力を
   一切接続しない**（ES ring/decoder は HDMI/AV 経路が別途作る）。TLV 取得は
   TLV デバイス＋自前 ring だけで完結する。
8. 前回の「追加 `connect_output` で 0 バイト」は、リング形状の不一致
   （`LINEAR_RING`/subtype 0）と、**4K 選局・TSin routing・0xcc の TLV exclusive 化**
   が未成立であることが有力候補。リングは製品と同じ
   **`{3,1,9,1,0x1000,0x400000,1,3,0}`（RING/SYSTEM/MMTP, 4 MiB, 1 unit=1 byte）**で
   作り、`tlv_score`（0x7F 同期）で判定する。

---

## 1. `wrap_demuxInit` の完全列

### 1.1 呼び出し元とライフサイクル

- `wrap_demuxInit`（adsb:0x4e308 / file 0x46308）は `adsb` プロセスの初期化関数
  `0x48b60` から **1 回だけ**呼ばれる（`adsb:0x48c08` の `bl 0x4e308`）[D]。
- 先頭で `DAT_0031cdb8 != 0` なら `0x3e9` を返す（二重初期化ガード）[R]。
- 成功時は最後に `DAT_0031cdb8 = 1` を立てる [R]。
- つまり **TLV/MMT グラフは adsb 起動時に一度だけ構築され、選局ごとには作り直されない**
  （選局で作り直されるのは MMT フィルタ/ES ring 側）[R]/[STR: "rebuilding the MMT filters and ES rings"]。

設定テーブルは `FUN_0005a278(&local_112c)` が埋める 0x44 バイト×N の配列で、
各エントリの `+0x8`（word index 2）が入力種別:

| 入力種別 | 分岐 | 内容 |
|---:|---|---|
| 0 | `0x4e502` 系 | MPEG2 tuner（DEMUX type 0x0C + TSI） |
| 1 | `0x4e5xx` 系（`0x4e66e`） | MPEG2 file（LOCAL_DEMUX type 0x06 + 独自 ring） |
| 2 | **`0x4e72a`** | **ADSB tuner（TLV type 0x24）** |

（この個体は type 2 エントリが存在するため、起動時に TLV デバイスと 4 ring が作られる。）

### 1.2 ADSB（TLV）分岐の実行列（`0x4e72a`–`0x4ecae`）

`FUN_00048094(cmd, buf)` は `ioctl(g_xcode_fd, cmd, buf)` の薄いラッパ
（`adsb:0x48094`、`ldr r3,[pc,#0x2e0]; ldr r0,[r3]; blx ioctl@plt`）[D]。
以下の buf はすべて 4 バイト word の配列。

#### (1) TLV デバイス生成 — `SDEV_CREATE_EX` 0x2D、24 B

`adsb:0x4e734–0x4e752`:

```
buf[0] = 0x24        // device type: TLV
buf[1] = 0          // n_input = 0   （2K DEMUX=1 / MMT=1 との対比）
buf[2] = 8          // n_output = 8  （2K DEMUX=0x100 / MMT=0x100 との対比）
buf[3] = 0
buf[4] = 0
buf[5] = out        // ← 成功時、ここに handle が返る
ioctl(0x2D, buf)
```

> word[1]/word[2] の「入力数/出力数」という名前は、同じ関数内の
> 2K DEMUX `{0x0C,1,0x100}`・MMT `{0x25,1,0x100}` と、それぞれが
> `config_tsi`/`connect_input` を持つ/持たない関係からの推定 [INF]。

- handle は **word[5]**（+0x14）。`libvixs:xcode_mini_device_create_ex @0x89cd4` も
  `str.w r9,[r1,#0x14]` で同じ位置に書く [D]。
- kernel check `xc_sdev_create_ex_func`（xcdrv:0x15674 / file 0x156a8）は
  要求長 `0x18` を `sdev+0x8b8` に設定する [D]。
- 2K 分岐の `SDEV_CREATE` 0x28 は **word[4]** が handle（`0x4e502–0x4e526`）[R]/[D]。
  **0x2D と 0x28 で handle の位置が違う**ので混同しないこと。

#### (2) TLV デバイス設定 — `0xA31002A`、8 B

`adsb:0x4ead4–0x4eafe`:

```
mode = psfc_fed_AdsbGetRecorderMode();
buf[0] = tlv_handle;          // word[5] of create
buf[1] = (mode == 0) ? 0 : 1; // 0=RECODER, 1=TUNER BOX
ioctl(0xA31002A, buf)         // 8 bytes のみ有効
```

- `mode == 0` のとき `str r3(0),[r8,#4]`（`0x4eaf0`）、`mode != 0` のとき
  `str r3(1),[r8,#4]`（`0x4ece6`）[D]。
- 失敗時はエラーログ `XC_SDEV_CTRL_TLV_DEV_CONFIG error!!`（`0x4ed2a` 付近）[STR]。
- 詳細は §3。

#### (3) 4 MiB ring ×4 — `SBUF_CREATE` 0x32、36 B

`adsb:0x4eb1e–0x4eb7c`（ループ本体）。buf は 9 word:

```
buf = { 3, 1, 9, 1, 0x1000, 0x400000, 1, 1, out }
       type  media subtype max_reader align  size   unit flags  handle(+0x20)
ioctl(0x32, buf)
```

- `type = 3` = RING [TAB: s_buffer_type_str]。
- `media = 1` = SYSTEM [TAB: s_media_type_str]。
- `subtype = 9` = **MMTP** [TAB: s_media_subtype_str, media 1 の index 9]。
- `size = 0x400000` は **units**、`unit = 1` なので **4 MiB**。
- `max_reader = 1`。ring 1 本につき reader 枠は 1（製品は MMT demux の
  connect_input で消費）。
- `flags = 1`。製品 ring は host aperture 外（`xc_map` 不可）。
- ループは `piVar4 += 9` で `__s + 0x24` まで = **4 回** [R]/[D]。
- kernel check `xc_sbuf_create_func`（xcdrv:0x15288）は要求長 `0x24` を設定 [D]。

#### (4) TLV 出力 → ring — `CONNECT_OUTPUT` 0x8002A、16 B

`adsb:0x4eb92–0x4ebb0`:

```
buf[0] = tlv_handle
buf[1] = 0
buf[2] = ring_handle
buf[3] = 0        // ← 成功時、接続 handle（または割当情報）が返る
ioctl(0x8002A, buf)
```

- 4 回とも `buf[1] = 0`。製品コードに他の値は無い。word[1] が「出力ポート」
  なのか「bind/予約」なのかは ARC 側の意味が未取得（§13）。
  いずれにせよ **製品と同じ形は `{dev,0,ring,0}`**。
- kernel check `xc_sdev_ctrl_connect_func`（xcdrv:0x14a14）は要求長 16 を設定し、
  payload は読まない（ARC へ転送）[D]。
- 返った word[3] はレコードに保存されるが、製品は以後使わない [R]。

#### (5) MMT_DEMUX 生成 — `SDEV_CREATE_EX` 0x2D、24 B

`adsb:0x4ebb8–0x4ebf2`:

```
buf = { 0x25, 1, 0x100, 0, 0, out }   // type=MMT_DEMUX, n_input=1, n_output=256
ioctl(0x2D, buf)
```

- handle は word[5]。[TAB: g_DeviceTypeStr index 37 = "MMT_DEMUX"]。

#### (6) ring → MMT — `CONNECT_INPUT` 0x7002A、16 B

`adsb:0x4ec08–0x4ec28`:

```
buf[0] = mmt_handle
buf[1] = 0
buf[2] = ring_handle
buf[3] = 0        // 成功時 connection handle
ioctl(0x7002A, buf)
```

- ここで ring の `max_reader = 1` が消費される。

#### (7) ring ready — `SBUF_CTRL` 0x00010035（値渡し）

`adsb:0x4ec48`:

```
ioctl(0x10035, ring_handle)   // 第2引数は handle 値そのもの（ポインタではない）
```

- kernel check `xc_sbuf_ctrl_ready_func`（xcdrv:0x16300）は要求長 4 を設定 [D]。

#### (8) MMT へ event — `SDEV_CTRL` 0x11002A（class 0 idx 17 = queue_event）

`adsb:0x4ec62–0x4ec76`:

```
buf = { mmt_handle, 0xC0000 }
ioctl(0x11002A, buf)
```

- 0xC0000 の意味は未確定（バッファ ready/入力有効化系と推定）[INF]。
- ※ Phase 1 の「`0xD002A` QUEUE_EVENT」は誤り。**0xD002A は set_clock_owner**、
  0x11002A が queue_event（`dc_cmd_check_dispatch` class 0 idx 17）。

#### (9) ループ後の確定処理

`adsb:0x4ec96` / `0x4ecae`:

```
FUN_00048a98(tlv_handle)         // adsb .bss 0x13ff78 に handle を保存
buf = { tlv_handle, 1 }
ioctl(0xD002A, buf)              // class 0 idx 13 = set_clock_owner → TLV を起動
```

- `FUN_00048a98`（0x48a98）は `ldr r2,=0x13ff70; str r3,[r2,#8]` のみ [D]。
  このグローバルは他関数から参照が見つからない（デバッグ/管理用と推定）。
- set_clock_owner が **TLV デバイスの「run」に相当**する。製品は起動時に実行済み。

### 1.3 対比: 2K 分岐（type 0）は TSI を明示的に結ぶ

`adsb:0x4e502–0x4ea1c` [R]/[D]:

```
create 0x28 {0x0C, 1, 0x100, 0, 0, out@word4}   // DEMUX, n_input=1, n_output=256
ioctl(0x50, tsi_cfg[10])                        // TSI_CONFIG (40 B)
ioctl(0x1F4002A, {demux, 0, port, -1})          // SDEV_CTRL class5 idx0 = TSI (16 B)
ioctl(0xD002A, {demux, 1})                      // set_clock_owner
ioctl(0x7002A, {demux, 0, ring, 0})              // connect_input（別 ring）
ioctl(0x10035, ring)
```

- **DEMUX（0x0C）は n_input=1 で、TSI を `0x50` + `0x1F4002A` で結ぶ。**
- **TLV（0x24）は n_input=0 で、`0x50`/`0x1F4002A`/`connect_input` を一切発行しない。**
  この対比が「TLV 入力は HW 内部」であることの最も強い静的根拠。

### 1.4 TLV 以外の入力分岐（参考）

- type 1（MPEG2 file）: `LOCAL_DEMUX` type 0x06 を生成し、`SBUF_CREATE`
  `{3,1,2,...}`（SYSTEM/TS）と `SBUF_CREATE_WRITE` 0x39 で ring を結ぶ
  （`0x4e66e` 付近）[R]。
- これらの ring は media/subtype が `SYSTEM/TS`。TLV だけが `SYSTEM/MMTP`。

---

## 2. デバイス種別と media/subtype の確定

### 2.1 `libvixs.so:g_DeviceTypeStr`（0x18cae4、38 エントリ）

`readelf -sW` の size 152 = 38 × 4 バイト（**文字列ポインタの配列**）[SYM]/[TAB]:

| index | 名前 | index | 名前 |
|---:|---|---:|---|
| 6 | `LOCAL_DEMUX` | 19 | `TS_RECODER` |
| 12 | `DEMUX` | 36 | **`TLV`** |
| 13 | `DVO` | 37 | **`MMT_DEMUX`** |
| 14 | `REMUX` | 34 | `ROTATOR` |

- adsb の `0x24` = **TLV**、`0x25` = **MMT_DEMUX**、`0x0C` = DEMUX、`0x13` = TS_RECODER。
- したがって前回の「物理 TSI0 を 130 B にして TS_RECODER(0x13) 出力を 130 B ring へ」
  は **TLV 経路ではない**。0x13 は TS 専用デバイスで、130 B framing を拒否した
  `XCE_INVAL_PARAM` はこの型違いと整合する。

### 2.2 media/subtype（`s_media_type_str` 0x18ca68 / `s_media_subtype_str` 0x18c6a8）

- media: `0 UNKNOWN, 1 SYSTEM, 2 DATA, 3 VIDEO_ES, 4 VIDEO_PES, …` [TAB]
- media 1 (SYSTEM) の subtype 9 = **MMTP** [TAB]
- 製品 TLV ring = `{type=3 RING, media=1 SYSTEM, subtype=9 MMTP}`。
  これは「TLV パケット（MMTP ペイロード）を運ぶ ring」であることを表す。
- 既存 `t4s-probe` の `create_capture_linear_buffer` は
  `{4 LINEAR_RING, 1 SYSTEM, 0 UNKNOWN, …}`。**subtype が製品と不一致**（§8）。

---

## 3. `0xA31002A` の意味と再現列

### 3.1 ioctl 符号化（確定）

`libvixs:xcode_mini_device_control @0x7cbf4`:

```
cmd = (sub << 16) | 0x2A
```

`xcdrv:xc_sdev_ctrl_func @0x15500`:

```
sub   = cmd >> 16
class = sub / 100
index = sub % 100
table = dc_cmd_check_dispatch[class]   // .data 0x108, 27 entries of {count, funcs}
if index < count: call funcs[index](sdev, payload, size)
```

- `0xA31002A` → `sub = 0xA31 = 2609 = 26*100 + 9` → **class 26, index 9**。
- `dc_cmd_check_dispatch[26]`（`.data` 0x4f0、count 10）の index 9 =
  **`xc_sdev_ctrl_tlv_dev_config_func`（xcdrv:0x14120 / file 0x14154）** [SYM]/[D]。

### 3.2 kernel check 関数の実体

`xc_sdev_ctrl_tlv_dev_config_func` は payload を読まず、次だけを行う [D]:

```
sdev->[0x8b8] = 8          // 要求長 = 8 バイト
sdev->[0x8b9..0x8bb] = 0   // 管理バイトのクリア
sdev->[0x8d8..0x8db] = 0
return 0
```

- 実処理は ARC ファームウェア側（ホスト取得物には無い）。
- 要求長 8 から、有効な引数は **word[0]=handle, word[1]=mode** の 2 語のみ。

### 3.3 mode の意味

`adsb` の唯一の実呼び出し（`0x4ead4`）は `psfc_fed_AdsbGetRecorderMode()` の戻りで
分岐する [R]/[STR]:

| 戻り | ログ | buf[1] | 解釈（推定） |
|---:|---|---:|---|
| 0 | `psfc_fed_AdsbGetRecorderMode 0(RECODER)` | 0 | 内蔵録画（TLV→4 ring→MMT） |
| 1 | `psfc_fed_AdsbGetRecorderMode 1(TUNER BOX)` | 1 | チューナーボックス（外部配信向け） |

- この個体は 4 ring が実在するので **mode=0 で成功した**と読める
  （mode に関係なく ring を作るコードだが、ring が生きている以上 config 自体は成功）。
- mode 1 の具体的な出力先（ホスト/ネットワーク）は ARC 側で未取得。[INF]

### 3.4 外部から同じ構成を再現する最小 ioctl 列

「既存の製品 TLV デバイスに追加タップする」場合は (1)(2) を **再実行しない**
（(2) は sdev の管理バイトをクリアするので録画中は危険）。新規に自前デバイスを
作る場合の完全列:

```
1. ioctl(0x2D, {0x24, 0, 8, 0, 0, &handle})           // SDEV_CREATE_EX, handle=word[5]
2. ioctl(0xA31002A, {handle, 0})                       // TLV dev config, 8 B (mode 0)
3. ioctl(0x32, {3, 1, 9, 1, 0x1000, 0x400000, 1, 3, 0})
                                                       // RING/SYSTEM/MMTP 4MiB, flags=3
4. ioctl(0x8002A, {handle, 0, ring, 0})                // connect_output
5. ioctl(0x10035, ring)                                // set_ready (値渡し)
6. reader = ioctl(0x37, {ring, 0})                      // create_reader (max_reader=1)
7. loop: ioctl(0x3B, get_read{reader,wait,timeout}) → xc_map(phys) → read
         ioctl(0x3C, commit_read{reader,addr,size,desc})
8. ioctl(0xD002A, {handle, 1})                         // set_clock_owner（自前生成時のみ）
```

- MMT demux（0x25）は **ES を得たい場合だけ**必要。生 TLV だけなら作らない。
- 4 本目の ring まで作る必要はない。1 本で足りる。
- flags は製品 ring が 1、host から `xc_map` するなら 3（既存コメント:
  「flags=3 の buffer だけが host aperture に載る」）。

---

## 4. 入力接続の正体と TSin routing

### 4.1 TLV デバイスに入力は無い（確定）

- 生成要求 `{type=0x24, n_input=0, n_output=8}`（§1.2 (1)）。
- `adsb` 全体で `0x24` に対する `connect_input` は **0 回**。
  `0x7002A` は「TLV→MMT」の 1 回（と 2K 分岐の別 ring）だけ。
- `adsb` は `xc_mini_device_config_tsi` / `setup_tsi_port` / `xcode_tsi_config` を
  **import も呼び出しもしない**（`readelf` の UND に無い）[SYM]。
- `0xA31002A` の引数にも入力ポート/TSI 情報は無い。
- `libvixs` の入力種別表 `g_InputType`（0x190800）にも `TLV` 入力種別は無い
  （`TV_TUNER=6, TS=7, FILE=8, …`）[TAB]。

→ **TLV デバイスは XCode6 の TLV キャプチャブロックを暗黙の入力とする
source デバイス**。ホストから buffer/TSI を `connect_input` する対象ではない。
（チップ内で TSI を 130 B に設定するか、専用 TLV 入力ピンかを区別する情報は
ホスト取得物には無い。§13）

### 4.2 ボード内 TSin routing は adsb/SFD 側

- `abs` には `Sfd_TsdSetTsIn : can not send messace`（VA 0xd1af20）と
  `[TSD]SetTsIn[%d:%d]`（0xd14524）、`TSin xc5 0..3`（0xd1bb40..）がある [STR]。
- `Sfd_TsdSetTsIn` の実体は **qid 0x52 へ msg 0x1e、0x1c バイト**を送る
  `abs:0x1a63a8`（file 0x19e3a8）[D]。3 引数を受け、0x1c バイトの body を
  組み立てて `eSysSendMessage(qid=0x52, msg=0x1e, body, 0x1c, -1)` を呼ぶ。
  body の詳細レイアウトは未確定（§13）。

- `abs` の SFD タスク `vSfdXci_DoSecFilCallback` が 0x1a63a8 を呼ぶ（0x1aec9a）[D]。
- `adsb` 側にも `[FSDTV] DBG SetTSPort[%d]`（VA 0xd9b0c）と
  `[ADSB] eAdsbSfmGetPlt: illeagal eTsInNo`（VA 0xde340）がある [STR]。
  `adsb:0x186e4` が `SetTSPort(idx)` を行い、`0x7a2f8` / `0x79838`
  （`transportSetTsdInputSource` 系）を呼ぶ [D]。
- したがって **物理 4K 復調の TLV 出力を XCode6 の TLV 入力へ繋ぐのは
  adsb/SFD のミドルウェア処理**であり、`tuner` の FED 選局だけでは完結しない。

### 4.3 FED 直接選局（`FedTuneRequest::advanced_bs`）の限界

- `tuner` の `Tune_ISDB_S3_SetTsid` は復調を S3 モードにし、TLV 1-bit 出力を
  有効化する（§5）。`Tune_ISDB_S3_IsTLVLocked` は復調の lock を返す。
- しかし **TSin routing（SFD/TSD）と TLV デバイスの設定は tuner バイナリの
  仕事ではない**。FED を直接叩く実験では、復調ピンに TLV が出ても
  XCode6 の TLV デバイスには届かない可能性が高い。
- 既存 `tuner.rs::change_channel`（0x6004 → qid 0x36F）は製品の
  TUM/SFM 経路を通るので、**TSin routing・MMT フィルタ再構築まで製品に任せられる**。
  こちらが本命。

---

## 5. 4K 復調側の TLV 出力（tuner）

### 5.1 `DMD_set_tsout`（0x26d74 / file 0x1ed74）

`tuner` の擬似コード/逆アセンブリ [R]/[D]:

```
if (ctx->system <= 1) {                    // 2K ISDB-S
    maskwrite(page0 0x0b, bit0, enable?1:0);
    maskwrite(page0 0xcc, 0x80, 0x80);     // bit7 = legacy TS output
} else if (ctx->system == 0xD) {           // ISDB-S3 (4K)
    if (ctx->[0xc28] & 4)
        write_regset(0x3b41c);             // 別 mode
    else
        write_regset(0x3b3cc);             // regset_TLV_SPI_1BIT
    maskwrite(page0 0xcc, 0x40, 0x40);     // bit6 = TLV output
}
```

- `0x3b3cc` は GOT スロット `tuner:0x36308` 経由で引く（VA=file でない RW 側は
  file = VA−0x10000、`0x3b3cc` の file は `0x2b3cc`）[GOT]。
- **0xcc は mask write**。2K は bit7 を、S3 は bit6 をセットし、**互いをクリアしない**
  （`0x26e0a` / `0x26e6c`）[D]。
  → 2K 選局の後で 4K を選ぶと **0xcc = 0xc0**（TS+TLV 同時）になり得る。
  既存 `gr_i2c::select_tlv_output_exclusive` のコメント/実測と一致。
- `ctx->[0xc28] & 4 == 0` が TLV 1-bit を選ぶ条件。`Tune_ISDB_S3_SetTsid` は
  選局時にこの word を 0 にしている（`0x20cb4`）ので、通常は TLV 側。

### 5.2 `Tune_ISDB_S3_SetTsid`（0x20b2c）と `IsTLVLocked`（0x21304）

- `0x20b2c`: ctx を dev (0x200/0x400/0x800) で選び、
  `ctx[0xc28 + idx] = 0`（`0x20cb4`）→ `ctx[0xc68] = 1`（`0x20cc4`）
  → `DMD_set_tsout(ctx)`（`0x20cca`）→ `Tune_ISDB_S3_SetTsid` 本体
  （`0x18`=TSID を page2 `0x5d/0x5c` へ、`0x20cf8`）[D]。
- `0x21304`: `ctx[4] = 0xD` の後 `FUN_00022eb4(ctx, 4)`（status read）を呼び、
  `ctx[14] == 0` なら **1（locked）**、非 0 なら **2（error）**を返す [D]。
- 期待する復調 register（read-only 検証用）:

| bank:reg | 期待値 | 根拠 |
|---|---|---|
| 0:0xcc | **0x40**（TLV only） | `DMD_set_tsout` S3、既存実測「known-good 4K box = 0x40」 |
| 0:0x0b | 0x00 | regset_TLV_SPI_1BIT |
| 0:0x0c | 0x00 | 同上 |
| 0:0x13 | 0x04 | 同上 |
| 0:0x16 | 0x21 | 同上 |
| 0:0x18 | 0x00 | 同上 |
| 0:0xd9 | 0x00 | 同上 |
| 7:0x0a | 0x00 | 同上 |
| 7:0x72 | 0x02 | 同上 |
| 7:0x73 | 0x03（実測）/ 0x15（regset 生値） | `gr_i2c::TLV_SPI_1BIT_TSOUT` |
| 7:0x75 | 0x01 | 実測 |
| 7:0x7b | 0x67 | 実測 |
| 7:0x7d | 0x81 | 実測 |
| 7:0x8d | 0x00 | regset |

- 0x73 の 0x03/0x15 の差は regset の mask 適用（triple の生値 vs 実測）と推定。
  判定には実測列（`TLV_SPI_1BIT_TSOUT`）を使う。

---

## 6. `bAdsbInitialized` の正しい実アドレスと park 条件

### 6.1 実アドレス = `0x02027184`（Phase 1 の値を維持）

`abs` の RW LOAD は `VirtAddr 0xfd1000, MemSiz 0x10432ba0`
→ マップ範囲 **`[0x00fd1000, 0x11403ba0)`**（readelf -lW）[D]。
`.bss` は `VA 0x010d6840, size 0x1032d360` → `[0x010d6840, 0x113560a0)` [D]。
`0x02027184` はこの範囲内（.bss）にある。**「0x8000–0x10d7000 の外」という前提は
RW LOAD の MemSiz を見落としている。**

`[GOT]` の証明:

- `abs .got` のスロット **`0xff7f5c`**（file `0xfe7f5c`）の値 = **`0x02027184`**。
- 書き込み: 関数 `abs:0xc669b4` の `0xc669bc–0xc669c6`。
  `r2 = pc + 0x390252` で GOT base `0xff6c14` を作り、
  `r3 = [r2 + 0x1348]`（= slot `0xff7f5c`）、`*r3 = 1`。
  `str r2,[r3]` は **0xc669c6**。
  この関数は ReportInitialized ケース `0xc36b38` から直接呼ばれる。
- 読み出し: `BhvTvAdsbTuneChangeChannel`（0xc66a84）の
  `0xc66a9a–0xc66aa2` が同じ GOT オフセット `0x1348` を引き、`cmp r3,#0`。

→ **ストアとチェックが同一スロットを指す。アドレスは確定。**

### 6.2 0x6004 の park 条件

`abs:BhvTvAdsbTuneChangeChannel`（0xc66a84 / file 0xc5ea84）[D]:

```
if (*bAdsbInitialized == 0) {
    store pending dev/arg;
    memcpy(0x5c bytes of param);
    eStmSetTimerAfter(group=0x7f, event=0x12096, 0,0, timeout=-1, arg=100);
    return 1;                        // park（送信しない）
}
... 通常の 0x6004 送信（0x457858 = vEtmMain+0x308） ...
```

- park は **UI 層だけ**の挙動。`bAdsbInitialized` が 1 ならそのまま送信。
- `msgAdsbCommReportInitialized` の受信ケース（`abs:0xc36b2a`、文字列
  `[BhvTvProcessProc]msgAdsbCommReportInitialized` @0xf89c00）は
  `0xc669b4` を呼び、フラグを 1 にした後 `0x4578fc` で **msg 0x6005（12 B）**を
  qid 0x36F へ送る。0x6004 の再送はこの関数では行わない（UI の再操作か
  timer 0x12096 側）。
- **adsb の 0x6004 ハンドラはフラグを見ない**:
  `vAdsbCommMain` dispatch で 0x6004 → 0x1012c → 0x117c4
  （`eAdsbCommTuneChangeChannel`）→ 0x10860 → `eSysExSendMessage(qid 0x371, msg 0x5004)`
  → tuner。abs を迂回して qid 0x36F に直接 0x6004 を送れば park を回避できる。
- 製品の 10-word body（既存 `tuner.rs::ChangeChannelRequest::body()` と一致）[D]:
  `{0, oid, 9, tune, 0x0b, sid, rf_khz, tlv_stream_id, not_decode, bid}`。
  `0x6004` は qid **0x36F**、長さ 0x28。実 msqid は `/tmp/bdh_out_msgh.lst`
  （既存 `tuner::resolve_msqid`）で解決する。

---

## 7. HDMI 無しで TLV だけを動かす

- `wrap_demuxInit` は MMT demux（0x25）の **出力を一切接続しない**。
  4 ring は MMT の入力にだけ繋がる。映像/音声 ES ring・decoder は
  `adsb:0x5a5a2` 系など **AV/HDMI 経路が別に作る**（本 doc の範囲外）。
- したがって **HDMI が無くても TLV デバイス＋ring は動く**。TLV 取得に
  ES デコードは不要。MMT demux を作る必要もない。
- 取得判定は既存 `graph::tlv_score`（0x7F 同期、type 0x01–0x03/0xFE/0xFF、
  16-bit BE length）を使う。0x7F が連続するかを確認する。

---

## 8. 「追加 connect_output が 0 バイト」の原因候補（優先順）

| # | 候補 | 根拠 | 次の判定 |
|---:|---|---|---|
| 1 | **4K 選局が成立していない**（lock していない/TSin routing 未設定） | TLV source は復調の TLV 出力を暗黙入力にする（§4） | 製品経路で 0x6004 → bank2 TSID/lock を read |
| 2 | **0xcc が 0xc0**（2K の legacy TS 出力が残る） | `DMD_set_tsout` は bit7/bit6 を互いにクリアしない（§5.1） | bank0 0xcc を read。0xc0 なら `select_tlv_output_exclusive` |
| 3 | **ring の media/subtype が違う** | 製品 ring は `SYSTEM/MMTP(9)`、既存 tap は `SYSTEM/UNKNOWN(0)` + `LINEAR_RING` | `{3,1,9,1,0x1000,0x400000,1,3,0}` で作り直す |
| 4 | **TSin routing が FED 直接選局で設定されていない** | SetTSPort/Sfd_TsdSetTsIn は adsb/SFD 側（§4.2） | 0x6004 経由（製品 TUM）で選局 |
| 5 | TLV dev の 5 番目の output が ARC にサービスされない | 製品は 4 本のみ。word[1]=0 の意味が未確定（§13） | 4 本の ring のうち 1 本を **読まない**形で tap できないか / port word の実験 |
| 6 | mode=1 (TUNER BOX) で ring 出力が抑止 | `0xA31002A` の mode 意味が未確定（§3.3） | mode を read できる手段が無い。まず mode=0 前提で他を潰す |

- 候補 1–4 は **読み取りで切り分け可能**。候補 5/6 は製品停止なしでは確定困難。

---

## 9. 実機で次に試す最小列（読み取り優先・復元付き）

> すべて **製品プロセスを止めない**。書込みは (E2) の 1 バイト I2C のみ。
> 事前に `t4s-probe` の既存 read-only 経路（`Graph::read`, `read_output_regs`,
> `sbuf_status_raw`）を使う。

### E0 — read-only スナップショット

1. `Graph::read()` で device/buffer/port 表を取得。
   - `device` 行で `words[1] == 0x24` の handle（既知の dev7 と照合）。
   - `port` 表（stride 20）で 0x24 に繋がる buffer handle を特定。
     `buffer` 表の先頭行を「TLV ring」と決め打ちしない。
2. `abs` の `bAdsbInitialized` を read-only:
   - `/dev/kmem` の VA `0x02027184` を 4 バイト read（`graph.rs` と同じ Kmem）。
   - 0 なら UI は park 状態。ただし adsb への直接 0x6004 は可能。
3. 復調 register read（`gr_i2c::read_output_regs`）:
   - bank0 `0x0b,0x0c,0x13,0x16,0x18,0xcc`
   - bank7 `0x0a,0x72,0x73,0x74,0x75,0x76,0x7b,0x7d,0x8d`
   - `0xcc` の bit7/bit6、bank7 が TLV 状態かを記録。
4. 4 本の TLV ring の `sbuf_status_raw(handle)` を read（writer/reader/ready の確認）。
5. `tsi_status` / `mmr_read32(0x6000/8/c/61f0)` は **読むだけ**。書かない。

### E1 — 製品経路で 4K 選局（bounded write なし）

1. `tuner::change_channel`（既存 0x6004 送出）で既知の 4K チャンネル
   （例: BS日テレ4K sid=101, dev=9, tune=0, not_decode=0）を送る。
   - これは製品 UI と同じ経路。TSin routing と MMT フィルタ再構築を製品が行う。
2. 最大 15 s、200 ms 間隔で read-only:
   - `read_output_regs` の lock（既存 `fed` コマンドは `lock & 0x20` と
     bank2 TSID 一致で判定）。
   - `0xcc` が 0x40 か、bank7 が TLV 状態かを再読。
3. 選局後に **tap を作る**（選局前に作らない: 製品の teardown で
   バッファが再構築される可能性がある）。

### E2 — TLV tap（connect_output、復元可能）

1. ring 生成（既存 `SBUF_CREATE` 0x32）:
   ```
   {3, 1, 9, 1, 0x1000, 0x400000, 1, 3, 0}
    type RING, SYSTEM, MMTP, max_reader=1, align=0x1000, size=0x400000 units,
    unit=1 byte, flags=3 (host map 可)
   ```
   - 製品 ring は flags=1 だが host map 不可。tap 用は flags=3。
     製品 REMUX の TSOUT も `{4,1,0,1,...,flags=3}` で device 出力されており、
     **flags=3 buffer に device が書く実績はある**（`remux_tsout_buffer_request`）。
2. `connect_output(tlv_handle, ring)`（0x8002A、`{dev,0,ring,0}`）。
   - 返り値 word[3] を保存（disconnect 用）。
3. `set_ready(ring)`（0x10035、値渡し）。
4. `XcReaderSource::attach(drv, ring, 200)` → `create_reader`（0x37）。
5. 3–5 s `get_read`（0x3B）+ `xc_map` + `commit_read`（0x3C）。
   - `total`、`tlv_score`（0x7F 同期）、先頭 64 B を記録。
6. cleanup: `disconnect(tlv, conn)`（0x9002A）→ `destroy_reader` →
   `destroy_buffer(ring)`（0x34）。
   - disconnect が失敗しても ring は destroy しない（使用中なら残す）。

### E3 — 0xcc が 0xc0 だった場合（bounded I2C write、復元付き）

- E1 の後も `0xcc & 0xc0 == 0xc0` なら:
  1. 既存 `gr_i2c::select_tlv_output_exclusive()` を実行
     （I2C0 セマフォ取得 → 0xcc を `(before & !0xc0) | 0x40` に 1 バイト書込み
     → readback 0x40 確認）。
  2. もう一度 `change_channel` で 4K を選局（製品に TLV 状態を再適用させる）。
     ※ 製品が 0xcc を再び 0xc0 に戻す可能性あり。その場合は E3 の write を
     選局の直後・tap の直前に置く。
  3. E2 の tap を再試行。
- **復元**: 事前に読んだ `0xcc` の値（0xc0）を書き戻す。書込みは 1 バイトのみ。
  復元しない場合も、次回 2K 選局で bit7 が再設定される（揮発）。
- 失敗時の最終復旧は **電源再投入**（フラッシュ書込みは無い）。

### E4 — それでも 0 バイトの場合（製品停止なしの範囲）

- 2K チャンネル → 4K チャンネルへ **製品経路で切り替える**（TSin routing の
  遷移を確実に走らせる）。
- `0xcc`/bank7 を再読し、TLV 状態を確認。
- 4 本の製品 ring の `sbuf_status_raw` で writer が進んでいるか（wpos 増加）を
  read-only で確認。製品 ring が進んでいれば TLV デバイス自体は動いている
  → 5 番目の output がサービスされていない（候補 5）。
- この場合は **port word（buf[1]）の実験**（0/1/4 を 1 回ずつ、disconnect で復元）
  を最後の手段にする。ARC の意味が未確定なので、成功しても製品 ring に
  影響が出ないことを確認する。

---

## 10. 製品停止が必要な場合の対象・復旧・watchdog

現時点で「停止が必要」になり得るのは次の 2 つだけ。**どちらも揮発状態**。

| 操作 | 停止が必要な理由 | 復旧 |
|---|---|---|
| `0x61f0` crossbar の書換（cmd 0x53） | 製品 abs は crossbar を書かないため、書くと TSI 経路と衝突し得る | 事前 read 値を `0x53` で書き戻す。失敗時は再起動 |
| `0x24` デバイスを自前で作り直す / `0xA31` を再実行 | 製品の TLV デバイスを壊す/状態をクリアする | adsb 再起動 → 通常は abs も再起動。最終は電源再投入 |

- **`abs` を SIGSTOP しない**。実測で約 1 分後に製品ウォッチドッグが再起動する
  （プロンプトの実測）。abs は監視/給電の一部を担っている可能性が高い。
- `adsb` だけを停止するのも危険: 0x24/0x25/ring は adsb プロセスの fd に紐づく。
  close でグラフが破棄され、abs が不整合を検知して再起動する可能性がある。
- **停止が必要な実験は行わない**のが本 Phase の方針。E0–E3 は停止不要で完結する。
- 復旧手順（停止した場合）:
  1. `0xcc`/bank7 の事前 read 値を書き戻す（I2C）。
  2. `0x61f0` を identity `0x10` に戻す（必要な場合のみ）。
  3. 製品プロセスを通常起動（`abs`→`adsb`→`tuner` の順は製品 init に任せる）。
  4. それでも不整合なら電源再投入。
- **禁止**: `tsi_enable_cats_board` の再実装、ring の destroy（製品の ring）、
  `destroy_reader`（この機種で slot を失う既知問題）、TSI を 130 B に再設定。

---

## 11. 禁止事項（要約）

- 製品の 4 本の TLV ring に対する `create_reader` / `set_ready` / `destroy`。
- 製品の TLV/MMT デバイスへの `destroy` / `disconnect`（自分が作った接続以外）。
- `TS_RECODER(0x13)` を TLV 経路に使うこと（型違い、130 B 拒否）。
- TSI の 130 B 再設定（`0x50` の me_packet=3）。2K 経路を壊す。
- `0xA31002A` の再実行（sdev の管理バイトをクリアする。意味は未確定）。
- `abs` の SIGSTOP / `adsb` の停止。

---

## 12. 根拠アドレス一覧

### 12.1 adsb（`.text` file = VA−0x8000、RW file = VA−0x10000）

| ラベル | VA | file | 根拠 |
|---|---:|---:|---|
| `wrap_demuxInit` | `0x4e308` | `0x46308` | [R] |
| TLV create 引数 | `0x4e734–0x4e752` | `0x46734–0x46752` | [D] |
| TLV create ioctl | `0x4e752` | `0x46752` | [D] |
| `psfc_fed_AdsbGetRecorderMode` 呼出 | `0x4ead4` | `0x46ad4` | [D] |
| `0xA31002A` ioctl | `0x4eafe` | `0x46afe` | [D] |
| mode=1 → buf[1]=1 | `0x4ece6` | `0x46ce6` | [D] |
| ring loop 先頭 | `0x4eb16` | `0x46b16` | [D] |
| `SBUF_CREATE` 0x32 | `0x4eb7c` | `0x46b7c` | [D] |
| `CONNECT_OUTPUT` 0x8002A | `0x4ebb0` | `0x46bb0` | [D] |
| MMT create 0x2D | `0x4ebf2` | `0x46bf2` | [D] |
| `CONNECT_INPUT` 0x7002A | `0x4ec28` | `0x46c28` | [D] |
| `SBUF_SET_READY` 0x10035 | `0x4ec48` | `0x46c48` | [D] |
| queue_event 0x11002A | `0x4ec76` | `0x46c76` | [D] |
| `FUN_00048a98`（handle 保存） | `0x4ec96` / `0x48a98` | `0x46c96` / `0x40a98` | [D] |
| `set_clock_owner` 0xD002A（TLV） | `0x4ecae` | `0x46cae` | [D] |
| 2K create 0x28（対比） | `0x4e502–0x4e526` | `0x46502–0x46526` | [D] |
| 2K `0x50` TSI config | `0x4e786` | `0x46786` | [D] |
| 2K `0x1F4002A` | `0x4e908–0x4e91c` | `0x46908–0x4691c` | [D] |
| `0x6004` dispatch→handler | `0x1012c` / `0x117c4` | `0x812c` / `0x97c4` | [D] |
| `eAdsbCommTuneChangeChannel` | `0x117c4` | `0x97c4` | [D] |
| tune 本体（FED へ 0x5004） | `0x10860` → `0x1f1ac` | `0x8860` / `0x171ac` | [D] |
| `SetTSPort` 系 | `0x186e4` / log `0xd9b0c` | `0x106e4` / `0xd1b0c` | [D]/[STR] |
| TLV handle グローバル | `0x13ff78`（.bss） | — | [D] |
| `g_DeviceTypeStr`（libvixs） | `0x18cae4` | `0x184ae4` | [SYM]/[TAB] |
| media/subtype 表（libvixs） | `0x18ca68` / `0x18c6a8` | `0x184a68` / `0x1846a8` | [SYM]/[TAB] |

### 12.2 abs（`.text` file = VA−0x8000、RW file = VA−0x10000）

| ラベル | VA | file | 根拠 |
|---|---:|---:|---|
| `BhvTvAdsbTuneChangeChannel` | `0xc66a84` | `0xc5ea84` | [D] |
| `bAdsbInitialized` チェック | `0xc66a9a–0xc66aa2` | `0xc5ea9a–0xc5eaa2` | [D]/[GOT] |
| 同 store 関数 | `0xc669b4` / store `0xc669c6` | `0xc5e9b4` / `0xc5e9c6` | [D]/[GOT] |
| GOT slot | `0xff7f5c`（値 `0x02027184`） | `0xfe7f5c` | [GOT] |
| RW LOAD 範囲 | `[0xfd1000, 0x11403ba0)` | — | [D] |
| ReportInitialized ケース | `0xc36b2a–0xc36b42` | `0xc2eb2a–0xc2eb42` | [D] |
| 文字列 `msgAdsbCommReportInitialized` | `0xf89c00` | `0xe81c00` | [STR] |
| 0x6005 送信 | `0x4578fc` | `0x44f8fc` | [D] |
| `0x6004` 送信（park しない側） | `0x457858` | `0x44f858` | [D] |
| `Sfd_TsdSetTsIn` | `0x1a63a8` | `0x19e3a8` | [D] |
| 同エラー文字列 | `0xd1af20` | `0xd12f20` | [STR] |
| `[TSD]SetTsIn` 文字列 | `0xd14524` | `0xd0c524` | [STR] |

### 12.3 tuner（`.text` file = VA−0x8000、RW file = VA−0x10000）

| ラベル | VA | file | 根拠 |
|---|---:|---:|---|
| `Tune_ISDB_S3_SetTsid` | `0x20b2c` | `0x18b2c` | [D] |
| `Tune_ISDB_S3_IsTLVLocked` | `0x21304` | `0x19304` | [D] |
| `DMD_set_tsout` | `0x26d74` | `0x1ed74` | [R]/[D] |
| 0xcc bit6 maskwrite | `0x26e6c–0x26e72` | `0x1ee6c–0x1ee72` | [D] |
| 0xcc bit7 maskwrite（2K） | `0x26e0a–0x26e10` | `0x1ee0a–0x1ee10` | [D] |
| regset writer（mode 0） | `0x22e2c` / `0x22e50–0x22e58` | `0x1ae2c` / `0x1ae50–0x1ae58` | [D] |
| `FUN_00022be4`（system 分岐） | `0x22be4` | `0x1abe4` | [D] |
| GOT slot → `regset_TLV_SPI_1BIT` | `0x36308` | `0x26308` | [GOT] |
| `regset_TLV_SPI_1BIT` 本体 | `0x3b3cc` | `0x2b3cc` | [D] |
| 0xcc 期待値（実測） | — | — | `gr_i2c::TLV_SPI_1BIT_TSOUT` |

### 12.4 xcdrv.ko（`.text` file = VA+0x34、`.data` file = 0x17b300+offset）

| ラベル | VA/offset | file | 根拠 |
|---|---:|---:|---|
| `dc_cmd_check_dispatch` | `.data 0x108` | `0x17b408` | [SYM]/[D] |
| `dev26_ctrl_table`（class 26, 10 entries） | `.data 0x4f0` | `0x17b7f0` | [SYM] |
| class 26 idx 9 = TLV dev config | — | — | [SYM] |
| `xc_sdev_ctrl_tlv_dev_config_func` | `0x14120` | `0x14154` | [D] |
| `xc_sdev_ctrl_connect_func`（idx 7/8） | `0x14a14` | `0x14a48` | [D] |
| `xc_sdev_ctrl_tsi_func`（class5 idx0） | `0x1349c` | `0x134d0` | [D] |
| `xc_sdev_create_func`（0x28, 24 B） | `0x17c98` | `0x17ccc` | [SYM] |
| `xc_sdev_create_ex_func`（0x2D, 24 B） | `0x15674` | `0x156a8` | [D] |
| `xc_sbuf_create_func`（0x32, 36 B） | `0x15288` | `0x152bc` | [D] |
| `xc_sbuf_ctrl_ready_func`（0x35/1, 4 B） | `0x16300` | `0x16334` | [D] |
| `xc_sdev_ctrl_func`（sub 分解） | `0x15500` | `0x15534` | [D] |

### 12.5 libvixs.so（`.text` file = VA、RW file = VA−0x8000）

| ラベル | VA | 根拠 |
|---|---:|---|
| `xcode_mini_device_control`（cmd 符号化） | `0x7cbf4` | [D] |
| `xcode_mini_device_create_ex` | `0x89cd4` | [D] |
| `xcode_mini_device_connect_output` | `0x8b1c1` | [D] |
| `xcode_mini_device_connect_input` | `0x8be6d` | [D] |
| `g_DeviceTypeStr` | `0x18cae4` | [SYM]/[TAB] |
| `s_media_type_str` | `0x18ca68` | [SYM]/[TAB] |
| `s_media_subtype_str` | `0x18c6a8` | [SYM]/[TAB] |

---

## 13. 未確定

1. **TLV 入力の物理形態**: チップ内 TSI を 130 B に内部設定するのか、
   専用 TLV 入力ピンなのか。ARC ファームウェア未取得。ホスト側に `0x50`/`0x1F4002A`
   の痕跡が無いことは確定。
2. **`0x8002A` の word[1] の意味**: 出力ポートか bind か。製品は常に 0。
   5 番目の output が ARC にサービスされるかは未確定。
3. **`0xA31002A` の mode=0/1 の実挙動**: 録画/チューナーボックスで
   出力先がどう変わるか。ARC 側。
4. **`0x11002A` の event 0xC0000 の意味**。
5. **`0x48a98` が保存する TLV handle の用途**（他参照なし）。
6. **`Sfd_TsdSetTsIn`（qid 0x52 msg 0x1e）の引数意味**と、4K TLV の
   `eTsInNo` がどれになるか。
7. **`0xcc` 0xc0 が TLV に実際に影響するか**（実機の TLV 出力波形/レジスタで要確認）。
8. **`0x73=0x03` vs regset 生値 0x15** の差（mask 適用の詳細）。
9. **mode=1 の個体差**（この機種の `psfc_fed_AdsbGetRecorderMode` の実値）。
10. **`0x24` の 8 出力の使い分け**（4 本しか使わない理由）。
