# Phase 1 — CS 2K 受信経路と BS4K/TLV 受信経路の静的解析

対象: SHARP 4S-C00AS1（ViXS XCode6, ARMv7）。**実機アクセスなし**。ホスト上の取得物のみ。
コード変更なし。本文は doc のみ。

| ファイル | md5 | 備考 |
|---|---|---|
| `.tmp/oem/abs` | `9fc3e99e628ba2893d3192d0319153e8` | 17.6 MB, stripped（dynsym に一部名あり） |
| `.tmp/oem/adsb` | `49e8d2c479a237cad76b2a11459d0153` | 1.1 MB, stripped |
| `.tmp/oem/tuner` | `8d2ef1dc9bb8af7f92efefdd152e9362` | 178 KB, stripped |
| `.tmp/oem/xcdrv.ko` | （not stripped。Phase 0 は別 md5 表記） | 2.0 MB, REL |
| `.tmp/oem/libvixs.so` | `80f28d4a4b1242d9a451d4ea6427a527` | 1.6 MB, **full symbol** |

> **ビルド差の警告（重要）**: 既存 `rust/t4s-core/src/remux4k.rs` の
> `adsb:videoSetFilter @0x5c178` / `audioSetFilter @0x4a9e4` /
> `remux_xcif_open_remux @0x421f8` などは**このビルドには当てはまらない**。
> 本ビルドで `0x5c178` は `fdc_sysGetIpAddressList`、`0x4a9e4` は
> `demuxStopFilterForNtpPacket` の内部であった（`[R]`）。
> 本ドキュメントのアドレスはすべて本ビルドで取得したもの。

## 手法とアドレス規約

- `libvixs.so` / `xcdrv.ko` / `adsb` は **REA 6.4.0 (Ghidra 12.1.4)** の擬似コードを最終根拠にした（`[R]`）。
- `abs` は 17.6 MB のため、REA の `search` / `xrefs` / 新規関数 `function` が
  プロバイダ timeout（330 s）に達した。**Phase 0 で既に擬似コードを取った関数以外は
  REA 擬似コードを取得できなかった**。そのため `abs` は
  `llvm-objdump --triple=thumbv7-linux-gnueabihf` の完全逆アセンブル（`.tmp/abs.dis`）と、
  PIC の `ldr rX,[pc,#imm]; add rX,pc` ＋ GOT オフセット参照を自前で復号する
  スクリプト（`.tmp/abs-refs.pkl`）で関数当たり付けと引数復元を行った（`[D]`）。
  関数境界と caller は `r2 -A`（`aaa`）の `afl`/`axt` を併用した（`[r2]`）。
- アドレスは VA。file offset は次で換算する。
  - `abs`: `file = VA - 0x8000`（`.text` VA `0x49c28` / file `0x41c28`）
  - `adsb`: `file = VA - 0x8000`（`.text` VA `0xe2e8` / file `0x62e8`）
  - `libvixs.so`: `.text` は **VA = file**（`0x55e90` 開始）
  - `xcdrv.ko`: `.text` は `file = VA + 0x34`
- 根拠タグ: `[R]` REA 擬似コード / `[D]` objdump / `[r2]` radare2 / `[M]` 参照資料の実測。

---

## 1. CS 2K — abs の 2K ルート生成

### 1.1 呼び出しチェーン（確定）

```
[どこかの 2K 選局処理]
  └─ Dxd_SetInputPort(ts_port, route_slot, active, 0)        abs:0x2848e4
       └─ 0x28b720(ts_port, route_slot, active, 0)           abs:0x28b720
            ├─ route_slot 3..6 → 0x28e30c / 0x28e3b4（section / PrvPes 系）
            └─ route_slot 0..2 → Dxd_CreateRoute(ts_port, route_slot, …)  abs:0x283f34
                 ├─ route_slot 0 or 2:
                 │    ├─ Dxd_SetTsConfig(route_slot)          abs:0x286490
                 │    │    ├─ tsi_enable_cats_board()          (libxcodeapi.so, 未取得)
                 │    │    └─ setup_tsi_port(phys_port, &cfg)  (libvixs.so:0x8e00d)
                 │    ├─ av_sync_set_sync_mode(av, 1)
                 │    ├─ xc_mini_device_create(fd, 0x0C, 0, 0x20, …)   ← MPEG_TS_DEMUX
                 │    ├─ set_pipeline(demux, av, pipeline)
                 │    ├─ xc_mini_device_config_tsi(fd, demux, 0, 0, -1)
                 │    └─ av_sync live mode 1
                 └─ route_slot 1:
                      ├─ av_sync_set_extra_delay(av, 0)
                      ├─ xc_mini_device_create(fd, 0x06, 1, 0x30, …)   ← 別デバイス
                      └─ pipeline 生成 / set_pipeline
```

