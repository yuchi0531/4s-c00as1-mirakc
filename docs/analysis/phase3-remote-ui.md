# Phase 3 — 物理リモコン/HDMI なしで製品 UI を操作する経路

対象: SHARP 4S-C00AS1（ViXS XCode6, ARMv7, Linux 2.6.31）。
**実機アクセスは read-only のみ**（`.tmp/dev.sh` 経由の telnet、`/proc`、`/proc/sysvipc/msg`、
`/dev/mem` 読み取り、短時間の TCP 接続）。書込み・再起動・ptrace・LD_PRELOAD は行っていない。
ホスト側は `.tmp/oem/abs`, `adsb`, `tuner`, `dlnactl`, `rrc`,
`.tmp/oem/libtpl_adsb_ext.so`（実機から read-only で取得）を解析した。

根拠タグ: `[D]` 逆アセンブリ、`[G]` GOT/大域変数、`[S]` 文字列、
`[O]` 実機 read-only 観測、`[R]` 関数境界/呼び出し関係、`[INF]` 推論、`[未]` 未確定。

---

## 0. 結論サマリ

1. **製品 UI のキー入力は UIM の 1 本の IPC メッセージに集約されている。**
   FPM（前面パネル/リモコン制御、qid `0x7B`）はキーコードを
   **`Q_UIM`（qid `0x7F`）へ msgId `0x2044`、8 バイト body** で送る [D]。
   body は `{u32 keyCode; u32 type}` で、`type==0` が押下、`type==2` が離す側と推定 [D][INF]。
   UIM はこれを受けて keyCode を内部コードへ変換し、Bhv のキー処理へ渡す [D]。
   したがって **外部プロセスから `eSysSendMessage(0x7F, 0x2044, body, 8)` を送れば
   製品 UI にキーイベントを注入できる可能性が高い** [有力]。ただしキーコード表は未確定。

2. **IP Control（ネットワーク経由のキー入力）の受け口は存在するが、現在は停止している。**
   abs は `Q_IPC_MAN`（qid `0x31D`）にサーバ起動 msg `0x1203` / 停止 msg `0x1201` を送る [D]。
   キー受信部 `0xa8fa34` は `[IpControl] Linux -> Host Key Code=0x%03X Sub Code=0x%x` を出し、
   **`Q_FPM`（qid `0x7B`）へ msg `0x11E59` / `0x11E69`** を送る [D]。
   しかし実機の `Q_IPC_MAN`（msqid 15401430）は **lspid=lrpid=0、qnum=0 で一度も受信されていない** [O]。
   IP Control サーバは起動していない（ネットワーク listen も 8001/2323/40772 のみ）[O]。
   現状のままではネットワークからキーを送る経路は使えない。

3. **DLNA/Dixim/8001 は選局・キー操作の経路ではない。**
   - TCP 8001 の所有者は **adsb**（PID 263）で、`libtpl_adsb_ext.so` の
     `TourbillonServer/1.0` 系 Web サーバ（`pnWebServer_*`, `mmt_start_server`）[O][D]。
     GET/バナーに応答せず、受け付けた接続は未読のまま CLOSE_WAIT に残る [O]。
   - `dlnactl` は DLNA サーバ管理用で、**eSysReceiveMessage を import しない**（ファイル駆動）[D]。
     `/tmp/dlnactl/dmsctl` と `/tmp/dlnactl/diximcds` をポーリングし、
     DMS の start/restart、タイトル登録/削除、電源断、USB mount/unmount を実行する [S][D]。
     ContentDirectory の SOAP は DMS への問い合わせで、TV 自身の選局 API ではない [D]。
   - `dlnactl` の DMS HTTP 接続先は **127.0.0.1:53207**（`0xCFD7`）[D]。
     現在 53207 の listener は無く、DMS 自体が起動していない [O]。
   - AVTransport / RenderingControl / SetAVTransportURI の文字列は
     abs/dlnactl のどちらにも見つからない [S]。

4. **4K 選局の製品側入口は `BhvTvAdsbTuneChangeChannel`（abs:`0xC66A84`）。**
   4K 専用 wrapper は `0xC66D50(dev)` で、保存済み 92 バイト構造体を読んで
   `0xC66A84(dev, 6, &t92)` を呼ぶ [D]。
   `bAdsbInitialized==0` のときは引数を保存し、**100 ms 後に msg `0x12096` で自分自身を再試行**する
   （`0xC669F8`）[D]。つまり製品 UI 経由の選局は「初期化待ち」を内部で行う。
   実機の `bAdsbInitialized`（abs VA `0x02027184`）は現在 **1** [O]。
   ただし外部から生の `0x6004` を送るだけでは、TUM が参照する
   **per-device 24 バイト block の Tune が更新されない**（Phase 2b の既知の blocker）[D]。

