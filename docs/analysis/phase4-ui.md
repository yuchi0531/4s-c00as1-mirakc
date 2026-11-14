# Phase 4 — 物理リモコン/HDMI なしで製品 UI を 4K 選局状態へ持っていく

対象: SHARP 4S-C00AS1（ViXS XCode6, ARMv7, Linux 2.6.31, `/usr/bin/abs`）。
前提: リモコン受光部は破損、HDMI は接続済みだが入力操作なし。
制約: 実機は read-only。書込みは **Q_UIM（qid 0x7F）へのキー注入メッセージのみ**。
プロセス停止・ptrace・LD_PRELOAD・ストレージ削除は行わない。

根拠タグ: `[D]` 逆アセンブリ、`[G]` 大域変数/GOT、`[S]` 文字列、
`[O]` 実機 read-only 観測、`[R]` 関数境界/呼び出し関係、`[INF]` 推論、`[未]` 未確定。

---

## 0. 結論サマリ

1. **UIM のキーコード表を確定した。**
   abs の `.data` に **キーコード 0x000–0x13D を名前文字列へ引く 318 エントリの表**がある
   （`0x1079FAC`、各 4 バイトは `.rodata 0xE5458C..` の `RC_*` 文字列へのポインタ）[D]。
   UIM のキー入力処理 `0x715F04`（msg `0x115C` 側）は、キー入力ログでこの表を
   `table[key]` として直接引く（`0x716142`）[D]。
   `0x2044` 側の `0x715DD4` も同じ RC コード空間を受け、
   同じ同義キー正規化表を持つため、**両経路で同じ keyCode を使える** [D]。
   したがって **0x000=RC_POWER, 0x001–0x00A=数字1–9,0, 0x00C=RC_CH_UP,
   0x00D=RC_CH_DOWN, 0x015=RC_ENTER, 0x016–0x019=上下左右, 0x13B=RC_4K** が確定 [D]。

2. **Phase 3 の「0x30/0x31/0x32 = 上下」という推定は誤り。**
   この表では `0x030=RC_TV`, `0x031=RC_RADIO`, `0x032=RC_DATA`。
   UIM の `0x715DD4` が行う変換は「同義キーの正規化」であり、
   `0x30→0x50(RC_MEDIA_T)`, `0x31→0x51(RC_MEDIA_R)`, `0x32→0x52(RC_MEDIA_D)`,
   `0x14→0x15(RC_ENTER)`, `0x0E→0x53`, `0x0B→0x4F`, `0x11→0x54`,
   `0x2F→0x32`, `0xB9/0x125→0x9C` など、メディア/入力系の別名を畳むための表 [D]。
   ナビゲーションキーは変換されず、そのまま `0x016–0x019`。

3. **これまでのキー走査が電源を入れられなかった理由を特定した。**
   UIM のキー受付フィルタ `0x743B34` は「直前キー」を `0x1D2AD48` に保持し、
   **入力キー == 直前キー のときはリピート扱いで捨てる**（`0x743BB6` → `0x743C10` return 0）[D]。
   起動直後の直前キーは **0**、ゲート `0x107B800` は **1** [O]。
   したがって **最初に送った `key=0x00 (RC_POWER)` は「直前キー 0 と同じ」で捨てられる**。
   先行して別キー（または power の release）を 1 回送り、直前キーを非 0 にしてから
   power press を送る必要がある [D][O][INF]。

4. **UI 画面を read-only で読む方法を確立した。**
   abs は DirectFB 1.4（`system=devmem`, `mode=1920x1080`）で、
   `/etc/directfbrc` の `video-phys=0x29000000`, `video-length=0x17000000` [O]。
   DirectFB の fusion 共有メモリ `fusion.0.1` 内のサーフェスレコード（offset `0xCF30`, `0xDB30`）に
   **1920x1080 / pitch 0x1E00 の 2 枚の ARGB サーフェス**の仮想アドレス・物理アドレス・pool offset がある [O]。
   - OSD/グラフィックス面: **phys `0x3C884000`**（VA `0x5DA21000`, pool offset `0x13884000`）
   - 背景（不透明グレー）面: **phys `0x3F814000`**（VA `0x609B1000`, pool offset `0x16814000`）
   画素は `devmem`（4K ページ mmap）で 32bit 読みできる。`dd if=/dev/mem` はこの領域で
   `Bad address (EFAULT)`、`/dev/xcfb_disp` の read はブロックする [O]。
   OSD 面を 64x36 グリッドで採取すると、上下バー・文字様グリフ・黄色アイコンが読め、
   **画面が実際に描画されていることを確認できる**（§4）。[O]

5. **4K 選局のキー列（推奨）**
   電源投入が必要な場合: `power release (0x00,type2)` → `power press (0x00,type0)`。
   既に UI が生きている場合: `0x13B (RC_4K)` または `0x010 (RC_MENU)` で 4K メニュー、
   `0x00C (CH UP)` / `0x00D (CH DOWN)` / 数字 `0x001–0x00A` / `0x015 (ENTER)` で選択。
   詳細は §5。**power キーは原則として手順の最後**（standby からの電源投入試験だけは
   §5.2 の順序で最初に送る例外）[INF]。

6. それでも UI が動かない場合の別経路は §5.5（IP Control / FPM 電源メッセージ / `0x6004`）。

---

## 1. 実機 read-only スナップショット（2026-11-13）

### 1.1 重要な状態変化（再起動の観測）

解析中に実機が**再起動した**（`/proc/uptime` が 0 min に戻り、20:27 に up 0 min）[O]。
dmesg に panic/oops はなく、watchdog の初期化ログのみ [O]。
映像メモリ（`0x3C884000` 周辺）を `devmem` で 2304 点連続サンプリングした数分後の出来事であり、
**因果関係は未確定**だが、`/dev/mem` の mmap を短時間に大量に繰り返すのはリスクがある [O][未]。
以後の採取は「1 セッション 64 点以下・間隔を空ける」に制限する（§4.4）。