- `Dxd_CreateRoute` の caller は **1 か所だけ**（`0x28b770`、`r2 axt`）。
- `Dxd_SetTsConfig` の caller も **1 か所だけ**（`0x283fda`、`Dxd_CreateRoute` 内）。
- `Dxd_SetInputPort` 自体の caller は 1 か所（ラッパ `0x28b920` の `0x28b974`）。
  そのラッパ `0x28b920` の caller は 3 か所（`0x1af708`, `0x1af736`, `0x26ca64`）。
  最初の 2 か所は同じ関数（`0x1aed08`）内で、route_slot をテーブル
  `ldrh [tbl, idx<<1]`（`idx` は 0/1）から取る。3 か所目は `vIssPlayMain`
  （録画/再生系）で、route_slot はグローバル変数値。
  すなわち **route_slot 0/1 は「メイン/サブの 2K TS 入力」**、2 は
  **別系統（CS/4K 側）**、3..6 は section/PrvPes と読める。
  0/1/2 のどれが BS でどれが CS かは、静的にはテーブル解決まで到達できず **未確定**。
  実機の TSI レジスタ read で確定できる（§5）。

### 1.2 `Dxd_SetTsConfig` の実体（abs:0x286490 / file 0x27e490）

`[D]` 0x286490–0x28650e。引数は `u16 route_slot`（r0）。

```
if (route_slot == 0)      phys = 0;
else if (route_slot == 2) phys = 1;
else return;                      // route_slot 1, 3.. は物理 TSI を触らない

cfg (tsd_configure_t, base = sp+8):
  +0x00 enable            = 1
  +0x04 parallel          = 0        // serial
  +0x08 packet            = 0        // me_packet: 0=188
  +0x0c use_sync          = 0
  +0x10 use_neg_edge      = (CPLDGetVer() <= 0x1ff) ? 1 : 0
  +0x14 active_high       = 1
  +0x18 drop_null         = 1

tsi_enable_cats_board();
setup_tsi_port(phys, &cfg);        // 0x2864ee
```

- `CPLDGetVer()` は `0x49bd0`（PLT）。
- **route_slot 0 → 物理 TSI0、route_slot 2 → 物理 TSI1**。188 byte framing、
  serial、sync 不使用、VALID active-high、BPC=0、CPLD 依存の clock edge、null 破棄。
- `tsi_enable_cats_board()` は abs の UND（`readelf`: `0x48800`）。libvixs にも
  無く、`libxcodeapi.so` 未取得のため実体不明。**物理 TSI1 を使う場合に
  この board enable が必須かどうかは未確定**（BS/TSI0 は製品起動時に既に
  enable されているため、bs2k 側が呼ばなくても動く）。

### 1.3 `Dxd_CreateRoute` の実体（abs:0x283f34 / file 0x27bf34）

`[D]` 0x283f34–0x28423c。引数 `(u16 ts_port, u16 route_slot, u32 active, u32 arg4)`。

- 検証: `ts_port <= 0x1d`, `route_slot <= 6`。
- `DxdSproInfo + 0x94*route_slot` を ctx とし、`ctx->[0]`（used フラグ）が
  立っていれば即 return。
- `xc_open_handle()` → `ctx->[8]`。
- route_slot 0/2:
  - `Dxd_SetTsConfig(route_slot)`（0x283fda）
  - `av_sync_set_sync_mode(ctx->[8]=fd, ctx->[0x14]=av, 1)`（0x283fea）
  - `xc_mini_device_create(fd, type=0x0C, 0, 0x20, 0, &ctx->[0x14], name)`
    （0x284022）— **MPEG-TS DEMUX, 32 output ports**
  - `set_pipeline(demux, av, ctx->[0xc])`（0x28405e）
  - `xc_mini_device_config_tsi(fd, demux, 0, 0, -1)`（0x284098）
    （`r2=me_control=0`, `r3=m_port=0`, 第5引数なし → libxcodeapi が `-1` を補う）
  - `av_sync live mode 1`、終了時に `xc_mini_device_run` を 2 回（0x2840e2–0x284112）
- route_slot 1:
  - `av_sync_set_extra_delay(av, 0)`（0x284124）
  - `xc_mini_device_create(fd, type=0x06, 1, 0x30, …)`（0x28415a）
  - pipeline create / set_pipeline

> **要点**: 2K の DEMUX は **常に `m_port=0`（logical TSI 0）** で作られる。
> 物理 TSI は `Dxd_SetTsConfig` が別に設定する。つまり製品コード内で
> 「physical TSI1 → logical 0」の対応は **明示的には書かれていない**。

### 1.4 cfg → カーネル TSI レジスタ（libvixs + xcdrv）

`setup_tsi_port`（libvixs:0x8e00d / file 0x8e00d、`[R]`）:

```
me_packet = {(0x137460)[packet-1]} if 1<=packet<=3 else 0
           // table = {1,2,3} → 0=188, 1=192, 2=204, 3=130
xcode_tsi_config(fd, port,
   me_packet,
   parallel      = cfg[+4]==1,
   enable_src    = cfg[+0]==1,
   use_sync      = cfg[+0xc]==1,
   active_high   = cfg[+0x14]==1,
   use_neg_edge  = cfg[+0x10]==1,
   drop_null     = 1 - clamp(cfg[+0x18]))
```