5. **物理リモコンなしで 4K を選ぶ最小手順（現時点で確実なもの）は
   「製品 UI のキーイベントを UIM に注入して UI を操作する」か、
   「製品プロセス内で `0xC66D50`/`0xC66A84` を呼ぶ」の 2 系統。**
   後者は ptrace 不可・LD_PRELOAD で watchdog 再起動という制約下では実行が難しい [O]。
   前者はキーコード表の確定が残る [未]。

---

## 1. 実機 read-only スナップショット（2026-11-13）

### 1.1 プロセスとスレッド

| PID | cmdline | 主な thread（comm） |
|---:|---|---|
| 279 | `/usr/bin/abs` | `abs.T_UIM` (TID 476), `T_FPM`, `T_COM`, `iComTask`, `eTcpIp*`, `T_SDIAGM`, `T_SDIAGE` |
| 263 | `/usr/bin/adsb` | `abs.T_ADSB_COMM` (TID 286), `abs.T_ADSB_TUM` (TID 287) |
| 258 | `/usr/bin/nimd` | LAN/Wi-Fi 設定 |
| 260 | `/usr/bin/rrc` | ネット予約 (RRSR) HTTP クライアント |
| 771 | `/root/dixim/dlnactl HVT-4KBC` | DLNA サーバ管理 |
| 815 | `/opt/t4s/bin/mirakc` | 40772 |

`ps` は busybox 版で引数なしだと自プロセスしか出さないため、
`/proc/[0-9]*/cmdline` と `/proc/<pid>/task/*/comm` を直接読んだ [O]。

### 1.2 listen ポート

```
tcp 0.0.0.0:40772  LISTEN 815/mirakc
tcp 0.0.0.0:8001   LISTEN 263/adsb
tcp :::2323        LISTEN 708/busybox (telnetd)
```

- **8001 は adsb**。`/proc/263/maps` に
  `/opt/tourbillon4/lib/libtpl_adsb_ext.so` があり、この .so に
  `pnWebServer_Create`, `pnWebServer_GetPortNumber`, `TourbillonServer/1.0`,
  `mmt_start_server`, `tpl_mmt_start_socketif` がある [O][S]。
- 8001 へ何も送らない/`\r\n`/`GET`/`HELP` のどれも 3 秒以内に応答なし。
  接続はサーバ側で accept されたまま `CLOSE_WAIT`（Recv-Q 1〜7）になり、
  未知データを読まない挙動 [O]。HTTP 風だが制御 API ではない [有力]。
- `dlnactl` の HTTP 先は 8001 ではなく `127.0.0.1:53207`。
  `0xB286` で `ldrh r3,[0x10AEA]` により port を読み、値は `0xCFD7` [D]。
  53207 は現在 listen していない [O]。

### 1.3 SysV メッセージキュー（`/proc/sysvipc/msg`）

`/tmp/bdh_out_msgh.lst` は `<qid>-<msqid>` の対応表 [O]。

| qid | 名前 | msqid | 最後の送信/受信 PID | 状態 |
|---:|---|---:|---|---|
| 127 (`0x7F`) | **Q_UIM** | 98307 | lspid 279 / lrpid 476 (`abs.T_UIM`) | 稼働 |
| 123 (`0x7B`) | **Q_FPM** | 10748232 | lspid 279 / lrpid 279 (`T_FPM` 系) | 稼働 |
| 879 (`0x36F`) | Q_ADSB_COMM | 1605681 | lspid 279 / lrpid 286 (`abs.T_ADSB_COMM`) | 稼働 |
| 881 (`0x371`) | Q_ADSB_TUM | 884763 | lspid 263 / lrpid 287 (`abs.T_ADSB_TUM`) | 稼働 |
| 797 (`0x31D`) | **Q_IPC_MAN** | 15401430 | lspid 0 / lrpid 0 | **未使用** |
| 455 (`0x1C7`) | tuner FEM | 557073 | — | 稼働 |

- Q_UIM=0x7F の根拠: abs `0x16B940` で
  `eSysInitMessageQueue(0x7F, "Q_UIM", 0x100)` [D]。
- Q_FPM=0x7B の根拠: abs `0x1693A4` で
  `eSysInitMessageQueue(0x7B, "Q_FPM", ...)` [D]。
- Q_IPC_MAN=0x31D の根拠: abs `0x16B96E` で
  `eSysInitMessageQueue(0x31D, "Q_IPC_MAN", 0x20)` [D]。
  `/proc/sysvipc/msg` の `lspid=lrpid=0` は「作成後、誰も receive/send していない」[O]。

### 1.4 abs の `bAdsbInitialized`

