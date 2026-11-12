# Phase 0 — 衛星 FED 制御の静的解析（別ビルド tuner / libvixs）

対象: SHARP 4S-C00AS1（ViXS XCode6, ARMv7）。実機アクセスなし。ホスト上の取得物のみ。

| ファイル | 種別 | md5 | 備考 |
|---|---|---|---|
| `.tmp/oem/tuner` | ELF32 ARM, stripped, 178920 B | `8d2ef1dc9bb8af7f92efefdd152e9362` | 参照リポジトリの `0416f546…` とは別ビルド |
| `.tmp/oem/libvixs.so` | ELF32 ARM shared, stripped, 1615324 B | `80f28d4a4b1242d9a451d4ea6427a527` | tuner からはリンクされない（下記） |

ツール: **REA 6.4.0**（Ghidra 12.1.4 / Java 25、スナップショット `.tmp/rea-snap-tuner` 使用）を最終根拠とし、
`r2 -A` / `llvm-objdump` / `readelf` / 生バイト列を補助照合に使った。固定アドレスは参照資料から流用していない。

> `libvixs.so` の `DT_NEEDED` は `libc.so.6`, `libgcc_s.so.1` のみ。`tuner` は `libsysapi/libcpld/libi2c/libgpiod/libstm`
> を必要とし、**libvixs をリンクしない**。したがって両バイナリは同じ I2C スタック規約（`/dev/i2c-N` + `I2C_RDWR 0x0707`）
> を共有するが、SC1501A のレジスタ列を libvixs 側で独立検証することはできない。この点は「未確定」に明記した。

---

## 1. 確定した I2C トランスポート（アドレス規約）

| 対象 | 7-bit slave | tuner が渡す値 | 変換 | 根拠 |
|---|---|---|---|---|
| 衛星 RF IC | `0x60`（unit0） / `0x61`（unit1） / `0x62`（unit2） | **8-bit** `0xc0` / `0xc2` / `0xc4` をそのまま `I2c_Write(0, …)` へ | 変換なし | `FUN_0001c964`（`0x1c964`）: unit 0/1/2 → `0xc0/0xc2/0xc4`、`FUN_0001ca3c`（`0x1ca3c`）が `I2c_Write(0,param_2,…)` |
| SC1501A 復調器 | `0x10 + page`（page 0→`0x10`, page 7→`0x17`） | **7-bit** `0x10 + page` を渡す | ラッパ内で `<<1` して 8-bit 化 | `FUN_0001e020`（`0x1e020`）: `local_9 = param_1 << 1` → `I2c_Write(0, local_9, …)` / `FUN_0001dfc4`（`0x1dfc4`）: `I2c_Read(0, param_1 << 1, …)` |
| unit オフセット | unit1 は `+0x20`, unit2 は `+0x40`（7-bit 換算で `0x30` / `0x50`） | `(unit & 7) * 0x20 + 0x10 + page` | 同上 | `FUN_00025f60`（`0x25f60`）、`FUN_00022d4c`（`0x22d4c`）、`FUN_00023224`（`0x23224`） |

- RF 側は 8-bit を直接渡し、復調側は 7-bit を `<<1` して渡す。**両呼び出し元が同じ `I2c_Write` に対して
  一貫した 8-bit 規約**になるため、`I2c_Write` は 8-bit アドレス期待と判断できる（libi2c.so 本体は未取得）。
- libvixs 側の独立照合: `xcode_io_t::i2c_write` は `addr = param_2 >> 1` として `ioctl(fd, 0x707, …)` を1メッセージで発行し、
  入力が 8-bit であることを確認できる（`libvixs:0x8d0c5`、文字列 `[I2C] WRITE @ %d-%2.2X`、`/dev/i2c-%d`）。
- 復調 maskwrite の意味は `new = (old | (value & mask)) & (value | ~mask)`（`FUN_0001e084` / `0x1e084`）。通常の
  `(old & ~mask) | (value & mask)` と等価。

### プロセス間セマフォ

| 用途 | セマフォ ID | 根拠 |
|---|---|---|
| RF IC（`0xc0` 系） | `0x3c6` | `x_pon/x_tune/x_fin_isdbs` の `eSysTakeSemaphore(0x3c6,-1)`（例 `0x1c3dc`, `0x1c50c`, `0x1c804`） |
| 復調器（`0x10` 系） | `0x3c5` | `Tune_ISDB_S_SetTsid` ほか（`0x1ef5a`, `0x1f1ae`, `0x1f264`） |

Phase 1 では既存 `gr_i2c.rs` と同じ **attach-only**（作成・初期化しない）を守る必要がある。

---

## 2. 衛星 RF IC（i2c-0 slave 0x60）の書き込み列

すべて `FUN_0001c934`（1 バイト）/ `FUN_0001c964`（複数バイト）→ `write_tuner` → `I2c_Write(0, addr8, reg, 1, buf, len)`。addr8 は unit 0/1/2 = `0xc0/0xc2/0xc4`。
unit は `FUN_0001c964` の第1引数（0/1/2 → `0xc0/0xc2/0xc4`）。

### 2.1 power-on（起動時のみ、`sc1501a_Init` から unit ごとに1回）

`x_pon_isdbs`（`FUN_0001c3d4`, VA `0x1c3d4` / file `0x143d4`）。明示 delay なし。