再起動後（fresh boot）の確認値:

| 項目 | 値 | 根拠 |
|---|---|---|
| abs PID | 279（この起動では 279。PID は起動ごとに変わる） | [O] |
| Q_UIM qid / msqid | `0x7F` / `98307`（`/tmp/bdh_out_msgh.lst` の `127-98307`） | [O] |
| Q_UIM lspid/lrpid | 279 / 476（`abs.T_UIM`）、qnum=0 | [O] |
| `bAdsbInitialized`（abs VA `0x02027184`） | **1** | [O] |
| UIM キーゲート `0x107B800` | **1** | [O] |
| UIM 直前キー `0x1D2AD48` | **0**（起動直後） | [O] |

> 注: 再起動で msqid は同じ 98307 だったが、これは製品起動スクリプトが同じ key で
> キューを作り直しているため。**msqid をハードコードせず `bdh_out_msgh.lst` から解決する**。

### 1.2 表示系デバイス

```
crw-rw---- 251,0 /dev/xcfb_disp
crw-rw---- 250,0 /dev/xcfb_g2d
/proc/fb: 空、/sys/class/graphics: 空（fb0 は無い）
/proc/devices: 250 xcfb_g2d, 251 xcfb_disp
```

abs は `/dev/xcfb_disp` を fd 16/18、`/dev/xcfb_g2d` を fd 17 で保持 [O]。
DirectFB のモジュール:

```
/usr/vixs/lib/directfb-1.4-0/systems/libdirectfb_devmem.so
/usr/vixs/lib/directfb-1.4-0/gfxdrivers/libdirectfb_vixs.so
/usr/vixs/lib/directfb-1.4-0/wm/libdirectfbwm_default.so
```

`/etc/directfbrc` [O]:

```
tmpfs=/dev/shm
mode=1920x1080
no-init-layer
video-phys=0x29000000
video-length=385875968   # 0x17000000
module-dir=/usr/vixs/lib/directfb-1.4-0
system=devmem
wm=default
```

abs の `/proc/279/maps` に次がある [O]:

```
4a19d000-6119d000 rw-s 29000000 00:05 1026  /dev/mem
```

これは DirectFB devmem システムの video memory 全体（phys `0x29000000`–`0x40000000`）の
ユーザ空間マッピング。DirectFB のサーフェスはこの中に置かれる。

---

## 2. キーコード表（確定）

### 2.1 表の所在と根拠

- キー名テーブル: abs VA **`0x1079FAC`**、**0x13E = 318 エントリ**、各 `u32`。
  各エントリは `.rodata` の `0xE5458C`–`0xE554B0` にある `RC_*` 文字列への絶対ポインタ [D]。
- 使用箇所: UIM のキー入力処理 `0x715F04` 内、`0x716142`:
  ```c
  if (key <= 0x13D)  name = keyNameTable[key];  // 0x1079FAC
  else               name = "unknown";          // 0xE... のフォールバック
  eDbgPrintf(0, fmt, name);
  ```
  [D]
- テーブルの内容は ELF の `.data` に**初期値として存在**する（実行時構築ではない）ため、
  オフラインで完全に復元できる [D]。
- 参考: `.rodata 0xE58A28` 以降にも別の `RC_*` 文字列群（`RC_4K`, `RC_TOOL`, …）があるが、
  これは FPM 側のログ用の部分集合で、**キー番号表ではない**。番号の根拠には使わない。

### 2.2 主要キー（実務で使うもの）

| keyCode | 名前 | 機能 | 根拠 |
|---:|---|---|---|
| `0x000` | `RC_POWER` | 電源トグル | [D] |
| `0x001`–`0x009` | `RC_No1`–`RC_No9` | 数字 1–9 | [D] |
| `0x00A` | `RC_No0` | 数字 0 | [D] |
| `0x00B` | `RC_SID` | 同義→`0x4F` | [D] |
| `0x00C` | `RC_CH_UP` | CH アップ | [D] |
| `0x00D` | `RC_CH_DOWN` | CH ダウン | [D] |
| `0x00E` | `RC_ES` | 同義→`0x53` | [D] |
| `0x010` | `RC_MENU` | メニュー | [D] |
| `0x011` | `RC_OSD_OFF` | 同義→`0x54 (RC_DISPLAY)` | [D] |
| `0x013` | `RC_INFO` | 情報 | [D] |
| `0x014` | `RC_SELECT` | 同義→`0x15 (RC_ENTER)` | [D] |
| `0x015` | `RC_ENTER` | 決定 | [D] |
| `0x016` | `RC_UP` | 上 | [D] |
| `0x017` | `RC_DOWN` | 下 | [D] |
| `0x018` | `RC_LEFT` | 左 | [D] |
| `0x019` | `RC_RIGHT` | 右 | [D] |
| `0x020` | `RC_POW_ON` | 電源オン専用 | [D] |
| `0x021` | `RC_POW_OFF` | 電源オフ専用 | [D] |
| `0x025` | `RC_BACK` | 戻る | [D] |
| `0x02F` | `RC_ADD_DATA` | 同義→`0x32 (RC_DATA)` | [D] |
| `0x030` | `RC_TV` | 同義→`0x50 (RC_MEDIA_T)` | [D] |
| `0x031` | `RC_RADIO` | 同義→`0x51 (RC_MEDIA_R)` | [D] |
| `0x032` | `RC_DATA` | 同義→`0x52 (RC_MEDIA_D)` | [D] |
| `0x05F` | `RC_RM_POWER` | リモコン電源（別名） | [D] |
| `0x072` | `RC_MAIN_POWER` | 主電源（別名） | [D] |
| `0x0B9` | `RC_REC_STOP` | 同義→`0x9C (RC_STOP)` | [D] |
| `0x125` | `RC_REC_ABORT` | 同義→`0x9C (RC_STOP)` | [D] |
| `0x13B` | `RC_4K` | 4K | [D] |