- abs VA `0x02027184`（Phase 2b の `0x2027184` と同一）。
- `/proc/279/pagemap` の該当ページ（index `0x2027`）は present、PFN から
  物理 `0x6B791184` を求め、`devmem 0x6B791184 32` で **`0x00000001`** [O]。
- したがって「adsb COMM/TUM の初期化ゲート」は通過済みで、
  製品 UI の選局関数は defer せず即時実行に進む状態 [O][INF]。
- adsb 側の `.bss`（`0x395DD4`, `0x395F60`）は `/proc/263/pagemap` が
  PFN を返さず（`0x0600...`）、read-only では直接読めなかった [O][未]。

### 1.5 Unix socket

- `/tmp/.IsLanAliveReq`: SOCK_DGRAM, inode 1260。**nimd (PID 258) の fd 8** [O]。
  LAN alive 確認用で、UI キー経路ではない。
- `@/tmp/nimconf.sock`: abstract SOCK_STREAM, inode 1258。**nimd の fd 9** [O]。
  Wi-Fi/LAN 設定用。多数の接続が滞留しているが UI 制御ではない。
- `@jp.co.smedio.tourbillon4.mmt_parser`: adsb の MMT パーサ用 [O]。
- `/tmp/dlnactl/dmsctl`, `/tmp/dlnactl/diximcds` は現在 0 バイト [O]。

---

## 2. 製品 UI のキー経路（確定）

### 2.1 FPM → UIM（押下イベント）

- FPM の本体 `vFpmMain`（abs:`0x520A5D`、サイズ 24436）は
  `msgFpdReportFpmKeycode` を受け、キーコード構造体を `[r7+0x108]` に持つ [D]。
  文字列 `FPM@%10d: got msgFpdReportFpmKeycode with %8X` の参照は abs `0x521030` [D][S]。
- キー共通処理 `0x512C54` は key word を decode し、押下時に
  **`eSysSendMessage(0x7F, 0x2044, bodyPtr, 8, timeout=1000)`** を実行する [D]
  （abs `0x51365C`〜`0x513670`）。
- body は 8 バイト:
  ```
  +0x00 u32 keyCode   // FPM/ベンダ固有コード
  +0x04 u32 type      // 0=押下, 1=抑止側, 2=離す側（1/2 は推定）
  ```
  根拠: `0x5135FE` が `[body+4]` を 0/1 で分岐し、`0x715ED4` が `2` のとき
  keyCode を反転して渡す [D]。

### 2.2 UIM の受信とディスパッチ

- UIM 本体 `vUimMain`（abs:`0x707590`）は
  メインループ `0x70795A` で `eSysExReceiveMessage(0x7F, ...)` を呼ぶ [D]。
- msgId は `0x707A2C` 以降の二分探索で分岐する [D]。
  `0x2044` のケースは `0x7151D6` → handler **`0x715DD4`** [D]。
- `0x715DD4` は body の keyCode を読み、次の変換を行う [D]:

| 入力 keyCode | 内部コード | 備考 |
|---:|---:|---|
| `0x2F` | `0x32` | |
| `0x30` | `0x50` | |
| `0x31` | `0x51` | |
| `0x32` | `0x52` | |
| `0x11` | `0x54` | |
| `0x14` | `0x15` | |
| `0x0E` | `0x53` | |
| `0xB9` | 別分岐 | 値は未確定（`0x715E72`） |
| `0x125` | 別分岐 | 値は未確定（`0x715E66`） |

- 変換後 `0x743B34`（UIM キーフィルタ）を呼び、Bhv のキー処理へ渡す [D]。
  この内部コードが UI の「決定/上下左右/チャンネル」に対応する [有力] が、
  数値と機能の対応は未確定 [未]。

### 2.3 外部からのキー注入（候補）

Q_UIM はプロセスをまたぐ SysV メッセージキューなので、
**libsysapi の `eSysSendMessage` を呼べるプロセスなら**、製品プロセスを止めずに
キーイベントを送れる [INF]。

```
qid      = 0x7F (Q_UIM; msqid は /tmp/bdh_out_msgh.lst の 127-<msqid>)
msgId    = 0x2044
body     = { u32 keyCode; u32 type; }   // type=0
len      = 8
timeout  = -1
```

- `eSysSendMessage` の ABI は `(qid, msgId, bodyPtr, len, timeout)`。
  製品コード `0x513670` と `0xc699ca`（`0x36F,0x6040`）で同じ形 [D]。
- msqid を直接使う API は無く、qid を渡すと libsysapi が
  `/tmp/bdh_out_msgh.lst` の対応で解決する [O][INF]。
- **未確定**: keyCode の実値。特に CH UP/DOWN、数字、決定、4K ボタン。
  `RC_CH_UP`, `RC_CH_DOWN`, `RC_4K` 等の enum 名は abs にある [S] が、
  FPM の生コードとの対応表は静的表として取れていない。