| # | reg | data | 命令 | 根拠 |
|---|---|---|---|---|
| 1 | `0x41` | `07 00 05` | `0x1c448` | `strb`列 `0x1c424`–`0x1c432` |
| 2 | `0x45` | `01 00` | `0x1c46c` | `0x1c44c`–`0x1c45a` |
| 3 | `0x0c` | `14 00` | `0x1c490` | `0x1c470`–`0x1c47e` |
| 4 | `0x7b` | `02 01` | `0x1c4b4` | `0x1c494`–`0x1c4a2` |

呼び出し元は `sc1501a_Init`（`FUN_0001ac80` / `0x1ac80`）の `bs tuner:%d` ループのみ（`callers` 1件）。
すなわち **2K 選局ごとには power-on されない**。デーモン差し替え時は起動時にこの4列を再現するか、
製品起動後は既に power-on 済みであることを前提にする必要がある。

### 2.2 tune（選局ごと）

`x_tune_isdbs`（`FUN_0001c500`, `0x1c500`）。引数 `(u32 if_khz, int is_s3 /*1=2K,2=S3*/, int unit)`。
`eFed_ISDB_S_TunerSetChannel`（`FUN_0001ad80`, `0x1ad80`）が 2K で `is_s3=1`、`eFed_ISDB_S3_…`（`FUN_0001ae50`, `0x1ae50`）が `2`。

| # | reg | data | 条件 | 命令 | 根拠 |
|---|---|---|---|---|---|
| 1 | `0x15` | `12` | 常 | `0x1c562` | — |
| 2 | `0x43` | `04` / `05` | `if_khz <= 2_150_000` / `>` | `0x1c588` | 分岐 `0x1c570`–`0x1c582`（`movw/movt 0x20ce70`） |
| 3 | `0x45` | `01` / `00` | 同上 | `0x1c5ae` | `0x1c596`–`0x1c5a6` |
| 4 | `0x6a` | `00 00` | 常 | `0x1c5d2` | — |
| 5 | `0x75` | `f9` | 常 | `0x1c5e4` | — |
| 6 | `0x40` | `07` | 常 | `0x1c5f6` | — |
| 7 | `0x41` | `07` | 常 | `0x1c608` | — |
| 8 | `0x48` | `07` | 常 | `0x1c61a` | — |
| 9 | `0x01` | `01` / `03` | `if_khz <= 2_150_000` / `>` | `0x1c640` | `0x1c61e`–`0x1c63a` |
| 10 | `0x04` | `c4 40` | 常 | `0x1c664` | — |
| 11 | `0x06` | 2K: `03 80 70 1e 02 24` / S3: `03 00 b4 78 08 30` | `is_s3==1`（2K）/ `is_s3==2`（S3） | `0x1c6ca` | 構築 `0x1c668`–`0x1c6b8` |
| 12 | `0x0c` | 低域: `fe 02 9e 16` / 高域: `fc 22 9e 16` | `if_khz <= 2_150_000` / `>` | `0x1c70e` | `0x1c6ce`–`0x1c6f8` |
| 13 | `0x10` | `[(if_khz>>2)&0xff, (if_khz>>10)&0xff, (if_khz>>18)&0x0f, ff, 00, 01]` | 常 | `0x1c75a` | `0x1c712`–`0x1c744`（`lsrs #2` 3回） |
| — | **delay 10 ms** | `usleep(10*1000)` | 常 | `0x1c760` → `0x1cba0` | `movs r0,#0xa`; `FUN_0001cba0` は `usleep(arg*1000)` |
| 14 | `0x05` | `00` | 常 | `0x1c772` | — |
| 15 | `0x04` | `c0` | 常 | `0x1c784` | — |

注意: `0x04` は手順 10 で `c4 40` を書いた直後に、周波数書込みと 10 ms 待ちを挟んで `c0` に戻す。

### 2.3 standby（`x_fin_isdbs`）

`FUN_0001c7fc`（`0x1c7fc`）。`eFed_ISDB_S_Tuner_standby_mode`（`FUN_0001af90`）から unit 指定で呼ばれる。

| # | reg | data | 命令 |
|---|---|---|---|
| 1 | `0x15` | `12` | `0x1c85a` |
| 2 | `0x43` | `05` | `0x1c86c` |
| 3 | `0x45` | `01` | `0x1c87e` |
| 4 | `0x0c` | `14 00 00` | `0x1c8a6` |
| 5 | `0x01` | `00` | `0x1c8b8` |
| 6 | `0x05` | `00` | `0x1c8ca` |
| 7 | `0x04` | `c0` | `0x1c8dc` |

これは既存 `rust/t4s-core/src/gr.rs:91-99` の「RF satellite standby」7 列と**バイト単位で一致**した
（参照ビルドでの固定アドレスは違うが、別ビルドでも同一列）。良いクロスチェック点である。

### 2.4 衛星選局の直前に実行される「地デジ RF standby」

2K `SetTsid` の冒頭で `FUN_0001af24(1, 0x10/0x20/0x40)` → `x_fin_isdbt(unit)` が呼ばれる。
`x_fin_isdbt`（`FUN_0001d99c`, `0x1d99c`）は次の5列（`0x74/0x67/0x5e/0x88/0x87`）。

| # | reg | data | 命令 |
|---|---|---|---|
| 1 | `0x74` | `12` | `0x1d9fa` |
| 2 | `0x67` | `00` | `0x1da0c` |
| 3 | `0x5e` | `15 00 00` | `0x1da34` |
| 4 | `0x88` | `00` | `0x1da46` |
| 5 | `0x87` | `c0` | `0x1da58` |