### 2.3 UIM の同義キー正規化（`0x715DD4` / `0x715F04`）

`msg 0x2044` のハンドラ `0x715DD4` は body 先頭の keyCode を次のように畳む [D]:

| 入力 | 出力 | 出力の名前 |
|---:|---:|---|
| `0x0B` | `0x4F` | `RC_CH_IN` |
| `0x0E` | `0x53` | `RC_ES_A` |
| `0x11` | `0x54` | `RC_DISPLAY` |
| `0x14` | `0x15` | `RC_ENTER` |
| `0x2F` | `0x32` | `RC_DATA` |
| `0x30` | `0x50` | `RC_MEDIA_T` |
| `0x31` | `0x51` | `RC_MEDIA_R` |
| `0x32` | `0x52` | `RC_MEDIA_D` |
| `0xB9` | `0x9C` | `RC_STOP` |
| `0x125` | `0x9C` | `RC_STOP` |

モデル/地域状態（`[state+0x16]` が `0xAE`/`0x89`/`0x215`）のときだけ追加で [D]:

| 入力 | 出力 |
|---:|---:|
| `0xA0` | `0x9D (RC_FF)` |
| `0x11A` | `0x9E (RC_REW)` |

**ナビゲーション/数字/CH/4K はここでは変換されない**（そのまま §2.2 の番号で届く）。

### 2.4 全キーコード表（0x000–0x13D）