`xcode_tsi_config`（libvixs:0x7bba9）は 10 word の ioctl `0x50` を組み立てるだけ。
実体は `config_tsi`（xcdrv.ko:0x3d8d4 / file 0x3d908、2076 B、`[R]`）。

`config_tsi` の要点:

| レジスタ | port0 のビット | port1 のビット | 内容 |
|---|---|---|---|
| `0x6000` | 0 / 0x4000 / 0x1000 | 1 / 0x8000 / 0x2000 | me_packet: 2 / 1 / 3 |
| `0x6000` | 0x100 | (port!=0 では設定しない) | parallel=1 のとき |
| `0x6000` | 0x1000000 | 0x2000000 | enable_src |
| `0x6000` | 0x10000 | 0x20000 | use_sync |
| `0x6008` | `1<<(port+0)` | `1<<(port+1)` | disable_auto_reset（製品は 0） |
| `0x6008` | `1<<(port+8)` | `1<<(port+9)` | active_high |
| `0x6008` | `1<<(port+0x10)` | `1<<(port+0x11)` | bpc_sel（製品は 0） |
| `0x6008` | `1<<(port+0x18)` | `1<<(port+0x19)` | use_neg_edge |
| `0x600c` | `1<<(port+8)` | `1<<(port+9)` | drop_null |

- 事前条件: `m_port < SKUCtrl_get_feature_control()`。
- `cmd == 0x53`（`param_2[0x22f] == 0x53`, すなわち TSI_CONFIG_EX）のときだけ
  **crossbar `0x61f0`** を触る:
  `nibble[logical_port] = asi_port`（`port<<2` の 4 bit を置換）。
  その後、物理レジスタの設定は `asi_port` を port として行う。
- 通常 `0x50` は `0x61f0` を触らない。

`0x61f0` の実測 identity は `0x10`（nibble0=0, nibble1=1）。
`map_tsi_one_to_zero()` は nibble1 を 0 にして「logical 1 → physical 0」を作る。

### 1.5 CS の TSI 経路 仮説

観測事実（プロンプト）: BS 2K は `physical TSI0` 設定で成功。CS は
`physical TSI0→logical1` と `physical TSI1→logical1` の両方で 0 byte。
本解析で確定したこと:

1. 製品は 2K DEMUX を **常に logical TSI 0** で作る（`config_tsi(...,0,0,-1)` は abs 内に 1 か所だけ）。
2. 製品は物理 TSI を route_slot 0/2 で **TSI0/TSI1** に設定する（1.2）。
3. 製品 abs は **`xc_mini_device_config_tsi` しか import しておらず、**
   `..._ex` / `xcode_tsi_config_ex` を import も呼び出しもしない
   （`readelf` の UND は `xc_mini_device_config_tsi` のみ。abs 内の
   `config_tsi@plt` 実呼び出しも 1 か所 `0x284098` だけ）。
   したがって abs レベルでは crossbar `0x61f0` の書込みは無い
   （crossbar 呼び出しは `vOutdMain`/`vAudMain` 系の音声 crossbar のみ）。
4. BS が TSI0 で動く以上、製品の BS は route_slot 0、CS は route_slot 2（TSI1）
   である可能性が高い。

したがって CS の DEMUX が logical 0 のまま物理 TSI1 を見るには、
**どこかで logical0→physical1 の対応が必要**。製品 abs にはその明示コードが
無いので、候補は次の 3 つ:

- **(H1) ARC ファームウェアが「直前に setup_tsi_port した物理ポート」を
  logical0 に暗黙バインドする。** この場合、順序（setup_tsi_port → DEMUX 生成 →
  config_tsi）が重要で、bs2k の「physical 設定 → demux 生成 → config_tsi(logical1)」
  は順序は合っているが `logical1` が余計。**CS では `config_tsi` の m_port を 0 に
  戻すのが最小変更**。
- **(H2) `tsi_enable_cats_board()` が crossbar/ボード経路を設定する。**
  この関数は route_slot に依存せず引数なしなので、ポート選択は直前の
  グローバル状態か CPLD 側。TSI1 を使うにはこれが必要な可能性。
- **(H3) 物理 TSI1 はこの SKU では未接続で、CS も TSI0 に出る。**
  この場合 CS 失敗は TSI ではなく demod 出力（`DMD_set_tsout` の system 分岐、
  `0xcc` の content select、TSID 設定）側。

**H1 が最も筋が良い**（abs のコードに crossbar が無い事実と、DEMUX の
`m_port=0` 固定が同時に説明できる）。H3 は §5 の MMR read で即否決できる。

### 1.6 tuner 側は CS でも同一か