既存 `gr_i2c.rs::standby_isdbt()` の5列と一致。**衛星を選ぶ前に地デジ側を standby に落とす**のが製品順序である。

（参考）`x_pon_isdbt`（`FUN_0001d394`, `0x1d394`）は起動時に `0x99/0x81/0x91/0x9b` → **10 ms** →
`0x74/0x67/0x5e/0x88/0x87/0x80/0x85` を書く。既存 `gr.rs:102-116` の11列と一致（`0x85=01` は `FUN_0001daa8`）。
10 ms は `FUN_0001df6c(10)` = `usleep(10000)`（`0x1df6c`）。

---

## 3. 復調器（i2c-0 slave 0x10 + page）の設定列

### 3.1 `set_system(ISDBS)` = `regset_ISDBS` の一括適用

`Tune_ISDB_S_SetTsid` は `ctx[1] == 1`（ISDB-S）を立て、初回（`DAT_0003672c == 1`）だけ
`FUN_00022434`（`0x22434`）を呼ぶ。この関数は system 1 で `FUN_00025f60(ctx, regset_ISDBS)` を実行する
（`0x22434` 内の switch、`regset_ISDBS` 参照は `0x36c70`）。

- `FUN_00025f60`（`0x25f60`）は 3 バイト triple `(page, reg, value)` を `ff ff ff` まで適用する。
  slave 計算は `((unit & 7) * 0x20 + page + 0x10)`、すなわち **page 0 → 7-bit `0x10`、page 7 → `0x17`**。
- 初回のみ `regset_Recommend_DIFF`（`0x38c30`, 62 entries）を先に適用し、`DAT_0003b740[unit]` を立てる。
- 2回目以降の 2K 選局では **この一括列は再送されない**（`DAT_0003672c` が 0 のため）。
  Phase 1 で「起動後はじめての 2K 選局」と「2回目以降」を区別して観測する必要がある。

### 3.2 `DMD_set_tsout(system=1)`

`FUN_00026d74`（`0x26d74`）。system `ctx[1] < 2` の分岐（= ISDB-S）で、次を **maskwrite** する。

| # | page | reg | mask | value | 意味 | 命令/根拠 |
|---|---|---|---|---|---|---|
| 1 | 0 | `0x0b` | `0x01` | `0x01`（`ctx[0xc28]&4==0` のとき） / `0x00` | TS pin enable | `0x26d74` 内、`ctx+0x30a`(=+0xc28) 判定 |
| 2 | 0 | `0xcc` | `0x80` | `0x80` | TS content select bit7 | 同上 |

`ctx[0xc28]` は `SetTsid` が `*(ctx + (unit + 0x30a)*4) = 0` でクリアする（`0x1f0d4`–`0x1f0de`）。
したがって 2K では `0x0b` は bit0=1 になる（既存 `gr_i2c.rs` の `BEFORE_TSOUT` と整合）。

### 3.3 TSID / mode 選択（`Demod set2`）

`FUN_00022be4`（`0x22be4`）→ system 1 は `FUN_00022d4c`（`0x22d4c`）の `param_2 == 0x2d` 分岐。

| # | page | reg | 操作 | 値 | 命令 |
|---|---|---|---|---|---|
| 1 | 2 | `0x31` | **maskwrite** mask `0x08` | `0x00`（bit3=0 → 絶対 TSID 選択） | `0x22db6`/`0x22db8` |
| 2 | 2 | `0x2f` | write | `(tsid >> 8) & 0xff` | `0x22de4` |
| 3 | 2 | `0x30` | write | `tsid & 0xff` | `0x22e0c` |

### 3.4 SeqStart（復調シーケンサ起動）

`Tune_ISDB_S_SeqStart`（`0x1f254`）→ `FUN_00022880` → `FUN_000228b0`（`0x228b0`）の system 1 分岐。

| # | page | reg | 値 | 命令/根拠 |
|---|---|---|---|---|
| 1 | 2 | `0x92` | `0x3f` | `FUN_000228b0` case 1 |

（ISDB-T では page1 `0xf4=0x9f` が sequencer start。ISDB-S は page2 `0x92=0x3f` が対応する。）

### 3.5 `regset_ISDBS`（168 entries。system 1 の全設定）

`regset_ISDBS` — VA `0x36c70` / file offset `0x26c70`, 168 entries + `ff ff ff`:

| page | slave7 | entries (index:reg=value) |
|---|---|---|
| `0x00` | `0x10` | `0:00=18`, `1:01=8e`, `2:02=21`, `3:03=d4`, `4:04=21`, `5:05=42`, `6:07=10`, `7:0b=00`, `8:0c=00`, `9:10=00`, `10:11=00`, `11:12=00`, `12:16=00`, `13:18=00`, `14:1a=10`, `15:1c=00`, `16:1d=01`, `17:2a=c4`, `18:2b=e0`, `19:2c=00`, `20:39=1c`, `21:3a=29`, `22:3d=1c`, `23:3e=29`, `24:41=1c`, `25:42=29`, `26:45=1c`, `27:46=29`, `28:63=17`, `29:65=50`, `30:6c=82`, `31:84=00`, `32:85=05`, `33:97=e1`, `34:c6=01`, `35:c9=00`, `36:cd=41`, `37:d9=00` |
| `0x01` | `0x11` | `38:01=12`, `39:02=ab`, `40:03=96`, `41:04=78`, `42:07=26`, `43:09=1b`, `44:13=02`, `45:16=9f`, `46:1c=00`, `47:23=01`, `48:24=02`, `49:2c=12`, `50:3e=00`, `51:46=68`, `52:50=70`, `53:54=0c`, `54:55=e0`, `55:61=00`, `56:68=f0`, `57:69=04`, `58:6a=03`, `59:7c=02`, `60:e5=00` |
| `0x02` | `0x12` | `61:11=12` |
| `0x03` | `0x13` | `62:00=50`, `63:09=0e`, `64:0b=14`, `65:18=00`, `66:19=00`, `67:23=c9`, `68:24=02`, `69:25=30`, `70:26=69`, `71:27=1f`, `72:3d=ed`, `73:3e=96`, `74:3f=4a`, `75:42=c8`, `76:43=2d`, `77:44=48`, `78:49=07`, `79:4a=f0`, `80:58=57`, `81:90=ed`, `82:9d=65`, `83:e1=08`, `84:e2=ae` |
| `0x04` | `0x14` | `85:00=b0`, `86:05=60`, `87:06=5f`, `88:07=27`, `89:08=b0`, `90:09=ff`, `91:0a=c0`, `92:0b=aa`, `93:0c=bb`, `94:0d=ee`, `95:0e=aa`, `96:0f=aa`, `97:10=10`, `98:11=62`, `99:12=0d`, `100:13=77`, `101:14=1d`, `102:15=1c`, `103:21=00`, `104:22=a0`, `105:2e=00`, `106:2f=0c`, `107:30=3b`, `108:32=0c`, `109:34=b1`, `110:35=ed`, `111:36=60`, `112:37=cc`, `113:38=6c`, `114:39=7d`, `115:3a=b1`, `116:3b=ed`, `117:43=97`, `118:46=a5`, `119:47=7e`, `120:48=1d`, `121:49=a5`, `122:4a=76`, `123:4b=a5`, `124:4c=76`, `125:4d=a4`, `126:4e=76`, `127:50=55`, `128:53=4f`, `129:54=c0`, `130:55=73`, `131:62=4b`, `132:79=12`, `133:7a=e2`, `134:7b=00`, `135:81=40`, `136:c9=85` |
| `0x05` | `0x15` | `137:e5=00` |
| `0x07` | `0x17` | `138:02=03`, `139:1e=c0`, `140:1f=a8`, `141:20=64`, `142:21=64`, `143:33=10`, `144:3b=07`, `145:72=00`, `146:74=80`, `147:76=80`, `148:8d=00`, `149:91=17`, `150:d3=1f`, `151:d4=ff` |
| `0x08` | `0x18` | `152:15=01`, `153:16=02`, `154:1a=c6`, `155:1d=6e`, `156:1f=a8`, `157:20=f0`, `158:2f=01`, `159:43=00`, `160:4a=02`, `161:4b=02`, `162:4c=02`, `163:4d=02`, `164:4e=bb`, `165:4f=bb`, `166:50=bb`, `167:51=bb` |

### 3.6 `regset_TLV_SPI_1BIT`（4K/TLV 側の参考。2K では書かれない）

`regset_TLV_SPI_1BIT` — VA `0x3b3cc` / file offset `0x2b3cc`, 12 entries + `ff ff ff`:

| page | slave7 | entries (index:reg=value) |
|---|---|---|
| `0x00` | `0x10` | `0:d9=00`, `1:0b=00`, `2:0c=00`, `3:13=04`, `4:16=21`, `5:18=00` |
| `0x07` | `0x17` | `6:0a=00`, `7:72=02`, `8:73=15`, `9:7b=67`, `10:7d=81`, `11:8d=00` |

### 3.7 `regset_Recommend_DIFF`（起動後、unit ごとに1回だけ）

`regset_Recommend_DIFF` — VA `0x38c30` / file offset `0x28c30`, 62 entries + `ff ff ff`:

| page | slave7 | entries (index:reg=value) |
|---|---|---|
| `0x00` | `0x10` | `0:00=94`, `1:01=8c`, `2:05=52`, `3:07=10`, `4:10=00`, `5:11=00`, `6:14=08`, `7:16=44`, `8:18=80`, `9:1c=01`, `10:1d=01`, `11:85=05`, `12:97=e1`, `13:bc=25`, `14:c6=01`, `15:cd=41` |
| `0x01` | `0x11` | `16:77=00` |
| `0x05` | `0x15` | `17:02=40`, `18:10=15`, `19:2c=9d`, `20:2d=ff`, `21:2e=02`, `22:30=cb`, `23:34=39`, `24:36=67`, `25:37=62`, `26:3b=30`, `27:3c=18`, `28:41=37`, `29:43=37`, `30:4d=e4`, `31:6e=4c`, `32:70=ff`, `33:75=c9`, `34:77=14`, `35:87=69`, `36:88=4a`, `37:89=31`, `38:8a=24`, `39:8b=1d`, `40:8c=1d`, `41:95=10`, `42:96=08`, `43:97=0f`, `44:e5=80` |
| `0x07` | `0x17` | `45:02=03`, `46:1e=c0`, `47:1f=a8`, `48:20=64`, `49:21=64`, `50:32=81`, `51:33=10`, `52:3b=07`, `53:74=80`, `54:76=80`, `55:91=17`, `56:95=18`, `57:96=19`, `58:97=2e`, `59:98=2f`, `60:99=34`, `61:9a=35` |

## 4. 2K 衛星選局の呼び出し順序（確定）