- **未確定**: type の 1 の意味（リピート/長押し/離上）。押下だけなら 0 で足りる [有力]。

### 2.4 debug コマンド（参考・遠隔不可）

abs は起動時に `eDbgSetLouncher(name, handler, desc)` で多数の debug コマンドを登録する [D]:
- `UimKeyEmulation`（no parameters）
- `SendKeyForUIH`（`<keys>`）handler `0x6F9C34`
  - 文字列を token 化し、実行時構築のキー表（`.bss`）を線形検索して
    Q_UIM へ `eSysExSendMessage` する [D]。
  - キー表は静的ファイルに無く、起動時初期化のみ [D][未]。
- しかし debug コンソール用の listen ポートは無い（2323 は telnetd、8001 は
  Tourbillon Web）[O]。debug コマンドはシリアル/保守経路と推定 [INF]。
  したがって **ネットワークのみでは debug コマンドを起動できない** [有力]。

---

## 3. IP Control（`Q_IPC_MAN`）経路

### 3.1 サーバ起動・停止

- abs の IP Control モジュールは `[IpControl] Linux -> IP Control Server Start` を出し、
  `eSysSendMessage(0x31D, 0x1203, ...)` で `Q_IPC_MAN` に起動を要求する [D]
  （abs `0xA904C8` 付近）。失敗時は 1 秒間隔で再試行する [S]。
- 停止は `0x1201`、状態要求は `0x1202` と読める [D]。
- `Q_IPC_MAN` の実 msqid 15401430 は **一度も受信されていない** [O]。
  つまり IP Control サーバタスクは起動していないか、機能が無効。

### 3.2 キー受信部

- 受信関数 `0xA8FA34` は引数の key word を decode し、
  `[IpControl] Linux -> Host Key Code=0x%03X  Sub Code = 0x%x` を出す [D][S]。
- キー種別に応じて `Q_FPM`（qid `0x7B`）へ次を送る [D]:
  - `0x11E59`, len 2（`0xA8FB10`, `0xA8FB3C`）
  - `0x11E69`, len 4（`0xA8FB74`）
- FPM はこれを受けて通常の FPM→UIM 経路（`0x2044`）に流す [INF]。
  したがって IP Control が動いていれば、ネットワークから FPM 経由で
  UI キーを注入できる [有力]。

### 3.3 現状の制約

- サーバ起動メッセージ `0x1203` を外部から送っても、`Q_IPC_MAN` の受信者が
  居ないため何も起きない [O]。
- IP Control の listen ポート/プロトコルは未確定 [未]。
  文字列は `Linux -> Host`（Linux 側アプリ → Host 側）で、
  一般の AQUOS IP Control (`POWR`, `VOLM` 等) とは別物の可能性が高い [INF]。
- `Q_IPC_MAN` を外部から起動するには、まず受信タスクを起動する必要があり、
  それは製品設定/保守経路の領域 [未]。

---

## 4. DLNA / Dixim / 8001 の評価

### 4.1 dlnactl

- 53 KB、ELF ARM、stripped。import に `eSysReceiveMessage` は無く、
  `eSysInitMessageQueue` と `eSysSendMessage` のみ [D]。
- 起動時に次を登録 [D]:
  - qid `0x306` = `QID_LINUX_DLNACTL`
  - qid `0x149` = `Q_DLNASNAVI`
  - qid `0x7F` = `Q_UIM`（abs と同一のキューに attach）
- メインループは `/tmp/dlnactl/dmsctl` を `fscanf` で読み、
  `param1/param2` のコマンドとして実行する [D][S]。
  `Command receive. param1=%d, param2=%d` [S]。
- コマンドはサーバ管理:
  `DlnaAppMsgDiximStartServer`, `RestartServer`, `RegistTitle`, `DeleteTitle`,
  `PowerOff`, `UsbMount`, `UsbUnmount` [S]。
- DLNA サーバへの HTTP は `127.0.0.1:53207` の
  `/dms/control/ContentDirectory` で、SOAP は Browse/Search/CreateObject 等 [D][S]。
- **TV 自身の AVTransport 制御、チャンネル選択、キー入力は無い** [S][D]。
- DMS が起動していないため、現状この経路自体が動かない [O]。

### 4.2 TCP 8001

- adsb の `libtpl_adsb_ext.so` は Tourbillon の Web サーバ機能を持つ [O][D]。
  文字列: `TourbillonServer/1.0`, `pnWebServer_*`, `mmt_start_server`,
  `tpl_mmt_start_socketif` [S]。
- MMT/ADSB コンテンツ配信（ブラウザ向け）と推定 [INF]。
  未知パスに応答しないのは、仮想ディレクトリ未登録のため [有力]。
- 制御 API の文字列（SetChannel, Key, Remote 等）は無い [S]。