```
000 RC_POWER          001 RC_No1            002 RC_No2            003 RC_No3          
004 RC_No4            005 RC_No5            006 RC_No6            007 RC_No7          
008 RC_No8            009 RC_No9            00A RC_No0            00B RC_SID          
00C RC_CH_UP          00D RC_CH_DOWN        00E RC_ES             00F RC_GUIDE        
010 RC_MENU           011 RC_OSD_OFF        012 RC_FLASHBACK      013 RC_INFO         
014 RC_SELECT         015 RC_ENTER          016 RC_UP             017 RC_DOWN         
018 RC_LEFT           019 RC_RIGHT          01A RC_PAGE_UP        01B RC_PAGE_DOWN    
01C RC_PAGE_RIGHT     01D RC_PAGE_LEFT      01E RC_NET_SEL        01F RC_ADJ_CLR      
020 RC_POW_ON         021 RC_POW_OFF        022 RC_RSV_CANCEL     023 RC_ADJ_IN       
024 RC_CONF_RGST      025 RC_BACK           026 RC_HELP           027 RC_RSV_CONF     
028 RC_COLOR_BUTTON1  029 RC_COLOR_BUTTON2  02A RC_COLOR_BUTTON3  02B RC_COLOR_BUTTON4
02C RC_CC             02D RC_WIDE           02E RC_TIME_CH        02F RC_ADD_DATA     
030 RC_TV             031 RC_RADIO          032 RC_DATA           033 RC_PID          
034 RC_OUTPUT         035 RC_525I           036 RC_525P           037 RC_750P         
038 RC_1125I          039 RC_EPG_ON         03A RC_EPG_OFF        03B RC_No11         
03C RC_No12           03D RC_No13           03E RC_No14           03F RC_No15         
040 RC_No16           041 RC_ILINK          042 RC_VIDEO_TEST1    043 RC_VIDEO_TEST2  
044 RC_SOUND_TEST1    045 RC_SOUND_TEST2    046 RC_MAIN_VERSION   047 RC_MODE_SW      
048 RC_MTX_MODE       049 RC_PROG_FAVORITE1  04A RC_PROG_FAVORITE2  04B RC_PROG_FAVORITE3
04C RC_PROG_FAVORITE4  04D RC_DOT            04E RC_FAVORITE       04F RC_CH_IN        
050 RC_MEDIA_T        051 RC_MEDIA_R        052 RC_MEDIA_D        053 RC_ES_A         
054 RC_DISPLAY        055 RC_INPUT          056 RC_SECRET_PASSWD  057 RC_SECRET_DIAG  
058 RC_SECRET_CONFIG_EXIT  059 RC_ADJ_1          05A RC_ADJ_2          05B RC_ADJ_F        
05C RC_IF_AMP         05D RC_SOUND_TEST3    05E RC_VIDEO_TEST3    05F RC_RM_POWER     
060 RC_ADJ_3          061 RC_EPG            062 RC_DEPG           063 RC_D            
064 RC_END            065 RC_IN_CHANGE      066 RC_ZOOM           067 RC_BR_CHANGE    
068 RC_MDISP          069 RC_OP_CHANGE      06A RC_STAND_STILL    06B RC_MEMORY       
06C RC_CANCEL_RESERVE  06D RC_ZOOM_L         06E RC_ZOOM_R         06F RC_D2           
070 RC_D3             071 RC_D4             072 RC_MAIN_POWER     073 RC_BS_HOLD      
074 RC_MR_2DISP_LB    075 RC_MR_1DISP       076 RC_TUNER          077 RC_D_VIDEO      
078 RC_S_VIDEO        079 RC_JAPANESE       07A RC_ENGLISH        07B RC_SCREEN_NORMAL
07C RC_SCREEN_WIDE    07D RC_SCREEN_FULL    07E RC_SCREEN_CINEMA  07F RC_TBD_1        
080 RC_TBD_2          081 RC_TBD_3          082 RC_TBD_4          083 RC_TBD_5        
084 RC_TBD_6          085 RC_TBD_7          086 RC_TBD_8          087 RC_TBD_9        
088 RC_TBD_10         089 RC_TBD_11         08A RC_TBD_12         08B RC_A_TER        
08C RC_D_TER          08D RC_BSD            08E RC_CS1            08F RC_CS2          
090 RC_SPTV           091 RC_VIDEO_TEST4    092 RC_VIDEO_TEST5    093 RC_VIDEO_TEST6  
094 RC_VIDEO_TEST7    095 RC_VIDEO_TEST8    096 RC_VIDEO_TEST9    097 RC_VIDEO_TEST10 
098 RC_VIDEO_TEST11   099 RC_EP             09A RC_PLAY           09B RC_PAUSE        
09C RC_STOP           09D RC_FF             09E RC_REW            09F RC_REC          
0A0 RC_SLOW           0A1 RC_REPEAT         0A2 RC_LIST           0A3 RC_RECMODE      
0A4 RC_RECSAVE        0A5 RC_RESCONF        0A6 RC_CATV_BS        0A7 RC_UV           
0A8 RC_BACK2          0A9 RC_ENTER2         0AA RC_RESET          0AB RC_PLAY2        
0AC RC_STOP2          0AD RC_REC2           0AE RC_FF2            0AF RC_REW2         
0B0 RC_CH_UP2         0B1 RC_CH_DOWN2       0B2 RC_PAUSE2         0B3 RC_G_CODE       
0B4 RC_EJECT          0B5 RC_CODE_CHANGE    0B6 RC_CS             0B7 RC_TIMESHIFT    
0B8 RC_REC_STILL      0B9 RC_REC_STOP       0BA RC_PREV           0BB RC_NEXT         
0BC RC_REPLAY         0BD RC_SKIP           0BE RC_PLAYLIST       0BF RC_DUBBING      
0C0 RC_MONITOR        0C1 RC_CHAPTER        0C2 RC_INFO_DISP      0C3 RC_HDD          
0C4 RC_DVD            0C5 RC_LIGHT          0C6 RC_LOCK           0C7 RC_LCD_DVD      
0C8 RC_LCD_HDD        0C9 RC_BLUE_KEY       0CA RC_GREEN_KEY      0CB RC_RED_KEY      
0CC RC_YELLOW_KEY     0CD RC_STOP_KEY       0CE RC_PLAY_KEY       0CF RC_REC_KEY      
0D0 RC_PAUSE_KEY      0D1 RC_REW_KEY        0D2 RC_FF_KEY         0D3 RC_DATA_KEY     
0D4 RC_POWER_ON       0D5 RC_POWER_OFF      0D6 RC_AUX1           0D7 RC_AUX2         
0D8 RC_AUX3           0D9 RC_TEST_PAT1      0DA RC_TEST_PAT2      0DB RC_TEST_PAT3    
0DC RC_VIDEO1         0DD RC_VIDEO2         0DE RC_VIDEO3         0DF RC_DIAG_START   
0E0 RC_DIAG_END       0E1 RC_DIAG_STATUS    0E2 RC_INSPECT1       0E3 RC_INSPECT2     
0E4 RC_INSPECT3       0E5 RC_SHIP           0E6 RC_SW_INSPECT     0E7 RC_PVR_INSPECT  
0E8 RC_VID_INSPECT    0E9 RC_ADJUST1        0EA RC_ADJUST2        0EB RC_ADJUST3      
0EC RC_FACT_MODE      0ED RC_FACT_ON        0EE RC_FACT_OFF       0EF RC_FACT_INIT    
0F0 RC_FACT_TEST1     0F1 RC_FACT_TEST2     0F2 RC_FACT_TEST3     0F3 RC_FACT_TEST4   
0F4 RC_FACT_RES1      0F5 RC_FACT_RES2      0F6 RC_FACT_RES3      0F7 RC_FACT_RES4    
0F8 RC_FACT_SP1       0F9 RC_FACT_SP2       0FA RC_FACT_SP3       0FB RC_FACT_SP4     
0FC RC_FACT_SP5       0FD RC_FACT_SP6       0FE RC_FACT_SP7       0FF RC_FACT_SP8     
100 RC_FACT_SP9       101 RC_FACT_CHECKER   102 RC_UV1            103 RC_UV2          
104 RC_UV3            105 RC_UV4            106 RC_UV5            107 RC_UV6          
108 RC_UV7            109 RC_UV8            10A RC_UV9            10B RC_UV10         
10C RC_UV11           10D RC_UV12           10E RC_SHARP13        10F RC_SHARP14      
110 RC_SHARP15        111 RC_SHARP16        112 RC_DVD_KEY        113 RC_RSV_LIST     
114 RC_RST_CONNSETTING  115 RC_BGREC_STOP     116 RC_BGREC2_STOP    117 RC_DISP_MODE    
118 RC_DVD_MENU       119 RC_NETWORK        11A RC_SLOWR,         11B RC_BGREC_ILINK  
11C RC_ONE_REC        11D RC_ONE_RESERVE    11E RC_ONE_DUBBING    11F RC_SETUP_MENU   
120 RC_NEWS           121 RC_PINP           122 RC_AQUOS          123 RC_CHANGE_MEDIA 
124 RC_REMAINTIME     125 RC_REC_ABORT      126 RC_ADV            127 RC_MYDISC       
128 RC_INTERNET       129 RC_VOLUME_UP      12A RC_VOLUME_DOWN    12B RC_FONT         
12C RC_DELETE         12D RC_REC_FRONT      12E RC_REC_STOP_FRONT  12F RC_PLAY_FRONT   
130 RC_STOP_FRONT     131 RC_SHDD_LOCK      132 RC_SHDD_UNLOCK    133 RC_SEARCH       
134 RC_WMON           135 RC_DIGEST_LIST    136 RC_DRAMA          137 RC_EMANUAL      
138 RC_TOOL           139 RC_CAPTION_SEARCH  13A RC_CAPTION        13B RC_4K           
13C RC_MAX            13D RC_CODE_TEST    
```