```
[FEM task] msg TuningStart (2K BS/CS)
  └─ vFedLocalMain (0x12761) → FUN_00014518 (0x14518)
       ├─ (system type == 2 のとき) FUN_00018ca0 (0x18ca0)
       │    └─ Tune_ISDB_S_SetTsid (0x1ef4c)
       │         1. eSysTakeSemaphore(0x3c5)
       │         2. FUN_0001af24(1, 0x10/0x20/0x40)
       │              └─ x_fin_isdbt(unit)  … RF addr8 0xc0/0xc2/0xc4 へ 5 writes
       │         3. ctx+0x04 = 1 (system=ISDB-S)
       │            ctx+0x18 = 25_000_000 (ADC 25 MHz。delay ではない)
       │            ctx+0x1c = 0
       │         4. DAT_0003672c==1 のときだけ FUN_00022434 (set_system)
       │              └─ Recommend_DIFF (初回のみ 62 writes)
       │              └─ regset_ISDBS (168 writes, page 0..8)
       │         5. ctx[0xc28]=0 → FUN_00026d74 (DMD_set_tsout system=1)
       │              └─ maskwrite page0 0x0b bit0=1
       │              └─ maskwrite page0 0xcc bit7=1
       │         6. FUN_00022be4 → FUN_00022d4c(param=0x2d)
       │              └─ maskwrite page2 0x31 bit3=0
       │              └─ page2 0x2f = TSID[15:8]
       │              └─ page2 0x30 = TSID[7:0]
       │         7. eSysGiveSemaphore(0x3c5)
       ├─ FUN_00019180 (0x19180)  SetFreq
       │    └─ clamp 950_000..2_150_000 kHz
       │    └─ FUN_0001f1a4 (0x1f1a4) → FUN_0001ad80 (0x1ad80)
       │         └─ x_tune_isdbs(if_khz, 1, unit) … RF 15 writes + 10 ms
       │    └─ 失敗時 eSysSleepTask(0x1e) 後に 1 回だけ再試行 (0x148a8–0x148b4)
       └─ FUN_00019228 (0x19228) SeqStart
            └─ FUN_0001f254 → FUN_00022880 → FUN_000228b0
                 └─ page2 0x92 = 0x3f
```

- 上位の tune timeout は **2000 ms**。`vFedmMain` は 2K BS/CS の TuningStart メッセージ `0x20` で
  `eStmExSetTimerAfter(0x1c7, 0x36, path, …, 2000)` を張り、TuningEnd `0x21` で解除する。
  ADBS/4K はメッセージ `0x2a` で timer `0x3b` を 2000 ms で張り、`0x30` で解除する。
- ロック判定は 2 段。TMCC テーブル取得 `FUN_00018d40`（`0x18d40`）と TS lock `FUN_00018d1c`（`0x18d1c`）。
  どちらも最終的に **page2 `0x5e` bit5** を見る（`FUN_00023224` case 1: `0x12:0x5e` を読み `>>5 &1`）。
- BS サーチ（`fFedBsSearchEnable`）では `FUN_00014968`（`0x14968`）が TSID 表を読み、
  `0xffff` と `0x40f1` 以外の最初の TSID へ `SetTsid` を再実行する。再試行 timer は 100 ms。

---

## 5. 待ち時間・poll 間隔・timeout

| 段 | 値 | 単位/意味 | 根拠 |
|---|---|---|---|
| RF PLL 周波数 `0x10` 書込み後 | `10` → `usleep(10*1000)` = **10 ms** | 実時間 | `x_tune_isdbs` `0x1c75e`–`0x1c760`、`FUN_0001cba0` `0x1cba0` |
| ISDB-T power-on `0x9b` 後 | 10 ms | 実時間 | `x_pon_isdbt` が `FUN_0001df6c(10)` を呼ぶ（`0x1df6c` = `usleep(ms*1000)`） |
| SetFreq 失敗時の再試行前 | `eSysSleepTask(0x1e)` | **tick 単位（1 tick の実時間は未確定）** | `0x148a8`–`0x148ae` |
| FEM 割込み進捗待ち | `eSysSleepTask(1)` × 最大 `0x65`(101) 回 | 同上 | `FUN_00014518` 冒頭ループ |
| 2K BS/CS tune timeout | **2000 ms** | ms（`eStmExSetTimerAfter`） | `vFedmMain` `0xaf29`、msg `0x20`→timer `0x36` |
| ADBS/4K tune timeout | **2000 ms** | ms | `vFedmMain`、msg `0x2a`→timer `0x3b`、失効時 `[FEM] ADBS Tuning Timeout` |
| CN/status poll | **2000 ms** | ms | `vFedLocalMain` msg `0x11ff9` 再 arm、`vFedAdbsLocalMain` も 2000 |
| BS サーチ再試行 | **100 ms** | ms | `vFedLocalMain` msg `0x39` 再 arm |
| FEM "TSin" dummy 待ち | 100 ms poll / **3000 ms** 上限 | ms | `vFedmMain` msg `0x44`（`< 3000` 判定と 100 ms 再 arm） |
| `vFedLocalSubAPoll` 周期 | `eSysSleepTask(0x32)` | tick（未確定） | `0x19f79` |
| `ctx+0x18` の `25_000_000` | **ADC クロック 25 MHz**（delay ではない） | Hz | `0x1f06c`–`0x1f072` が literal `0x1f158` を `ldrd`。実体は file `0x17158` = `40 78 7d 01` = `0x017d7840` = 25,000,000 |