### 4.3 その他

- `/tmp/.IsLanAliveReq` と `@/tmp/nimconf.sock` は nimd のもの [O]。
  UI/選局の制御には使えない。
- `rrc` は RRSR（ネット予約）で、HTTP + SHA1 + gzip の予約 API [S][D]。
  キー/選局ではない。

---

## 5. `BhvTvAdsbTuneChangeChannel` の前提（abs 側 call site から確定）

### 5.1 シグネチャと defer

REA/Ghidra の decompile（`.tmp/rea-snap-abs`）と `llvm-objdump` から [D]:

```c
undefined4 BhvTvAdsbTuneChangeChannel(u32 path, u32 mode, void *t92)
{
  if (bAdsbInitialized == 0) {
      eGlobalPathName = path;
      eGlobalTuneType = mode;
      memcpy(tGlobaltTuningInfo, t92, 0x5c);
      eStmSetTimerAfter(0x7F, 0x12096, 0, 0, -1, 100);  // 100 ms
      return 1;                                         // defer
  }
  ...
}
```

- 再試行関数 `0xC669F8` は、`bAdsbInitialized` がまだ 0 なら
  同じ timer `0x12096` を 100 ms 後に再設定し、
  1 なら保存済み `eGlobalPathName` / `eGlobalTuneType` / `tGlobaltTuningInfo` を
  引数に `0xC66A84` を呼ぶ [D]。
- UIM 側の timer 受信は msgId `0x2096`（`0x12096` の下位）で
  handler `0xC669F8` に到達する [D]（abs `0x70DF44` → `0x713B8A`）。
- したがって **初期化前に呼んでも失われない**。製品 UI と同じ挙動になる。
- 実機は現在 `bAdsbInitialized=1` なので、次の呼び出しは即時実行 [O]。

### 5.2 mode の意味

`0xC66A84` の分岐 [D]:

| mode | 処理 |
|---:|---|
| `< 6` | `g4KPathName = path` |
| `< 7` かつ `(1<<mode) & 0x4F`（0,1,2,3,6） | UI 前処理（`FUN_c5a0e0`, `FUN_75f3c8`, `FUN_75f440`, `bVodStart`） |
| `== 6` | **4K 経路**。`FUN_500298(1,&fmt)` で video format `0x200/0x400/0x800` を読み、`9/10/11` を得る |
| それ以外 | `FUN_c6de7c(path, t92+0xc, t92+0x18)` で Dev を解決 |

- 92 バイト構造体の使用フィールド [D]:
  - `+0x00 u32 Dev`
  - `+0x04 u32 Tune`
  - `+0x08 u8 OID`
  - `+0x09 u8 BID`（`FUN_4bd978` の結果で上書き）
  - `+0x0A u16 NID`
  - `+0x0C u16 SID`
  - `+0x0E u16 TSID`
  - `+0x10 u32 RF`
  - `+0x14 u32 NotDecode`
  - `+0x18`〜`+0x5B` はそのまま `memcpy` 保存
- 4K で実際にログ/送信に使うのは
  `+0x14 NID`, `+0x18 SID`, `+0x1C FREQ`, `+0x20 TSID` [D]。
  これは `0x6004` body とは別で、`0xC66A84` 内の `local_38` 構造体に詰め替えられる。

### 5.3 0x6004 送信

`0xC66A84` の末尾で `FUN_457858(0, &keyinfo)` を呼ぶ [D]。
`0x457858` は 0x28 バイト body を組み、`eSysSendMessage(0x36F, 0x6004, body, 0x28)` する [D]:

| body offset | 値 |
|---:|---|
| `+0x00` | devIdx = 0（`local_20`） |
| `+0x04` | OID = keyinfo+8（u8） |
| `+0x08` | Dev = keyinfo+0（u32） |
| `+0x0C` | Tune = keyinfo+4（u32, = mode） |
| `+0x10` | NID = keyinfo+0xA（u16） |
| `+0x14` | SID = keyinfo+0xC（u16） |
| `+0x18` | RF = keyinfo+0x10（u32） |
| `+0x1C` | TSID = keyinfo+0xE（u16） |
| `+0x20` | NotDecode = keyinfo+0x14（u32） |
| `+0x24` | BID = keyinfo+9（u8） |

- これは Phase 2b §2.3 の「製品 10 word」と一致 [D]。
- **重要**: `0x457858` は per-device 24 バイト block を更新しない [D]。
  TUM `0x5004` は block の `+0x14`（Tune）で分岐するため、
  block が古い/未設定だと `0x6004` を送っても選局しない [D]（Phase 2b §2.5）。

### 5.4 4K wrapper `0xC66D50`