`phase0-tuner.md` §4 のとおり、2K 衛星は `FUN_00014518` の
`system type == 2` 分岐で **`Tune_ISDB_S_SetTsid` → `SetFreq` → `SeqStart`** を
使う。CS 固有の分岐は見つかっていない（`[R]`/`[D]` 既存資料）。
- `Tune_ISDB_S_SetTsid`（tuner:0x1ef4c）: ctx+4=1（ISDB-S）、初回 `set_system`、
  `DMD_set_tsout(case 0/1)` で page0 `0x0b` bit0=1、`0xcc` bit7=1、
  `Demod set2 0x2d` で page2 `0x31` bit3=0（絶対 TSID）、`0x2f/0x30` に TSID。
- `SeqStart`: page2 `0x92 = 0x3f`。
- **TSID=0 と実 TSID にコード上の分岐は無い**。0 は「絶対 TSID 0 を選ぶ」
  というだけで、特殊扱い（自動選択）ではない。CS で TSID 0 が効かないのは
  コードではなく、復調側が TSID 0 のストリームを出さないため、と読める。
- CS で `Tune_ISDB_S3_SetTsidEx`（`stream_id`）を使う分岐は 2K には無い。
  4K 側の関数（tuner:0x20b2c 系）と混同しないこと。

### 1.7 DEMUX の PID / TSID

- 製品の PID 追加は `Dxd_SetPid`（abs:0x284ce2）→ `xc_mini_device_add_pid`
  （`Dxd_AllocateAvPes` / `Dxd_SetAvPes` 経由）。`u32::MAX`（全 PID）は
  製品では使わない（PID 個別 + PAT/PMT section filter）。
- 既存 `bs2k.rs` の `add_demux_pid(device, u32::MAX, connection)` は
  **製品と同形ではない**。全 PID フィルタが 2K DEMUX（type 0x0C, 32 port）で
  受理される保証は静的には無い。CS の 0 byte が「フィルタ未通過」なのか
  「入力が無い」なのかを切り分けるには、まず §5 の TSI/ロック read を行う。
- 製品の TS 出力バッファ: MPEG-TS DEMUX の ring は
  `xc_buffer_create(type=3 RING, media=2, max_reader=1, align=0x1000,
  size=0x6000 units × 188 = 0x468000 B)`（`xcdrv.rs` の
  `demux_ts_buffer_create_request` と一致、`[R]`）。CS でも同じ DEMUX/同じ
  出力 port を使う（ポート番号は route_slot に依存しない）。

---

## 2. BS4K / TLV — adsb の TLV パーサとリング

### 2.1 全体像

```
abs  BhvTvAdsbTuneChangeChannel (0xc66a84)
  └─ eSysSendMessage(qid=0x36F, msg=0x6004, 10 words)   abs 0x457858
       ↓
adsb ADSB-COMM → ADSB-TUM → fdc_chm*（外部 libfdc、未取得）
  └─ tuner FED ADBS (SetTsid system=0xD → SetFreq → SeqStart)
       ↓ 復調 TLV 出力（1-bit serial, 物理 TSI0）
adsb wrap_demuxInit (0x4e308)
  └─ ADSB tuner demux 生成（type 0x24）＋ 4× ring ＋ 4× type 0x25
```

### 2.2 `wrap_demuxInit`（adsb:0x4e308 / file 0x46308）

`[R]` 擬似コード（`normalized_result.pseudocode`）。ADSB tuner 分岐
（config table の `type == 2`）は概略次:

```
// 1) TLV demux 本体（me_type 0x24, 8 output ports）
local = { type=0x24, 0, 8, 0, 0 };        // 0x4e73a: cmd 0x2D = SDEV_CREATE_EX
ret  = ioctl(0x2D, &local);               // handle → __s[2]
// 2) TLV device config
local = { handle, recorder_mode };        // psfc_fed_AdsbGetRecorderMode()
ret  = ioctl(0xA31002A, &local);          // class26 index9 = TLV_DEV_CONFIG

// 3) 4 本の 4 MiB ring（0x4eb16..0x4ec84 の do-while、4 回）
buf  = ioctl(0x32, {3,1,9,1,0x1000,0x400000,1,1,0});
       // type=3 RING, index1=1, index2=9 (media/kind のいずれか),
       // max_reader=1, align=0x1000, size=0x400000, unit=1, flags=1, out=0
conn = ioctl(0x8002A, {tlv_dev, 0, buf, 0});   // connect_output
mmt  = ioctl(0x2D,  {0x25,1,0x100,0,0,0});     // MMT demux: 1 input, 256 outputs
ioctl(0x7002A, {mmt, 0, buf, 0});              // connect_input(mmt ← ring)
ioctl(0x10035, buf);                           // SBUF_CTRL SET_READY
ioctl(0xD002A, {mmt, 0xC0000});                // QUEUE_EVENT
```

- 生成順: **TLV demux → ring → connect_output → MMT demux → connect_input →
  ring ready → event**。ring は 4 本、各 4 MiB、media kind 9。
- `connect_output` 要求は `{device, 0, buffer, 0}`（reply の word3 が connection handle）。
  `xcdrv.rs` の `connect_output` と同一形（`[R]` libvixs 0x8b1c1 は wrapper）。