`eSysSleepTask` / `eStmExSetTimerAfter` の tick 分解能は libsysapi.so 内にあり、本 Phase では未取得。
**2000 ms / 100 ms は `eStmExSetTimerAfter` の引数が ms であることを前提**にしている（ABI.md の他 ioctl も ms 表記）。
tick 系（`0x1e`, `0x32`, `0x65`）は ms と断定しない。

---

## 6. 周波数変換（BCD → LNB IF → RF PLL）

`FUN_00014518`（`0x14518`）でメッセージの BCD 周波数を展開する。

1. `param_1[1]` の BCD ニブルを十進化（例 `0x01172748` → `1_172_748`）: `0x145c0`–`0x14672`。
2. `×10` して kHz 相当（`11_727_480`）にし、`- 0xa2eef0`（= `10_678_000`、LNB LO 10.678 GHz）:
   即値は `0x14726`–`0x1472e` の `movw #0x1110 / movt #0xff5d`（`0xff5d1110` = `-0xa2eef0`）。
3. その後、`tFemTuningSubParam[path].+8 == 1` のときだけ帯域別の補正
   （`+0x25760` または `-0x12bb0`）が入る: `0x1475c`–`0x14856`。
   **この補正が BS 2K パスに適用されるかは未確定**（path 依存フラグ）。
4. 結果 `if_khz` を `eFed_ISDB_S_TunerSetChannel` が `950_000..2_150_000` に clamp（`FUN_0001ad80`）。
5. `x_tune_isdbs` が `if_khz >> 2` を 3 バイト little-endian で `0x10` に書く（`0x1c712`–`0x1c75a`）。

例（BS 朝日 `0x01172748`）: `11_727_480 - 10_678_000 = 1_049_480 kHz` → `>>2 = 262_370 = 0x0400E2`
→ `0x10 = [e2, 00, 04, ff, 00, 01]`。補正 `+0x25760` が入る場合は `1_202_920 kHz` → `[ba, 96, 04, ff, 00, 01]`。
**どちらが実際に使われるかは Phase 1 で I2C を観測して確定する。**

---

## 7. 根拠アドレス一覧

VA → file offset: `.text`/`.rodata` は `VA - 0x8000`、`.data` は `VA - 0x10000`。

### 7.1 関数

| ラベル | VA | file off | 根拠・備考 |
|---|---|---|---|
| `x_pon_isdbs` (`FUN_0001c3d4`) | `0x1c3d4` | `0x143d4` | 文字列 `0x2be28` “x_pon_isdbs: take semaphore error” の xref |
| `x_tune_isdbs` (`FUN_0001c500`) | `0x1c500` | `0x14500` | 文字列 `0x2be70` |
| `x_fin_isdbs` (`FUN_0001c7fc`) | `0x1c7fc` | `0x147fc` | 文字列 `0x2beb8` / `0x2bedc` |
| `write_tuner` (`FUN_0001ca3c`) | `0x1ca3c` | `0x14a3c` | 文字列 `0x2bf00` `sut_pj784_isdbs.c:write_tuner` |
| `read_tuner` (`FUN_0001caf0`) | `0x1caf0` | `0x14af0` | 文字列 `0x2bf34` |
| RF unit→8-bit addr (`FUN_0001c964`) | `0x1c964` | `0x14964` | unit 0/1/2 → `0xc0/0xc2/0xc4` |
| RF 1-byte write (`FUN_0001c934`) | `0x1c934` | `0x14934` | `FUN_0001c964(...,len=1)` |
| `usleep(ms*1000)` (`FUN_0001cba0`) | `0x1cba0` | `0x14ba0` | — |
| `x_pon_isdbt` (`FUN_0001d394`) | `0x1d394` | `0x15394` | 文字列 `0x2c6b4` |
| `x_fin_isdbt` (`FUN_0001d99c`) | `0x1d99c` | `0x1599c` | 文字列 `0x2c794` |
| ISDB-T unit→8-bit (`FUN_0001dd30`) | `0x1dd30` | `0x15d30` | unit 0/1/2 → `0xc0/0xc2/0xc4` |
| ISDB-T `usleep(ms*1000)` (`FUN_0001df6c`) | `0x1df6c` | `0x15f6c` | — |
| 復調 write（7bit→`<<1`） (`FUN_0001e020`) | `0x1e020` | `0x16020` | `I2c_Write(0, param_1<<1, …)` |
| 復調 read (`FUN_0001dfc4`) | `0x1dfc4` | `0x15dc4` | `I2c_Read(0, param_1<<1, …)` |
| 復調 maskwrite (`FUN_0001e084`) | `0x1e084` | `0x16084` | 式は本文 1 章。vtable には Thumb ビット付き `0x1e085` が入る |
| vtable 設定 (`FUN_0001e17c`) | `0x1e17c` | `0x1617c` | `ctx->vtbl` (ctx+0xc70) に write/mask/read を設定（値は Thumb ビット付き `0x1e021/0x1e085/0x1dfc5`） |
| unit ctx 初期化 (`FUN_0001e200`) | `0x1e200` | `0x16200` | `malloc(0x24)`、`stride 0xc78` |
| `sc1501a_Init` (`FUN_0001ac80`) | `0x1ac80` | `0x12c80` | `x_pon_isdbt` → `x_pon_isdbs` を unit ごとに実行 |
| standby dispatch (`FUN_0001af24`) | `0x1af24` | `0x12f24` | `(1, 0x10/0x20/0x40)` → `x_fin_isdbt(0/1/2)` |
| `Tune_ISDB_S_SetTsid` (`FUN_0001ef4c`) | `0x1ef4c` | `0x16f4c` | 文字列 `0x2cb80` |
| `Tune_ISDB_S_SetFreq` (`FUN_0001f1a4`) | `0x1f1a4` | `0x171a4` | 文字列 `0x2cc5c` |
| `Tune_ISDB_S_SeqStart` (`FUN_0001f254`) | `0x1f254` | `0x17254` | 文字列 `0x2cca4` |
| `Tune_ISDB_S_IsTMCCLocked` (`FUN_0001f3ac`) | `0x1f3ac` | `0x173ac` | 文字列 `0x2cd18` |
| `Tune_ISDB_S_IsTSLocked` (`FUN_0001f51c`) | `0x1f51c` | `0x1751c` | 文字列 `0x2cd80` |
| 2K/S3 統合 (`FUN_0001ec98`) | `0x1ec98` | `0x16c98` | 同じ helper を 1 関数で順に呼ぶ |
| `set_system` (`FUN_00022434`) | `0x22434` | `0x1a434` | system 1 → `regset_ISDBS` |
| regset 適用 (`FUN_00025f60`) | `0x25f60` | `0x1d560` | triple `(page,reg,value)`、`+0x10` |
| `DMD_set_tsout` (`FUN_00026d74`) | `0x26d74` | `0x1d674` | ISDB-S: `0x0b`/`0xcc` maskwrite |
| `Demod set2` dispatch (`FUN_00022be4`) | `0x22be4` | `0x1a3e4` | system 1 → `FUN_00022d4c` |
| TSID (`FUN_00022d4c`) | `0x22d4c` | `0x1a34c` | `0x31` mask, `0x2f`, `0x30` |
| tune end (`FUN_000228b0`) | `0x228b0` | `0x1a8b0` | system 1: `0x12:0x92=0x3f` |
| lock 判定 (`FUN_00023224`) | `0x23224` | `0x1b224` | system 1: `0x12:0x5e` bit5 |
| TMCC 判定 (`FUN_0002710c`) | `0x2710c` | `0x1f10c` | system 1: `0x12:0x5e` bit5 |
| SetTsid wrapper (`FUN_00018ca0`) | `0x18ca0` | `0x10ca0` | `fFedDemodulatorSetTs` |
| SetFreq wrapper (`FUN_00019180`) | `0x19180` | `0x11180` | clamp + `FUN_0001f1a4` |
| SeqStart wrapper (`FUN_00019228`) | `0x19228` | `0x11228` | `fFedTunerSeqStart` |
| 選局要求 (`FUN_00014518`) | `0x14518` | `0xc518` | SetTsid→SetFreq→SeqStart |
| BS サーチ (`FUN_00014968`) | `0x14968` | `0xc968` | TSID 表読み・再選局 |
| poll task (`vFedLocalSubAPoll`) | `0x19f79` | `0x11f79` | `eSysSleepTask(0x32)` |
| `vFedLocalMain` | `0x12761` | `0xa761` | timer 2000/100/3000 の一部 |
| `vFedAdbsLocalMain` | `0xebe1` | `0x6be1` | ADBS timer 2000 |
| `vFedmMain` | `0xaf29` | `0x2f29` | ADBS/2K timeout timer、`0x9c4`=2500 |