> 上表は `0x1079FAC` の 318 エントリをそのまま名前解決したもの。
> 0x13C は `RC_MAX`、0x13D は `RC_CODE_TEST`（上限番兵）。

---

## 3. キー経路とメッセージ（確定/推定）

### 3.1 Q_UIM のメッセージ形式

`/tmp/bdh_out_msgh.lst` の `127-<msqid>` で Q_UIM を解決する [O]。
SysV メッセージの mtext は製品の `SysVWire` 形式 [D]（`rust/t4s-core/src/tuner.rs` と一致）:

```
+0x00 long  mtype = 1
+0x04 u32   unused[2] = 0
+0x0C u32   self_ptr  = 0
+0x10 u32   inner_head= 0
+0x14 u32   msgId
+0x18 u32   len
+0x1C u32   src = 1
+0x20 u8    payload[64]
mtext size = 0x5C (92)
```

キー注入は次の 2 通り [D]:

| msgId | 用途 | handler |
|---:|---|---|
| `0x2044` | FPM → UIM の通常キー入力 | `0x715DD4`（`0x70DDE2` → `0x7151D6`） |
| `0x115C` | 別系統の UIM キー入力（名称未確定、body は同じキー形式） | `0x715F04`（`0x708D68` → `0x7151EC`） |

どちらも body は同じ `{u32 keyCode; u32 type;}`（len=8）[D]。
`0x2044` のほうが FPM と同じ経路であり、まずこれを試す。

### 3.2 `type` の意味

- `0x715DD4`: body+4 が **2 のときだけ** `key = ~key` にしてからフィルタへ渡す [D]。
- FPM 側デコード `0x512C54` は raw key word の種別 `kind` を次のように body type へ写す [D]:
  - `kind 1` → type `0`（押下）
  - `kind 2` → type `1`
  - `kind 0` → type `2`（離上）
- さらに FPM は **type==1 のとき 0x2044 を送らない**（`0x513652`–`0x513658`）[D]。
- 以上から: **type 0 = 押下、type 2 = 離上、type 1 = リピート/長押し系（FPM は UIM へ送らない）** [D][INF]。
- 注入では **type 0 のみ**を使い、必要なら release に type 2 を使う。

### 3.3 UIM のキー受付ゲート（重要）

`0x715DD4` は正規化後、キーを `0x743B34` に渡す。`0x743B34` は [D]:

```
gate = *(u32*)0x107B800          // 実測 1
prev = *(u32*)0x1D2AD48          // 実測 0（起動直後）
key  = 入力キー

if (key == 0x8F) key = 0x8E;     // RC_CS2 -> RC_CS1 の別名畳み
if (gate != 0) {
    if (key == prev) goto repeat;         // 0x743B56/0x743B78
    *(u32*)0x1D2AD4C = 0;                 // 0x743B7C（別フラグ）
    prev = key;                           // 0x743B84（prev = *(u32*)0x1D2AD48）
    ... state チェック ...
    return 1;                             // 受理
} else {
    if (key == prev) return 0;
    *(u32*)0x1D2AD48 = 0x13C;             // 0x743B60..0x743B68（タイマ）
    return 0;                             // ゲート閉
}
repeat:  // key == prev
    if (key <= 0x15) { ...; return 0; }   // 0x743BC2 → 0x743C10
    if (key <= 0x19) { repeat カウンタ; }
    ...
```

- **起動直後は gate=1, prev=0** [O]。
- したがって **最初の `key=0x00 (RC_POWER)` は prev と一致し、repeat 経路で捨てられる** [D][O]。
  これが「0x00 を送っても電源が入らない」直接の理由 [有力]。
- 非 0 キーを 1 回でも受理させると prev がその値になり、次の `key=0x00` は受理される [D]。
- `type=2` の release は `~key` なので、`key=0x00,type=2` は `0xFFFFFFFF` として受理され、
  **prev を非 0 にできる**（他キーを押さずに primer になる）[D]。

### 3.4 FPM / IR の raw → RC 変換（参考）

物理リモコンが復活した場合、または FPM 生コードを直接送る場合の表。
`0x512C54` が raw key word をデコードし、`0x513D50` が 16x16 の u32 行列を引く [D]:

- raw word の下位ニブル `kind`（0/1/2）→ body type（2/0/1）。
- raw word が `(w>>28)==1` の形式では、`kind=(w>>26)&3`、keyCode=`w & 0xFFFFFF`（24bit）を
  そのまま使う（IP Control 系の「Host Key Code」形式）[D]。
- `(w>>8)&0xFF` を `kb` とすると、`row = kb & 0xF`, `col = kb >> 4`。
- `sub = (w>>16)&0xF`、`SysConf_IsCPLD()` で 4 つの行列を選択。
- 行列アドレス（`.data`、各 16x16 u32 = 0x400 バイト）[D]:
  - `0x1056F80`（sub!=1, CPLD=0）
  - `0x1057380`（sub!=1, CPLD=1）
  - `0x1057780`（sub==1, CPLD=0）
  - `0x1057B80`（sub==1, CPLD=1）

主要キーの raw バイト（`0x1057780` の例）[D]:

| 機能 | raw byte |
|---|---:|
| POWER | `0x80` |
| 数字 1–9,0 | `0x40,0xC0,0x20,0xA0,0x60,0xE0,0x10,0x90,0x50,0xD0` |
| CH UP | `0xB0` |
| CH DOWN | `0x70` |
| MENU | `0x48` |
| ENTER | `0x98` |
| UP/DOWN/LEFT/RIGHT | `0x58,0xD8,0xB8,0x38` |
| BACK | `0x9C` |