- `xc_sbuf_create` のフィールド順は本ビルドの実測形
  `[type, ?, media/kind, max_reader, align, size, unit, flags, out]`。
  **max_reader は index3**。TLV ring は `1`。
  これは video ES ring（§2.3）で `4` が入り、参照資料の「video max_reader=4」と
  一致することから確定した。media/kind が index1 か index2 かは未確定
  （TLV ring は index1=1 / index2=9、video ring は index1=3 / index2=0）。

### 2.3 ES ring（参考: 4K の映像/音声）

- 映像 ES ring: adsb:0x5a5a2–0x5a5d6
  `{3,3,0,4,0x1000,0x1400000,1,1,0}` — RING, media=3, **max_reader=4**,
  20 MiB、unit=1。参照 `remux4k.rs` の `VIDEO_ES_SIZE=0x1400000`,
  `max_reader=4` と一致。
- 音声 ES ring: 本ビルドでは 2 MiB の ring 生成が複数箇所
  （0x500f4, 0x5b984）にあるが、参照資料の `media=6 / max_reader=5` と
  一致する生成列をまだ特定できていない（**未確定**）。0x5b984 は
  `{3,9,0,1,0x1000,0x200000,?,3,0}` で media 9（TLV 系）と読めるため、
  AVM の音声リングではない可能性が高い。
- 4K の ES ring は `MMT_DEMUX(type 0x25)` の出力側に作られ、HDMI 接続時のみ
  AVM/decoder が reader を付ける。HDMI 無しで「ES リング無し」なのは
  この reader/decoder 不在と整合する（リング自体は 0x25 生成時に作られうる）。

### 2.4 TLV リングの正しい消費者

- 製品の消費者は **`me_type 0x25`（MMT_DEMUX）**。`connect_input(mmt, ring)` が
  ring の `max_reader` スロットを 1 つ消費する。
- TLV ring の `max_reader = 1` なので、**製品が接続した時点で空きスロットは 0**。
  したがって `xc_buffer_create_reader`（create_reader）は**不可**（成功しても
  製品の reader と競合し、この機種では `destroy_reader` が -1022 で slot を
  失う既知問題もある）。
- **外部タップの唯一の合法形は、`type 0x24` デバイスへの追加 `connect_output`**
  （自前の ring を 2 本目の output として付ける）。`type 0x24` は 8 output ports
  なので空きがある。ユーザの「dev7 の出力 duplicate が 0 byte」は、
  トポロジ不正ではなく **TLV demux に入力（チューナ出力/ロック）が無い**ためと
  読むのが自然（§2.6）。
- `TS_RECODER(type 0x13)` はこの製品の TLV 経路では**使われていない**。
  0x13 は SDK の `ts_input_t` 系（libvixs）が使う形で、製品 adsb の
  0x24/0x25 チェーンとは別物。

### 2.5 TLV パーサを構成する関数と `bAdsbInitialized`

- TLV パーサ（`0x24`）＋ 4 ring ＋ `0x25` の構成は **`wrap_demuxInit`
  （adsb:0x4e308）内の ADSB tuner 分岐**。チューナ選局ごとではなく
  **adsb 初期化時に一度**作られる。
- `SetTsid/SetFreq/SeqStart` は **tuner** 側（`Tune_ISDB_S3_*`）。adsb は
  `fdc_chm*` 経由で tuner FED にメッセージを送るだけで、TLV パーサ自身は
  RF/復調を触らない。
- `remux_xcif_*`（adsb の文字列: `remux_xcif_open_remux`, `..._connect_avesbuff`,
  `..._add_stream` など。VA は `0x160b5c` 付近）は **録画 REMUX** の API で、
  4K の TLV 取得には関与しない（既定の再生/録画経路）。
- `bAdsbInitialized`（abs `.bss` `0x02027184`, 4 B）を 1 にする主体:
  文字列 `[BhvTvProcessProc]msgAdsbCommReportInitialized`（abs VA `0xf89c00`、
  参照サイト `0xc36b2e`）を持つメッセージハンドラ。adsb 側は
  `[ADSB-COMM] RCV msgAdsbCommReportInitialized` / `eAdsbCommProcInitialized`
  を経て初期化完了を返す。**「adsb が初期化完了を報告した時点で abs が
  フラグを立てる」**と推定する（フラグへの実ストア命令は GOT 間接のため
  静的には未特定。§6）。

### 2.6 HDMI 無しで TLV だけを動かす最小列

1. **adsb を初期化完了まで進める**（`bAdsbInitialized=1` になるのを待つ）。
   これが 0 の間、abs は `0x6004` を park するので高レベル選局は動かない。
   - 代替: `0x6004` を直接 `msgsnd`（qid 0x36F）する、または adsb の
     ADSB-COMM に `msgAdsbTuneRequestCommInitialize` 相当を送る。
2. **4K 選局**（`SetTsid(system=0xD, stream_id)` → `SetFreq` → `SeqStart`）。
   既存 `fed_hw::adsb` の step 列を使う。`Tune_ISDB_S3_IsTLVLocked`
   （tuner:0x21304）で lock を確認。lock まで 2000 ms。