### 7.2 データ / 文字列

| 内容 | VA | file off |
|---|---|---|
| `regset_ISDBS`（168 triples） | `0x36c70` | `0x26c70` |
| `regset_TLV_SPI_1BIT`（12 triples） | `0x3b3cc` | `0x2b3cc` |
| `regset_Recommend_DIFF`（62 triples） | `0x38c30` | `0x28c30` |
| 復調 ctx base（unit0） | `0x3b938` | `0x2b938` |
| ADC 25 MHz 定数プール | `0x1f158` | `0x17158` |
| system 切替フラグ `DAT_0003672c` | `0x3672c` | `0x2672c` |
| RF unit 番号 `DAT_0003b718` / `DAT_0003b720` | `0x3b718` / `0x3b720` | `0x2b718` / `0x2b720` |
| `"Tune_ISDB_S_SetTsid %d %d\n"` | `0x2cb80` | `0x24b80` |
| `"Tune_ISDB_S_SetFreq %d %d\n"` | `0x2cc5c` | `0x24c5c` |
| `"Tune_ISDB_S_SeqStart %d\n"` | `0x2cca4` | `0x24ca4` |
| `"Tune_ISDB_S_IsTMCCLocked…"` | `0x2cd18` | `0x24d18` |
| `"Tune_ISDB_S_IsTSLocked[%d]"` | `0x2cd80` | `0x24d80` |
| `"x_pon_isdbs: …"` | `0x2be28` | `0x23e28` |
| `"x_tune_isdbs: …"` | `0x2be70` | `0x23e70` |
| `"x_fin_isdbs: …"` | `0x2beb8` | `0x23eb8` |
| `"sut_pj784_isdbs.c:write_tuner…"` | `0x2bf00` | `0x23f00` |

---

## 8. 未確定項目

1. **libi2c.so のアドレス規約**: 呼び出し2箇所（RF=8-bit 直接、復調=7-bit を `<<1`）から「`I2c_Write` は 8-bit 期待」と
   判断したが、libi2c.so 本体を取得していないため直接確認ではない。libvixs の `i2c_write` は 8-bit 入力→`>>1` で一致。
2. **`eSysSleepTask` / `eStmExSetTimerAfter` の tick 分解能**: `0x1e`, `0x32`, `0x65` の実時間は未確定。
   2000 ms / 100 ms は timer 引数が ms である前提。