（別モデル行列 `0x1057B80` では POWER=`0x17`, CH UP=`0xD1`, CH DOWN=`0xD2` など。）
**行列値そのものが RC コード**であり、UIM へは行列値を送ればよい [D]。

---

## 4. UI 画面の読み取り（read-only キャプチャ）

### 4.1 DirectFB のサーフェス発見

`fusion.0.1`（`/dev/shm` の通常ファイル、73 KB）を `base64` で取得して調べると、
サーフェスレコードが 2 つある [O]:

| fusion offset | data VA | phys | pool offset | pitch | 解釈 |
|---|---:|---:|---:|---:|---|
| `0xCF30` | `0x609B1000` | **`0x3F814000`** | `0x16814000` | `0x1E00` (7680) | 背景面（不透明グレー） |
| `0xDB30` | `0x5DA21000` | **`0x3C884000`** | `0x13884000` | `0x1E00` (7680) | OSD/グラフィックス面（alpha 0 中心） |

近傍に `1920 (0x780)` と `1080 (0x438)`、`0x418C04`（ARGB 系フォーマット）がある [O]。
`phys = 0x29000000 + pool_offset` が `/dev/mem` マッピングの式と一致する [O][INF]。

> この 2 つの pool offset は**その起動時の値**。DirectFB のアロケータは起動順に依存するため、
> 再起動後は `base64 /dev/shm/fusion.0.1` を取得し直し、
> `pitch=0x1E00` を持つレコード（`va, phys, pool_offset` の 4 word）を再探索してから
> `devmem` のアドレスを決める。背景面/OSD 面の識別は「全面 `0xFFA4A4A4` か、
> alpha 0 が多いか」で行う [O]。

### 4.2 実測した画面内容（64x36 グリッド、OSD 面）

`0x3C884000 + y*0x1E00 + x*4` を 30 px 間隔で採取した結果 [O]:

- 上端と下端に **横幅いっぱいのバー**（`0xFF5A5A5A` / `0xFF323232`）
- 中央は大半が **`0x00000000`（alpha 0 = 透明）**
- 上から 16–19 行目に **文字様のグリフ**（`0xFF5A5A5A` 等）
- 26–27 行目に **矩形ボックス**
- 黄色系 `0xFFF5AC08` が 19 点（アイコン/強調表示）

一方、背景面 `0x3F814000` は全面 `0xFFA4A4A4` の一様グレー [O]。
したがって **画面 = グレー背景 + OSD オーバーレイ**であり、
OSD 面の非透明画素を数えることで「UI が描画されている/変化した」を判定できる [有力]。

参考: 64x36=2304 点の内訳は
`0x00000000` 1812 点、`0xFF5A5A5A` 251 点、`0xFF323232` 188 点、
`0xFFF5AC08` 19 点、`0xFF000000` 16 点、`0xFF404040` 5 点、`0xFFFFFFFF` 3 点 ほか [O]。
観測時は standby 相当で、全画面メニューではなく「バー + 小さな表示」だった。

### 4.3 読み取りコマンド（4K ページ mmap の devmem）

```sh
# Q_UIM の msqid
grep '^127-' /tmp/bdh_out_msgh.lst
grep ' 98307 ' /proc/sysvipc/msg    # qnum=0 を確認

# OSD 面の 1 画素（x,y）
devmem $((0x3C884000 + y*0x1E00 + x*4)) 32

# 32x18 の粗いグリッド（576 点）
for y in 0 60 120 180 240 300 360 420 480 540 600 660 720 780 840 900 960 1020; do
  for x in 0 60 120 180 240 300 360 420 480 540 600 660 720 780 840 900 960 1020 \
           1080 1140 1200 1260 1320 1380 1440 1500 1560 1620 1680 1740 1800 1860; do
    devmem $((0x3C884000 + y*0x1E00 + x*4)) 32
  done
done
```

### 4.4 制約と安全策

| 手段 | 結果 | 根拠 |
|---|---|---|
| `devmem <phys> 32`（mmap） | **読める**（背景/OSD とも） | [O] |
| `dd if=/dev/mem bs=4096 skip=...` | **`Bad address (EFAULT)`**（映像メモリ） | [O] |
| `dd if=/dev/xcfb_disp` | read がブロック（戻らない） | [O] |
| `/dev/fb0`, `/sys/class/graphics/fb0` | 存在しない | [O] |
| `/proc/279/mem` | ptrace 相当のため**使わない** | 制約 |

- 全画面 8.29 MB を `devmem` で読むには 200 万回のプロセス起動が必要で非現実的。
  **粗いグリッド（数十〜数百点）で状態判定**し、必要なら後日、mmap 対応の read-only ツールを
  別途用意する（本 Phase ではアップロードしない）。
- 2304 点の連続 `devmem` の数分後に再起動が観測された [O]。因果は未確定だが、
  **1 セッション 64 点以下、間に `sleep 1` を入れる**運用にする。
- 画面が変わらない場合、サーフェスが scanout でない可能性もある。次点は背景面 `0x3F814000`。

---

## 5. 4K 選局までのキー列

### 5.1 前提

- `bAdsbInitialized=1`（再起動後も確認）[O]。
- Q_UIM は稼働、qnum=0 [O]。
- UIM の直前キーは 0（起動直後）[O]。
- 電源キーは `0x00 RC_POWER`。専用の `0x20 RC_POW_ON` / `0x21 RC_POW_OFF` もある [D]。

### 5.2 電源投入（standby から）

**手順 P（推奨・他キーを触らない）**

1. `{key=0x00, type=2}`（power release）を 1 回。
   - `0x715DD4` が `~0 = 0xFFFFFFFF` に変換 → `0x743B34` が受理し、直前キーを非 0 にする [D]。