3. **物理 TSI0 を 4K 用に構成**（製品は `regset_TLV_SPI_1BIT` 相当:
   demod page0 `0x13=04`, `0x16=21`, `0xcc=80` 等）。`Dxd_SetTsConfig` の
   2K cfg とは別物（TLV は 1-bit serial）。
4. **`0x24` TLV demux の生存確認**（graph の device type `0x24`、output ring 4 本、
   各 `0x400000`）。既存の graph snapshot で dev ハンドルと ring ハンドルを取る。
5. **追加 `connect_output(tlv_dev, own_buffer)`** で自前 ring を作り、
   `sbuf_set_ready` して読む。`connect_input` は使わない（max_reader=1 を
   製品が消費済み）。
6. ES デコードは行わない（HDMI 無しでも TLV は取れる。`0x25` の出力 ES ring は
   触らない）。

> 0 byte だった原因の最有力: 手順 1–3 が成立していない（= TLV demux に
> 入力が無い）。dev7 の type 0x24 と 4 ring の存在は「初期化済み」だけを
> 示し、「選局済み・lock 済み」は示さない。

---

## 3. 製品を止める必要がある箇所と復旧手順（案）

現時点で「停止が必要」なのは次の 2 つだけ。どちらも**揮発性（再起動で消える）**。

| 操作 | なぜ停止が必要か | 復旧 |
|---|---|---|
| `0x61f0` crossbar nibble の書換（`cmd 0x53`） | 製品 abs は crossbar を書かないため、書くと製品の TSI 経路と衝突しうる。書込前に nibble を read して保存 | 保存値を `cmd 0x53` で書き戻す。失敗時は再起動（`0x61f0` は電源復帰で identity `0x10`） |
| `bAdsbInitialized` を強制的に 1 にする / `0x6004` を直接送る | abs の状態機械を迂回する。競合すると adsb/abs のメッセージ列が不整合 | プロセス再起動（`abs`/`adsb`）。設定ファイル変更なし |

- **製品停止の前に必ず保存**: `0x6000`, `0x6008`, `0x600c`, `0x61f0` の 4 word、
  `DxdSproInfo` の used/handle、dev type `0x24`/`0x25` のハンドルと ring ハンドル、
  `abs`/`adsb`/`tuner` の PID。
- **禁止**: `tsi_enable_cats_board` の再実装、`0x0A31` の TLV dev config の
  作り替え、ring の destroy、`destroy_reader`。
- 失敗時の最終復旧は **電源再投入**（本解析の操作はすべて MMR/ioctl の
  揮発状態で、フラッシュ書込みは無い）。

---

## 4. 未確定項目

1. **route_slot 0/1/2 と BS/CS の対応**。`Dxd_SetInputPort` の caller が
   route_slot をテーブル/グローバルから取るため、即値では決まらない。
2. **`tsi_enable_cats_board()` の実体**（libxcodeapi.so 未取得）。TSI1 の
   board-level enable か、crossbar 設定か。
3. **`config_tsi` の `m_port` の意味**。ARC ファームウェア側
   （`process_ioctl_request`）が未解析。H1（直前に設定した物理ポートへ暗黙
   バインド）は仮説。
4. **`bAdsbInitialized` の実ストア命令**。GOT 間接のため未特定。
5. **音声 ES ring の生成列**（media 6 / max_reader 5）。
6. **`0x24` TLV demux の物理入力接続**（どの `connect_input`/TSI ルートで
   物理 TSI0 に結ばれるか）。`wrap_demuxInit` には `0x24` への connect_input が
   見えないため、別関数（TSI ルート側）が接続している可能性。
7. **`0xD002A` QUEUE_EVENT の event `0xC0000` の意味**。
8. **`0x61f0` の nibble の向き**（logical→physical か physical→logical か）。
   kernel の log 文言は "XC_TSD_CROSSBAR ... port / asi port" で、
   `nibble[port]=asi` と読めるが、ARC 側の解釈は未確認。

---

## 5. 次に必要な実機試験（すべて read-only から）

### E0（最優先・read-only）— 製品が CS で使う物理 TSI を確定

製品 UI で **BS を選局** → 4 word read。次に **CS を選局** → 4 word read。

```
MMR 0x6000, 0x6008, 0x600c, 0x61f0   (kernel config_tsi と同じ読み口;
                                      snapshot_tsi_mapping の mmr_read32)
```

判定:
- CS で `0x6000` bit25 (enable) / bit17 (sync) / bit13,15 (packet) が立っていれば
  → **physical TSI1**（route_slot 2）を使っている（H3 否決）。
- CS でも bit24/16/12-15 が port0 のままなら → **CS も TSI0**。H3 が本命になり、
  失敗は demod 出力/TSID/filter 側。
- `0x61f0` が CS で `0x10` 以外になれば → 製品が crossbar を書いている（H2）。

### E1（read-only）— CS 中の復調出力