3. **IF 補正ブロックの適用条件**: `tFemTuningSubParam[path].+8 == 1` のときに `+0x25760` または `-0x12bb0` が入る。
   BS 2K パスでこれが 1 か 0 か未確定。BS 朝日の `0x10` が `[e2,00,04,…]` か `[ba,96,04,…]` かは実測が必要。
4. **`DAT_0003672c` の意味**: `SetTsid` が「1 のときだけ set_system を実行して 0 に戻す」。書き込み元は
   `FUN_00020b2c`（ISDB-S3 系）にもある。system 変更フラグと推定するが、初期値・遷移の全経路は未確定。
5. **`x_pon_isdbs` が起動時のみか**: `callers` は `sc1501a_Init` の1件のみ（REA）。ただし間接呼び出しの可能性はゼロではない。
6. **復調 page/reg の機能名**: 値はバイト単位で確定したが、各レジスタの意味（PLL/AGC/等）はベンダ名なし。
   page2 `0x92=0x3f` を「SeqStart」と推定している（ISDB-T の page1 `0xf4=0x9f` との対応から）。
7. **TS lock と TMCC lock が同じ `0x5e` bit5**: コード上は両方とも同じビット。ベンダの層分け（TMCC vs TS）は
   コード内で区別されていないため、意味の差は未確定。
8. **`FUN_0001df90`（vtable[0]）**: REA が関数として解決できず未解析。初期化/クローズの実体は未確定。
9. **`regset_PSEQ_ISDBS3`（4527 B）**: ISDB-S3 用の triple 列（`page0 0xf4/0xf5/0xf6` の内部シーケンス）。
   2K では未使用。S3 の遅延は本表には含まれない（triple のみ）。
10. **ADBS 経路の RF IC**: 4K も同じ `x_tune_isdbs(is_s3=2)` を使うことは確定。ただし ADBS タスクが
    `FUN_00014518` と同じ順序で呼ぶかは未確認（`FUN_0001ec98` 統合関数も存在する）。
11. **`x_read_agc_isdbs` / `x_tuner_calcRSSI_isdbs`**: 本 Phase の対象外。AGC/RSSI レジスタは未抽出。
12. **別ビルド差**: ISDB-T 側（`x_pon_isdbt`, `x_fin_isdbt`）は参照 Rust とバイト一致したが、
    ISDB-S 用 `regset_ISDBS` は参照ビルドの表と同一か未比較（参照側の同表データを本リポジトリに持っていない）。

---

## 9. Phase 1（実機照合）前に必要な動的計測

実機 I/O は Phase 1 以降。以下は**読み取り専用または既知の純正動作の観測**に限定する。

1. **起動直後のスナップショット**: RF `0x60/0x61/0x62` の `0x41,0x45,0x0c,0x7b` を読み、`x_pon_isdbs` の4値が
   起動後に残っているか確認。復調 `0x10..0x18` の page 0/1/2/7/8 主要レジスタも同時に保存。
2. **2K 選局の I2C トレース**: `i2c-0` 上の `0xc0` と `0x10..0x18` をロジックアナライザまたは
   `i2c-stub`/ftrace で捕捉し、RF 15 列 + 復調（Recommend 62 + ISDBS 168 + tsout 2 + TSID 3 + SeqStart 1）
   の順序を一致確認。特に `0x10 = [e2,00,04,…]` か `[ba,96,04,…]`（未確定項目3）を確定。
3. **初回 vs 2回目**: 起動後はじめての 2K 選局で `Recommend_DIFF` 62 列 + `regset_ISDBS` 168 列が出るか、
   2回目以降で省略されるかをトレースで確認（`DAT_0003672c` の意味）。
4. **ロック時間の実測**: `0x12:0x5e` bit5 を 5–10 ms 間隔で read-only ポーリングし、SeqStart から lock までの
   実時間を分布で取得。2000 ms timeout に対する余裕を確認。
5. **10 ms 待ちの妥当性**: RF `0x10` 書込み直後 10 ms 以内 / 以後で `0x12:0x5e` を読んだ差を確認。
   （製品コードは 10 ms 固定。PLL lock が 10 ms 未満かは未検証。）
6. **`eSysSleepTask` の実時間**: SetFreq 失敗時の再試行間隔をトレースで測り、`0x1e` tick の ms 換算を確定。
7. **0xcc / 0x0b / 0x13 の残留**: 4K→2K 切替後に `0x10:0xcc` が `0xc0` か `0x80` か、`0x13` が `0x04` か `0x00` かを
   読み取り、既存 `gr_i2c.rs` の TLV 残留モデルをこのビルドで再確認。
8. **unit→物理 FED の対応**: `0xc0/0xc2/0xc4` と筐体の FED1/2/3、`0x10/0x30/0x50` の対応を、
   各 FED を1本ずつ選局して RSSI/TS の変化で確認。
9. **セマフォ**: `0x3c5` / `0x3c6` の attach-only 取得が製品デーモンと共存できるか、既存 `gr_i2c.rs` と同条件で確認。
   （作成・初期化は禁止。）
10. **タイムアウト再現**: アンテナ入力なしで 2000 ms 後に `[FEM] ADBS Tuning Timeout` 相当の状態遷移が出るか、
    status/err コードを確認。

---

## 10. 付録

復調テーブルの実体は本文 3.5–3.7 に掲載した（`regset_ISDBS` / `regset_TLV_SPI_1BIT` /
`regset_Recommend_DIFF` の全 triple と各 entry の VA）。