2. 1 秒待つ。`/proc/sysvipc/msg` で Q_UIM qnum=0 を確認。
3. `{key=0x00, type=0}`（power press）を 1 回。
4. 5–10 秒待つ。判定:
   - abs の thread に `MIXER` / `Sink` / `buf_buf` が増える（HDMI 視聴時）[O]。
   - OSD 面の非透明画素数が変わる（§4.3 のグリッド）。
   - `Q_UIM` qnum=0 のまま。

**手順 Q（代替・release が効かない場合）**

1. `{key=0x25 (RC_BACK), type=0}` を 1 回（primer。standby では無害）。
2. 1 秒待つ。
3. `{key=0x00, type=0}` を 1 回。

> 注意: いずれも **最初に 0x00 を単独で送らない**。prev=0 のため捨てられる（§3.3）。

### 5.3 UI が出た後の 4K 選局

1. `{key=0x13B (RC_4K), type=0}` を 1 回。
   - `0x715F04` の受理ホワイトリストに `0x13B` が明示されている（`0x71605E`）[D]。
   - 4K 専用ボタン。4K/2K 切替または 4K チャンネルリストを開く [有力]。
2. 変化がなければ `{key=0x10 (RC_MENU), type=0}` → `{key=0x16/0x17 (UP/DOWN)}` でメニュー移動
   → `{key=0x15 (RC_ENTER)}`。
3. チャンネル選択:
   - `{key=0x0C (CH UP)}` / `{key=0x0D (CH DOWN)}`
   - 数字 `{0x001..0x00A}` + `{0x15 (ENTER)}`
   - `{key=0x13 (RC_INFO)}` で選局状態を確認。
4. 判定は毎回 §4.3 の OSD グリッド + `Q_ADSB_TUM`/bank2（Phase 2b の read-only 手順）。

> 4K のチャンネル番号は `ch.json` の 45168–45330 が「チャンネル識別子」で、
> UI の数字入力とは一致しない [未]。UI 上で BS4K の一覧から選ぶのが確実 [有力]。

### 5.4 power キーの扱い

- **電源を切る**操作は手順の最後に置く。既に視聴中の場合は、4K 選局の確認が終わるまで
  `0x00/0x20/0x21` を送らない。
- **standby からの電源投入**は、それ自体が最初の操作になるので §5.2 の順序
  （release → press）でのみ行う例外とする。その場合も **power press は 1 回だけ**、連打しない。

### 5.5 UI が動かない場合の別経路

1. **`0x115C` 経路**: `0x715F04` は `0x2044` とは別の入力系統。
   同じ body 形式なので、`0x2044` で反応がなければ `0x115C` を 1 回試す [D]。
2. **IP Control（Q_IPC_MAN qid 0x31D）**: 実機では未使用（lspid/lrpid=0）。
   サーバ起動 `0x1203` の受信者がいないため、現状は使えない [O]。
3. **FPM 電源メッセージ**: FPM は電源キーを UIM と別に扱う可能性がある [未]。
   `vFpmMain` / `eFpmPowerOffKey` 周辺の文字列
   `FPM : eFpmPowerOffKey() msgFpmRequestUimRestartParticular RC_POWER` [S] があり、
   FPM が直接 `Q_FPM`/`Q_*` に電源要求を出す経路が存在する [有力]。
4. **`0x6004` 直接選局**: Phase 2b のとおり、per-device block の Tune が一致していれば
   UI を介さず選局できる。block が古い場合は `0x6005` では Tune は入らない [D]。
5. **プロセス内 `0xC66D50(dev)` / `0xC66A84(dev,6,&t92)`**: ptrace/LD_PRELOAD 不可のため
   本 Phase では実行しない（watchdog リスク）[O]。

---

## 6. 失敗時の復旧

| 操作 | リスク | 復旧 |
|---|---|---|
| Q_UIM `0x2044` / `0x115C` キー注入 | 低（製品の通常キー経路） | `0x25 (BACK)` を 1 回、または `0x10 (MENU)`→`0x25` |
| 誤って電源オフ | 低 | §5.2 の primer + `0x00 press` で再投入 |
| メニュー迷子 | 低 | `0x25 (BACK)` 連打を避け 1 回ずつ。`0x010 MENU` で再入 |
| `0x6004` 送信 | 低（製品も使う） | 通常選局で上書き。復元不要 |
| 映像メモリの大量 `devmem` | **中**（再起動を観測） | 間隔を空ける。再起動後は msqid/prev key を再確認 |
| abs/adsb のメモリ書換、ptrace、LD_PRELOAD | 高（watchdog） | 電源再投入。`bAdsbInitialized` を再確認 |

- 工場出荷系キー（`0x0E0`–`0x101` 付近、`RC_FACT_*`）は**送らない**。
- `0x21 RC_POW_OFF` も、復旧手段が確保できるまで送らない。

---

## 7. 未確定事項

1. **`key=0x00` が UIM から実際に電源投入シーケンスを起動するか**。
   `0x743B34` で受理されることは確定したが、その後 UIM メインループが
   power をどう dispatch するかは未確定 [未]。
   FPM が電源を直接持つ可能性もある [有力]。
2. **`0x2044` と `0x115C` のどちらが power に必要か** [未]。
3. **`type=1` の正確な意味**（リピート/長押し）。FPM は UIM へ送らない [D][未]。
4. **OSD 面 `0x3C884000` が実際の scanout か**。背景面 `0x3F814000` との合成順は未確定 [未]。
5. **BS4K の UI チャンネル番号 / TSID / NID / SID / RF** [未]（Phase 1/2b の資料と突合が必要）。
6. **再起動の原因**（映像メモリの連続 `devmem` との因果）[未]。
7. **`RC_4K (0x13B)` の正確な動作**（4K 切替か、4K リストか）[未]。
8. **FPM 電源メッセージの msgId / body**（`eFpmPowerOffKey` 周辺）[未]。

---