`read_isdbs_status` に加えて、tuner の `ctx+0x38`（TLV lock）ではなく
2K の TS lock（page2 `0x5e` bit5）と `0xcc`/`0x0b`/`0x13`/`0x16` を
SeqStart 後 0/10/50/200 ms で読む。`0xcc` が `0xc0`（TS enable + bit6）で
`0x16=0` なら ISDB-S 2K の出力形。

### E2（bounded write; 復元可能）— logical0 に physical1 を割り当てる

E0 で「CS は TSI1 だが `0x61f0` は identity のまま」だった場合のみ。

```
snapshot = mmr_read32(0x61f0)                  // 0x10 を期待
ioctl 0x53: {port=0, me_packet=0, parallel=0, enable=1, use_sync=0,
             active_high=1, use_neg_edge=?, drop_null=1, asi_port=1}
   // 0x6000/0x6008/0x600c は asi_port=1 として書かれる（kernel config_tsi）
DEMUX は既存のまま (config_tsi m_port=0) で PID/TSID を設定し読み取り
最後に ioctl 0x53: {port=0, ... same physical fields ..., asi_port=0} で
nibble0 を 0 に戻す（= 0x10 復元）
```

注意: この操作は「物理 TSI1 の再設定」も同時に行う。E0 で読んだ
`0x6000/8/c` の物理1フィールドをそのまま使い、**製品が設定した値を上書きしない**。

### E3（4K TLV の最小取得）

1. `bAdsbInitialized` が 1 になるまで待つ（abs の 0x6004 が park されない）。
2. 4K 選局（`SetTsid(0xD, stream_id)` → `SetFreq` → `SeqStart`）。
3. `Tune_ISDB_S3_IsTLVLocked` が真。
4. `0x24` device に **追加 `connect_output`**（4 MiB ring, max_reader=1）→
   `set_ready` → `read`。0 byte なら graph の `0x24` device の run 状態と
   入力接続を確認。
5. ES デコードは触らない（HDMI 無しでも TLV は取れる）。

### E4（read-only）— TLV demux の入力接続

graph の port テーブルで `type 0x24` device の **input 側**（device←buffer また
buffer←device）を列挙し、どの TSI/物理ルートに結ばれているかを確認。
`0x24` の入力が「なし」なら、`config_tsi`/TSI ルートの再構築が必要。

---

## 6. 根拠アドレス一覧

### 6.1 abs（file = VA − 0x8000）

| ラベル | VA | file | 根拠 |
|---|---:|---:|---|
| `BhvTvAdsbTuneChangeChannel` | `0xc66a84` | `0xc5ea84` | `[R]` 既存 / `[D]` |
| `vEtmMain+0x308`（0x6004 送信） | `0x457858` | `0x44f858` | `[D]` |
| `msgAdsbCommReportInitialized` ハンドラ | `0xc36b2e`（参照 site） | `0xc2eb2e` | `[D]` 文字列 xref |
| `bAdsbInitialized` | `0x02027184` | —（.bss） | `[DYN]` |
| `Dxd_SetInputPort` | `0x2848e4` | `0x27c8e4` | `[D]` / `[r2]` |
| route helper（`Dxd_CreateRoute` 呼出し） | `0x28b720` | `0x283720` | `[D]` / `[r2]` |
| `Dxd_CreateRoute` | `0x283f34` | `0x27bf34` | `[D]` / `[r2]` |
| `Dxd_SetTsConfig` | `0x286490` | `0x27e490` | `[D]` / `[r2]` |
| `setup_tsi_port` 呼出し | `0x2864ee` | `0x27e4ee` | `[D]` |
| `tsi_enable_cats_board` 呼出し | `0x2864e2` | `0x27e4e2` | `[D]` |
| `Dxd_SetTsConfig` 呼出し | `0x283fda` | `0x27bfda` | `[D]` |
| DEMUX create (type 0x0C, 0x20) | `0x284022` | `0x27c022` | `[D]` |
| `config_tsi(...,0,0,-1)` | `0x284098` | `0x27c098` | `[D]` |
| `set_pipeline(demux)` | `0x28405e` | `0x27c05e` | `[D]` |
| route_slot==1 分岐（type 0x06） | `0x28415a` | `0x27c15a` | `[D]` |
| `Dxd_SetPid` | `0x284ce2` | `0x27cce2` | `[D]` / `[r2]` |
| `Dxd_SetAvPes` 周辺 | `0x286514` | `0x27e514` | `[D]` 文字列 xref |
| `Dxd_AllocateSecFilter` | `0x287d90` | `0x27fd90` | `[D]` 文字列 xref |
| `TSin xc5 0..3` 文字列 | `0xd1bb40` | `0xd13b40` | `[D]` |

### 6.2 adsb（file = VA − 0x8000）