```c
int FUN_00c66d50(u32 dev)  // 4K 用
{
    u8 t92[0x5c];
    FUN_540e48(dev, t92);   // 保存済み構造体をコピー
    return BhvTvAdsbTuneChangeChannel(dev, 6, t92);
}
```

- `FUN_540e48` は `(dev-1)` を 0..5 に clamp し、semaphore `0xBE` の下で
  大域テーブルの `0x5C * idx` を memcpy する [D]。
- 呼び出し元は abs `0xC670B6` / `0xC670C2`。状態 `[r7+0x84]==3` の分岐で呼ばれる [D]。
- したがって **4K の製品 UI 経路は `0xC66D50(dev)`** で、
  dev に対応する保存済み 92 バイト構造体が既に正しいことが前提 [D]。

### 5.5 初期化ハンドシェイクと `0x6005`

- abs の Bhv ディスパッチ `0xC36B2C` は
  `[BhvTvProcessProc]msgAdsbCommReportInitialized` を受けると
  `0xC669B4` を呼ぶ [D][S]。
- `0xC669B4` は **`bAdsbInitialized = 1`** を書き、
  `FUN_52d628` の結果から `0x4578fc(0, a, b)` を呼ぶ [D]。
- `0x4578FC` は `0x6005` を 12 バイト body で送る [D]:
  ```
  +0x00 u32 0
  +0x04 u8  a
  +0x08 u8  b          // 全体 0x0C バイト、+0x05..+0x07 は未使用
  ```
  実体は `0xA6F19E`（`r1=(u8)mode, r2=(u8)(index-3)`）と
  `0xC669E0`（初期化時）の 2 か所から呼ばれる [D]。
- これは Phase 2b の「0x6005 は 12 バイト、0x5003 が 24 バイト block を書く」と整合 [D]。
  ただし `a/b` は `0x52d628` が返す設定値で、**Tune そのものではない** [D][未]。
- 結論: 実機で `bAdsbInitialized=1` なのは、このハンドシェイクが完了し
  `0x6005` が既に送られた後 [O][INF]。外部から `0x6004` だけを送っても
  block の Tune は更新されない理由がここにある [D]。

---

## 6. 4K 選局の最小手順

### 6.1 現時点で「確実」と言える順序

1. **状態確認（read-only）**
   - abs VA `0x02027184` の `bAdsbInitialized` を読む。
     現在は 1 [O]。0 なら製品 UI 経路でも defer される。
   - `Q_ADSB_COMM`/`Q_ADSB_TUM` の msqid を `/tmp/bdh_out_msgh.lst` で解決。
   - demod bank2 が `0x6080` のままなら未選局。

2. **製品 UI 経路（プロセス内）**
   - 既に保存済み 4K 設定がある: `0xC66D50(dev)`。
   - 自分で 92 バイト構造体を作る: `0xC66A84(dev, 6, &t92)`。
   - `bAdsbInitialized==0` でも 100 ms 後に自動再試行される。
   - ただし ptrace 不可・LD_PRELOAD で watchdog 再起動のため、
     外部からの「プロセス内呼び出し」は現実的でない [O]。

3. **IPC 経路（外部プロセス）**
   - Phase 2b §6.1 の `0x6001` → wait → `0x6002` → `0x6004` は
     adsb COMM/TUM のゲートを開けるが、`0x6005` の block 更新が
     製品初期化時にしか起きないため、Tune が古いまま [D]。
   - `bAdsbInitialized=1` の現在、`0x6004` 単独でも block が一致していれば
     `SetTSPort` まで到達し得る [有力]。block の一致確認が先。

4. **キー注入経路（外部プロセス）**
   - `eSysSendMessage(0x7F, 0x2044, &{keyCode,0}, 8, -1)` で UI にキーを送る。
   - UI 側で「4K チャンネル」を選ぶ操作（数字/CH UP/4K ボタン）を再現する。
   - これが物理リモコンの代替として最も自然 [有力]。
   - ただし keyCode 表が未確定 [未]。

### 6.2 4K チャンネルデータ

- `ch.json` の BS4K チャンネル番号（`45168`〜`45330`）は
  チャンネル識別子で、TSID そのものではない [S][未]。
- `tsid-bs.json` には 2K BS の TSID はあるが、BS4K の TSID 行は無い [S]。
- 4K の `NID/SID/TSID/RF` は Phase 1/2 の demod/TSID 資料と
  実機の `tsid` 測定を突き合わせる必要がある [未]。
- `0x6004` body の Dev/Tune/NID/SID/RF/TSID は
  TUM の block が正しければログ以上の意味を持たない [D] ため、
  block をどう設定するかが本質 [未]。

### 6.3 フローチャート