## 8. 次の実機試験（最小・read-only + Q_UIM のみ）

### 8.1 事前確認（書込みなし）

```sh
# 1) Q_UIM の msqid と状態
grep '^127-' /tmp/bdh_out_msgh.lst
grep ' 98307 ' /proc/sysvipc/msg          # qnum=0, lrpid=476 を確認

# 2) 電源/初期化状態
#    bAdsbInitialized: VA 0x02027184 を pagemap→devmem
#    UIM gate:          VA 0x107B800
#    UIM prev key:      VA 0x1D2AD48   (0 なら primer 必須)
```

### 8.2 試験 1: primer の効果確認（書込みは Q_UIM のみ）

1. `{key=0x00, type=2}` を 1 回送る。
2. `/proc/sysvipc/msg` で qnum=0、`0x1D2AD48` が `0xFFFFFFFF` になったことを確認。
3. `{key=0x00, type=0}` を 1 回送る。
4. 5–10 秒後に §4.3 の 32x18 グリッドと thread 一覧を確認。

期待: prev が非 0 になり、power press が受理される。
失敗時: §5.5 の `0x115C` 経路へ。

### 8.3 試験 2: ナビゲーションの実地確認（UI が出てから）

1. `{0x10 MENU,0}` → グリッド変化を確認。
2. `{0x17 DOWN,0}` を 1 回 → 変化。
3. `{0x15 ENTER,0}` → 変化。
4. `{0x25 BACK,0}` で戻る。

### 8.4 試験 3: 4K

1. `{0x13B,0}` → 4K メニュー/切替の変化。
2. 必要なら `{0x10 MENU,0}` → `{0x16/0x17}` → `{0x15}`。
3. `{0x0C CH UP,0}` または数字 + `{0x15}`。
4. bank2 / TSID / OSD で選局を確認（Phase 2b の read-only 手順）。

> 各ステップは **1 キーずつ、間に 1 秒以上**。連打しない。
> 画面確認は 64 点以下の `devmem` グリッドに留める（§4.4）。

---

## 9. 根拠アドレス一覧

### 9.1 abs（キー）

| 内容 | アドレス |
|---|---|
| キー名テーブル（318 entries, u32[0x13E]） | `0x1079FAC` |
| テーブルが指す文字列群 | `0xE5458C`–`0xE554B0` |
| UIM のキー名参照（`table[key]`） | `0x716142` |
| Q_UIM dispatch `0x2044` | `0x70DDE2` → `0x7151D6` → `0x715DD4` |
| Q_UIM dispatch `0x115C` | `0x708D68` → `0x7151EC` → `0x715F04` |
| UIM 同義キー変換 | `0x715DFA`–`0x715E76` |
| UIM キーゲート | `0x743B34` |
| gate 変数 | `0x107B800` |
| 直前キー | `0x1D2AD48` |
| repeat 分岐 / 破棄 | `0x743BB6` / `0x743C10` |
| FPM キーデコード | `0x512C54` |
| FPM → UIM `0x2044` 送信 | `0x51365C`–`0x513670` |
| FPM type 変換 | `0x512D4E`–`0x512D76` |
| raw 行列デコード | `0x513D50` |
| raw 行列（4 表） | `0x1056F80`, `0x1057380`, `0x1057780`, `0x1057B80` |
| `BhvTvAdsbTuneChangeChannel` | `0xC66A84` |
| 4K wrapper | `0xC66D50` |
| `0x6004` sender | `0x457858` |

### 9.2 表示（DirectFB / 実機）

| 内容 | 値 |
|---|---|
| `/etc/directfbrc` video-phys / length | `0x29000000` / `0x17000000` |
| abs の `/dev/mem` mapping | `0x4A19D000`–`0x6119D000`, offset `0x29000000` |
| OSD 面 | phys `0x3C884000`, VA `0x5DA21000`, pool `0x13884000`, pitch `0x1E00` |
| 背景面 | phys `0x3F814000`, VA `0x609B1000`, pool `0x16814000`, pitch `0x1E00` |
| fusion レコード位置 | `/dev/shm/fusion.0.1` offset `0xCF30`, `0xDB30` |
| 画面モード | 1920x1080, 32bpp（pitch 7680） |

### 9.3 文字列

| 文字列 | 備考 |
|---|---|
| `[HCI_KEY_CODE]: Code = %s` | UIM のキー名ログ |
| `FPM : eFpmPowerOffKey() msgFpmRequestUimRestartParticular RC_POWER` | FPM 電源経路の存在示唆 |
| `RC_POWER`, `RC_CH_UP`, `RC_CH_DOWN`, `RC_4K` | キー名テーブル |

---

## 10. 付記: なぜ Phase 3 の走査は失敗したか

- 走査した `0x0E, 0x11, 0x14, 0x2F, 0x30, 0x31, 0x32, 0x8F, 0x9D, 0x9E, 0xB9, 0x125` は、
  **ナビゲーション/電源ではない**（`RC_ES`, `RC_OSD_OFF`, `RC_SELECT`, `RC_ADD_DATA`,
  `RC_TV`, `RC_RADIO`, `RC_DATA`, `RC_CS2`, `RC_FF`, `RC_REW`, `RC_REC_STOP`, `RC_REC_ABORT`）[D]。
  `RC_SELECT 0x14` は `RC_ENTER 0x15` に畳まれるが、これは「決定」であって電源ではない。
- `0x00–0x1FF` 走査は **0x00 を最初に送っており**、起動直後の prev=0 と衝突して
  power が捨てられた [D][O]。
- その後のキー（0x01–）は受理されるが、standby のままでは UI が応答しない。
  `MIXER`/`Sink` スレッドの有無だけで判定していたため「何も起きない」と見えた [INF]。
- 正しい順序は **primer → power press → UI キー**。§5.2/§8 を参照。