| ラベル | VA | file | 根拠 |
|---|---:|---:|---|
| `wrap_demuxInit`（`FUN_0004e308`） | `0x4e308` | `0x46308` | `[R]` |
| TLV device create（0x2D, type 0x24） | `0x4e73a` | `0x4673a` | `[D]` / `[R]` |
| TLV dev config（`0xA31002A`） | `0x4eafa` | `0x46afa` | `[D]` / `[R]` |
| 4 MiB ring / connect loop | `0x4eb16`–`0x4ec84` | `0x46b16`–`0x46c84` | `[D]` / `[R]` |
| `xc_sbuf_create`（0x32） | `0x4eb3c` / `0x4eb7c` | `0x46b3c` / `0x46b7c` | `[D]` |
| MMT device create（0x2D, type 0x25） | `0x4ebb8` | `0x46bb8` | `[D]` |
| video ES ring（20 MiB, max_reader=4） | `0x5a5a2` | `0x525a2` | `[D]` |
| 2 MiB ring 候補（media 9） | `0x5b984` | `0x53984` | `[D]` |
| 2 MiB ring 候補（remux 系） | `0x500f4` | `0x480f4` | `[D]` |
| `[ADSB-COMM] RCV msgAdsbCommReportInitialized` | `0xff...` 文字列 | — | `[STR]` |
| `remux_xcif_open_remux` 文字列 | `0x160b5c` | `0x158b5c` | `[STR]` |

### 6.3 libvixs.so（file = VA）

| ラベル | VA=file | 根拠 |
|---|---:|---|
| `xcode_tsi_config` | `0x7bba9` | `[R]` |
| `xcode_tsi_config_ex` | `0x7bce9` | `[R]` |
| `xcode_mini_device_config_tsi` | `0x83339` | `[R]` |
| `xcode_mini_device_tsi_info` | `0x83281` | `[SYM]` |
| `xcode_tsi_status_check` | `0x7be49` | `[SYM]` |
| `xcode_io_t::setup_tsi_port` | `0x8e00d` | `[R]` |
| `xcode_io_t::setup_tsi_port_ex` | `0x8e0a5` | `[SYM]` |
| `xcode_mini_device_connect_output` | `0x8b1c1` | `[SYM]` |
| `xcode_mini_device_connect_input` | `0x8be6d` | `[SYM]` |
| `xcode_buffer_create_reader` | `0x8b97d` | `[SYM]` |

### 6.4 xcdrv.ko（file = VA + 0x34）

| ラベル | VA | file | 根拠 |
|---|---:|---:|---|
| `config_tsi`（2076 B） | `0x3d8d4` | `0x3d908` | `[R]` |
| `get_tsi_status` | `0x3e0f0` | `0x3e124` | `[SYM]` |
| `xc_sdev_ctrl_tsi_func` | `0x1349c` | `0x134d0` | `[SYM]` |
| `xc_tsi_config_func`（cmd 0x50） | `0x14d08` | `0x14d3c` | `[SYM]` |
| `xc_tsi_config_ex_func`（cmd 0x53） | `0x14c18` | `0x14c4c` | `[SYM]` |
| `xc_sbuf_create_func`（cmd 0x32） | `0x15288` | `0x152bc` | `[R]` |
| `xc_sdev_create_ex_func`（cmd 0x2d） | `0x15674` | `0x156a8` | `[SYM]`＋`[R]`（size 0x18） |
| `xc_sdev_ctrl_connect_func`（0x2a/7,8） | `0x14a14` | `0x14a48` | `[SYM]`＋`[R]`（size 0x10） |
| `xc_sdev_ctrl_tlv_dev_config_func` | `0x14120` | `0x14154` | `[SYM]` |
| `xc_sbuf_ctrl_ready_func` | `0x16300` | `0x16334` | `[SYM]` |
| TSI MMR | `0x6000` / `0x6008` / `0x600c` / `0x61f0` | — | `[R]` |

---

## 7. まとめ（要点）

- **CS 2K**: 製品の 2K DEMUX は常に `logical TSI 0`。物理 TSI は
  `Dxd_SetTsConfig` が route_slot 0→TSI0 / 2→TSI1 に設定（188 serial,
  sync off, VALID high, BPC0, null 破棄）。abs は crossbar `0x61f0` を一切
  書かない。CS が TSI1 なら、製品は「直前に設定した物理ポートへ暗黙バインド」
  している可能性が高い。**まず MMR 4 word の read-only 比較**で確定する。
- **CS の 0 byte** は TSI 以前に「復調が TS を出していない」可能性もある。
  TSID=0/実 TSID にコード分岐は無い。PID は製品が個別設定（全 PID は非製品形）。
- **4K TLV**: 製品は `type 0x24`（8 out）→ 4 MiB ring 4 本（max_reader=1）
  → `type 0x25`（MMT, 1 in / 256 out）の順で構成。**ring の reader は製品が
  1 枠使い切るので create_reader 不可。外部タップは `0x24` への追加
  `connect_output` のみ**。`TS_RECODER(0x13)` は使われない。
- **0 byte の dev7 duplicate** は「TLV demux に入力/ロックが無い」ことが
  最有力。HDMI 無しでも TLV は取れるが、`bAdsbInitialized=1` と 4K 選局
  （system 0xD）+ TSI0 の 1-bit serial 構成が前提。