```
4K を選びたい
├─ 製品 UI を操作できるか
│   ├─ Yes: Q_UIM 0x2044 でキー注入 → UI の 4K 選択
│   │       (keyCode 表が未確定。CH UP/数字/4K の順で試す)
│   └─ No: 0x6004 を送る
│           ├─ bAdsbInitialized=0 → 製品経路でも defer。0x6001/0x6002 で初期化
│           ├─ block の Tune が古い → 0x6005 だけでは Tune は入らない [D]
│           └─ block が一致 → SetTSPort まで到達。bank2 TSID を read-only 確認
└─ どうしても外部 IPC で完結しない
    └─ プロセス内 0xC66D50/0xC66A84 が必要（ptrace/LD_PRELOAD は watchdog リスク）
```

---

## 7. 復旧と watchdog リスク

| 操作 | リスク | 復旧 |
|---|---|---|
| Q_UIM `0x2044` キー注入 | 低。製品の通常キー経路。誤キーは無視/画面遷移のみ | 通常の戻る/電源キー、最悪は電源再投入 |
| `0x6004`/`0x6005` 送信 | 低。製品の通常メッセージ | 通常選局で上書き。復元操作不要 |
| Q_IPC_MAN `0x1203` 起動要求 | 中。受信者が居ないとキューに残るだけだが、将来のタスクと衝突し得る | 送らない。送った場合は qnum を read-only 確認 |
| abs/adsb の `.bss` 直接書換 | 高。状態表を壊す | adsb 再起動 → 電源再投入 |
| ptrace / LD_PRELOAD / abs 再起動 | 高。watchdog 再起動を観測済み | 電源再投入。再起動後に `bAdsbInitialized` を再確認 |

- キー注入は「押下 1 回」だけにし、連打しない。`type=1/2` の意味が未確定なので
  最初は `type=0` のみ使う [未]。
- 電源キー/工場出荷系のキーは送らない。
- `0x6004` は製品自身も使うため、失敗しても復元不要 [D]。
- `Q_IPC_MAN` の起動要求は「受信者不在」を確認済みなので、現状は送らない [O]。

---

## 8. 未確定事項

1. **FPM keyCode と UI 機能の対応表**（CH UP/DOWN、数字、決定、4K、戻る）。
   `0x715DD4` の変換表は一部のみ。`SendKeyForUIH` の実行時表は `.bss` で静的取得不可。
2. **`type` 1/2 の正確な意味**（リピート、離上、長押し）。
3. **per-device 24 バイト block の Tune を外部から設定するメッセージ**。
   `0x6005` は 12 バイトで Tune ではない [D]。他の sender は abs 内に見つかっていない。
4. **adsb の state/op 表 `0x395F60` の現在値**。
   `/proc/263/pagemap` が PFN を返さず read-only で読めなかった [O]。
5. **IP Control サーバの有効化条件と listen ポート/プロトコル**。
   `Q_IPC_MAN` が未使用なので、設定メニュー/保守起動の可能性 [INF]。
6. **BS4K の TSID/NID/SID/RF の確定値**。
   `ch.json` の 45168〜45330 はチャンネル識別子で TSID ではない [未]。
7. **`eSysExSendMessage` の厳密な ABI**（debug `SendKeyForUIH` の解釈）。
8. **8001 の仮想ディレクトリ/パス**（TourbillonServer の URL 空間）。
9. **`Q_UIM` の msqid は起動ごとに変わる**。ハードコード禁止。

---

## 9. 次の read-only 試験（書込みなし）

1. **`bAdsbInitialized` の再確認**
   - `/proc/279/pagemap` の index `0x2027` から PFN を求め、
     `devmem <phys> 32` で読む。0 なら製品初期化が失われている。
2. **Q キューの状態確認**
   - `/proc/sysvipc/msg` で `Q_UIM`/`Q_FPM`/`Q_ADSB_COMM`/`Q_ADSB_TUM` の
     `lspid/lrpid/qnum` を読む。qnum が増えていれば誰かが送っている。
3. **8001 のプロトコル探索（短時間）**
   - `TourbillonServer/1.0` を User-Agent にして `HEAD /`, `GET /`,
     `/mmt`, `/adsb` を 1 秒 timeout で試す。状態変更なし。
4. **dlnactl の状態**
   - `/tmp/dlnactl/dmsctl`, `/tmp/dlnactl/diximcds`, `/tmp/.dlna_data` を read。
   - DMS の 53207 が listen しているか `netstat -lnt` で確認。
5. **nimd socket の peer**
   - `/proc/net/unix` の `@/tmp/nimconf.sock` の inode と、
     `/proc/258/fd` の対応を確認。UI 経路でないことの再確認。
6. **abs の大域変数**
   - `bAdsbInitialized` の他、`tGlobaltTuningInfo`（`0x6d7f874`）と
     `eGlobalTuneType`（`0x6d7f8d8`）を pagemap+devmem で read。
     直前の選局要求が残っているかを見る。
7. **（書込み解禁後）キー注入の段階試験**
   - まず `0x2044` で `keyCode=0x0E`（`0x53` へ変換される比較的無害なキー）を 1 回。
   - 画面/ログ/`/tmp` の変化を確認してから CH UP/DOWN を試す。
   - 電源キーは送らない。

---

## 10. 根拠アドレス一覧

### 10.1 abs

| 内容 | アドレス |
|---|---|
| `Q_UIM` init (0x7F) | `0x16B940` |
| `Q_FPM` init (0x7B) | `0x1693A4` |
| `Q_IPC_MAN` init (0x31D) | `0x16B96E` |
| `vUimMain` | `0x707590` |
| UIM receive (`eSysExReceiveMessage(0x7F)`) | `0x70795A` |
| UIM dispatch (msgId 二分探索) | `0x707A2C` |
| UIM case `0x2044` → handler | `0x7151D6` → `0x715DD4` |
| FPM keycode log ref | `0x521030` |
| FPM → UIM `0x2044` 送信 | `0x51365C`–`0x513670` |
| key decode `0x512C54` | `0x512C54` |
| `eUimKey` filter | `0x743B34` |
| IPControl key handler | `0xA8FA34` |
| IPControl key → FPM `0x11E59`/`0x11E69` | `0xA8FB10`, `0xA8FB3C`, `0xA8FB74` |
| IPControl server start `0x1203` | `0xA904C8` |
| IPControl server stop `0x1201` | `0xA9055C` |
| `eDbgSetLouncher` table | `0x7060xx` |
| `SendKeyForUIH` handler | `0x6F9C34` |
| `BhvTvAdsbTuneChangeChannel` | `0xC66A84` |
| defer/retry | `0xC669F8` |
| 4K wrapper | `0xC66D50` |
| 0x6004 body builder/sender | `0x457858` |
| 0x6005 sender | `0x4578FC` |
| `msgAdsbCommReportInitialized` handler | `0xC36B2C` |
| set `bAdsbInitialized=1` + 0x6005 | `0xC669B4` |
| `bAdsbInitialized` VA | `0x02027184` |

### 10.2 dlnactl

| 内容 | アドレス |
|---|---|
| `QID_LINUX_DLNACTL` init (0x306) | `0x929E` |
| `Q_DLNASNAVI` init (0x149) | `0x92B0` |
| `Q_UIM` init (0x7F) | `0x92C2` |
| HTTP client (Host/port) | `0xB23C`–`0xB28E` |
| DMS port 定数 `0xCFD7` (53207) | `0x10AEA` |
| dmsctl 監視ループ | `0x9454` 以降 |
| UIM への応答送信 | `0x9416`–`0x9440` |

### 10.3 文字列（abs, VA）

| 文字列 | VA |
|---|---|
| `Q_UIM` | `0xD00DB8` |
| `Q_FPM` | `0xD0059C` |
| `Q_IPC_MAN` | `0xD00DBE` |
| `[IpControl] Linux -> Host Key Code=0x%03X  Sub Code = 0x%x` | `0xF318E4` |
| `[IpControl] Linux -> IP Control Server Start` | `0xF31A08` |
| `[UIM] msgUimRequestUimKeyInput` | `0xEBF44C` |
| `UimKeyEmulation` | `0xE53C08` |
| `SendKeyForUIH` | `0xE53C2A` |
| `[BhvTvProcessProc]msgAdsbCommReportInitialized` | `0xF89C00` |
| `[msgRequestUimAdsbTuneChangeChannel]` | `0xF901E0` |
| `BhvTvAdsbTuneChangeChannel OID(...)` | `0xF90264` |
| `TourbillonServer/1.0`（libtpl_adsb_ext.so） | `0x149E4` |

---

## 11. まとめ（実務向け）

- **物理リモコンの代替として最も見込みがあるのは Q_UIM への `0x2044` キー注入**。
  FPM と同じメッセージを送るだけで、製品 UI のキー処理に入る。
  必要なのは「キーコード表」と「送信できるプロセス」の 2 点。
- **IP Control は現状停止**。`Q_IPC_MAN` が未使用なので、ネットワークからの
  キー入力は今は使えない。起動条件は未確定。
- **DLNA/8001 は選局に無関係**。dlnactl は DMS 管理、8001 は adsb の
  Tourbillon Web。AVTransport も無い。
- **4K の製品経路は `0xC66D50`/`0xC66A84`**。`bAdsbInitialized=1` なら
  初期化待ちは無い。外部 IPC の `0x6004` は block の Tune 更新が無いため、
  そのままでは選局しない。
- **復旧は通常選局/電源再投入で足りる**。abs/adsb のメモリ書換と
  ptrace/LD_PRELOAD は watchdog リスクがあるので避ける。
