# リモコン Web UI (`t4s-remote`)

SHARP 4S-C00AS1 の製品 UI に、LAN 経由でリモコンキーを送る小さな HTTP サーバです。
リモコン受光部が壊れているため、製品の通常キー経路である **Q_UIM キュー**
(qid `0x7F`、メッセージ **`0x12044`**、body `{u32 keyCode; u32 type}`) に直接
メッセージを書き込みます。送信には `t4s-core` の `tuner::send_raw_message` を使います。

> **msgId の訂正**: 以前の実装は `0x2044` を送っていましたが、abs の受信ディスパッチは
> `0x00012044` と比較するため `0x2044` は**一致せず黙って破棄**されます（prev key も
> 更新されない）。現在は `0x12044` を使用します。別系統の有効 msgId は `0x1115C`
> （`0x115C` ではない）です。詳細は `docs/analysis/phase4-device-blue.md` §2.1。

- crate: `rust/t4s-remote/`（`src/main.rs`, `src/keys.rs`, `src/json.rs`, `src/index.html`）
- バイナリ: `t4s-remote`（static `armv7-unknown-linux-musleabihf`、外部 crate 依存なし）
- 既定の待受: **`0.0.0.0:8484`**
- 認証: **なし**（LAN 専用。§7 参照）
- 書き込み先は `/tmp` のみ（`/tmp/t4s-remote.log`, `/tmp/t4s-remote.pid`）

---

## 1. ビルドと配置

```sh
cd rust
cargo build --release --target armv7-unknown-linux-musleabihf \
      --manifest-path t4s-remote/Cargo.toml \
      --target-dir target/remote
cd ..
cp rust/target/remote/armv7-unknown-linux-musleabihf/release/t4s-remote .tmp/http/t4s-remote
```

`t4s-remote` は独自の `[workspace]` を持つので、`rust/Cargo.toml` の `members`
には追加していません（他作業と衝突しないため）。`t4s-core` へは path 依存です。

`scripts/deploy.sh` は `mirakc` / `t4s-tuner` / `t4s-cas` と一緒に
`t4s-remote` も `.tmp/http` から実機 `/mnt/t4s/opt/t4s/bin/` へ転送します
（ビルド済みの `rust/target/remote/.../t4s-remote` があればそれを優先）。

`.tmp/http` は `make serve`（`python3 -m http.server 8080`）で配信されています。
実機への配置は telnet 越しに wget します（`.tmp/dev.sh` は 1 コマンド実行用）:

```sh
sh .tmp/dev.sh 'wget -q http://192.168.100.100:8080/t4s-remote \
  -O /mnt/t4s/opt/t4s/bin/t4s-remote && \
  chmod 755 /mnt/t4s/opt/t4s/bin/t4s-remote'
```

実行中のバイナリを上書きすると `Text file busy` になります。先に停止してください。

---

## 2. 起動と停止

```sh
# 起動（推奨: 自分で daemon 化し、PID を /tmp/t4s-remote.pid に書く）
# 冪等: pid が生存していれば "already running" で何もしない。
# stale PID ファイルは削除して起動し直す。
sh scripts/ondevice-remote.sh start

# 状態（pid / ps / LISTEN / ログ末尾）
sh scripts/ondevice-remote.sh status

# 停止
sh scripts/ondevice-remote.sh stop
```

電源再投入後は `scripts/ondevice-up.sh` が mirakc と一緒に自動起動します
（§10）。`ondevice-remote.sh start` も同じ冪等チェックを持ちます。

実機上で直接:

```sh
/mnt/t4s/opt/t4s/bin/t4s-remote --daemon --port 8484
kill $(cat /tmp/t4s-remote.pid)
```

`scripts/ondevice-remote.sh` はホスト側から telnet (2323) 経由で起動・停止します。
`--daemon` は fork → `setsid()` → stdout/stderr を `/tmp/t4s-remote.log` に付け替え、
というごく普通の daemon 化です。telnet を切っても残ります。

> busybox の `setsid` applet でも同様に起動できます
> （`setsid /mnt/t4s/opt/t4s/bin/t4s-remote --port 8484 > /tmp/t4s-remote.log 2>&1 &`）。
> ただし PID が `/tmp/t4s-remote.pid` に残らず、`ps` (busybox) は同一セッションの
> プロセスしか表示しないため、停止・確認が面倒になります。スクリプトは `--daemon` を使います。

停止は `kill`（SIGTERM）で十分です。サーバは状態を持たないので、再起動しても
問題ありません（primer は次のキー送信時に自動で入ります）。

---

## 3. URL と API

| メソッド | パス | 内容 |
|---|---|---|
| GET | `/` | リモコン UI（ダーク、スマホ対応、キーボードショートカット、ログ） |
| GET | `/api/keys` | 全キー `0x000–0x13D` の `{"code","name"}` 配列（318 件） |
| GET | `/api/status` | uptime / 製品スレッド / SI / I2C regs / Q_UIM / 最終キー |
| POST | `/api/key` | 1 キー送信 |
| POST | `/api/macro` | 複数キーを順に送信 |

ブラウザ: `http://192.168.1.154:8484/`

### 3.1 `POST /api/key`

```json
{ "code": 37, "name": "RC_BACK", "type": 0, "release": true }
```

- `code` または `name`（`RC_` 省略可、大文字小文字無視）。範囲は `0x000–0x13D`。
- `type`: `0` = 押下（既定）、`2` = 離上のみ（押下なし）。それ以外は 400。
- `release`: 既定 `true`。`true` なら `type 0` → **200 ms** → `type 2`。
  hold 時間は `--hold-ms`（20–1000 ms）で変更できます。80 ms では製品が
  押下を認識しないことがあったため、既定を 200 ms にしています。
  `false` なら `type 0` のみ（押下保持）。
- 応答例:

```json
{"ok":true,"code":37,"name":"RC_BACK","type":0,"release":true,"primer":null}
```

`primer` は今回挿入した primer のキーコード（無ければ `null`）。

### 3.2 `POST /api/macro`

```json
{ "steps": [
    {"code": 315, "delay_ms": 500},
    {"code": 1,   "delay_ms": 350},
    {"name": "RC_ENTER"}
] }
```

サーバ側で順に送信し、各 step の後に `delay_ms` 待ちます。
制限: 64 step、1 step の `delay_ms` ≤ 5000、合計 ≤ 30000 ms。
応答: `{"ok":true,"steps":3,"elapsed_ms":1400}`。同期実行なので、UI は完了後にログを出します。

### 3.3 `GET /api/status`

```json
{
  "now": 1794572040, "time": "21:14:00 JST",
  "uptime_s": 2811.70,
  "threads": {"MIXER": false, "Sink": false, "buf_buf": false, "HevcFactory": true},
  "si": {"ok": false, "reason": "no usable /tmp/adsb MH-SDT + MPT"},
  "regs": {"ok": false, "reason": "I2C read did not finish in 2 s"},
  "q_uim": {"ok": true, "qid": "0x7f", "msqid": 98307, "qnum": 0, "lspid": 279, "lrpid": 476},
  "last_key": {"code": 21, "name": "RC_ENTER", "type": 0, "age_s": 189, "at": "21:10:53 JST"}
}
```

- `uptime_s`: `/proc/uptime`。
- `threads`: 全プロセスの thread `comm` に `MIXER` / `Sink` / `buf_buf` / `HevcFactory`
  が現れるか（製品側は `abs.MIXER_*` のような名前）。
- `si`: `t4s_core::si::live_service("/tmp/adsb")`。MH-SDT と MPT が食い違う遷移中は `ok:false`。
- `regs`: `t4s_core::gr_i2c::read_output_regs()` の `lock` と `bank2_selection`。
  I2C0 セマフォは製品チューナや他の診断が数秒保持することがあるため、
  **ワーカースレッドで非同期に読み、200 ms 待っても終わらなければ
  `ok:false`（または直近のキャッシュ）を返します**。成功値は 30 秒キャッシュし、
  その間は再読込しません。HTTP 処理は接続ごとのスレッドなので、
  status の I2C 待ちがキー POST をブロックすることはありません。
- `q_uim`: `/tmp/bdh_out_msgh.lst` の `127-<msqid>` と `/proc/sysvipc/msg` の
  `qnum` / `lspid` / `lrpid`。`qnum=0` は UIM が消費済み。
- `last_key`: このサーバプロセスが最後に送ったキー。

---

## 4. キー表

全 318 件は `GET /api/keys` と `rust/t4s-remote/src/keys.rs`（`docs/analysis/phase4-ui.md`
§2.4 から機械的に転記）にあります。UI に出している主なキー:

| 名前 | code | 用途 |
|---|---:|---|
| `RC_POWER` | `0x000` | 電源（**初期設定に入る可能性あり**、§6） |
| `RC_No1`–`RC_No9` | `0x001`–`0x009` | 数字 1–9 |
| `RC_No0` | `0x00A` | 数字 0 |
| `RC_CH_UP` / `RC_CH_DOWN` | `0x00C` / `0x00D` | CH |
| `RC_MENU` | `0x010` | MENU |
| `RC_OSD_OFF` / `RC_FLASHBACK` | `0x011` / `0x012` | 画面表示消去 / 前の番組 |
| `RC_INFO` | `0x013` | 情報 |
| `RC_SELECT` | `0x014` | 選局 |
| `RC_ENTER` | `0x015` | 決定（**更新通知が出ているときは押さない**） |
| `RC_END` | `0x064` | 終了（「更新ソフトウェアがあります」通知を消す） |
| `RC_UP`–`RC_RIGHT` | `0x016`–`0x019` | 十字 |
| `RC_PAGE_UP` / `RC_PAGE_DOWN` | `0x01A` / `0x01B` | ページ |
| `RC_BACK` | `0x025` | 戻る |
| `RC_COLOR_BUTTON1`–`4` | `0x028`–`0x02B` | 色キー（並び順は未確定） |
| `RC_WIDE` | `0x02D` | ワイド |
| `RC_EPG` | `0x061` | EPG |
| `RC_BSD` | `0x08D` | BS |
| `RC_CS` | `0x0B6` | CS |
| `RC_CS1` / `RC_CS2` | `0x08E` / `0x08F` | CS1 / CS2 |
| `RC_SETUP_MENU` | `0x11F` | 設定メニュー |
| `RC_VOLUME_UP` / `RC_VOLUME_DOWN` | `0x129` / `0x12A` | 音量 |
| `RC_4K` | `0x13B` | 4K |

**`RC_POW_OFF` (`0x021`) は UI にも API にも置いていません。** 送らないでください。

**`RC_ENTER` (`0x015`) は更新通知が出ているとき危険です。** 通知の「確認」に
フォーカスがある状態で押すとファームウェア更新・再起動につながります。通知は
**`RC_END` (`0x064`) の「終了」**で消してください。通知がモーダル表示中の間は
INFO / EPG も吸われます。UI の「決定」ボタンは確認ダイアログを挟みます。

UI のキーボードショートカット: 矢印 → 十字、Enter → 決定、Backspace/Esc → 戻る、
数字 → 数字、`c` → CH↑、`n` → CH↓、`+`/`-` → 音量、`m` → MENU、`b` → 戻る、`i` → 情報。

---

## 5. primer（必須）

UIM のキー受付は「直前キーと同じコード」をリピートとみなして捨てます
（`docs/analysis/phase4-ui.md` §0.3）。起動直後の直前キーは `0` なので、
**何も送らずに最初の押下を送ると `key=0x000` のとき捨てられます**。対策:

- primer = `{0x000, type 2}`（power の離上だけ、押下なし）を 1 回送り、直前キーを 0 以外にする。
- サーバは **押下の直前に必要なときだけ** primer を自動で入れます:
  - まだ 1 度もキーを送っていない（直前キー = 0 の可能性）
  - 直前の押下が同じコード（リピートで捨てられる）
- 押下コードが `0x000` のときだけ primer を `{0x025, type 2}`（BACK の離上）にします。
  `{0x000, type 2}` だと直前キーが 0 のままになり、power 押下が捨てられるためです。

したがって **同じキーを連打しても primer が自動で挟まり、10 連打も通ります**
（`CH UP ×10` マクロはこれを利用）。

UI の「primer」ボタンは `{0x000, type 2}` を明示的に 1 回送ります。
UIM が再起動した場合（abs の再起動）は、次の押下時に自動で primer が入ります。

---

## 6. 起動直後の初期設定画面

電源投入後は製品の初期設定画面が出ることがあります。**`RC_POWER` (`0x000`) は
初期設定に入る（または電源を切る）可能性があるため、試験では送っていません。**

UI のマクロ:

| マクロ | 内容 |
|---|---|
| primer | `{0x000, type 2}` |
| 初期設定を抜ける | `RC_BACK` (`0x025`) を 600 ms 間隔で 3 回（キャンセル専用。決定はしない） |
| 4K 101 | `RC_4K` → `1` → `0` → `1` → `RC_ENTER`（各 350–500 ms） |
| CH UP ×10 | `RC_CH_UP` を 350 ms 間隔で 10 回（primer 自動挿入） |

初期設定を抜けるマクロは「戻る」だけを送ります。実際の画面で足りない場合は、
UI の十字キー・決定キーで手動操作してください（`0x016–0x019`, `0x015`）。

`4K 101` マクロは実装と疎通のみ確認し、**実機では押していません**（他作業が
4K 選局・ARC を試験中のため）。

---

## 7. セキュリティ

- **認証なし・CSRF 対策なし・TLS なし。** `0.0.0.0:8484` を信頼できる LAN 内に
  限定して公開してください。インターネットへ公開しないでください。
- 誰でも製品 UI のキーを押せる状態になります（`RC_POW_OFF` は送れませんが、
  `RC_POWER` は押せます）。
- 書き込みは `/tmp/t4s-remote.log` と `/tmp/t4s-remote.pid` のみ。製品ストレージや
  `/mnt/t4s` へは書きません。

---

## 8. 実機試験（2026-11-13 21:09–21:14 JST）

環境: host `192.168.100.100` → 実機 `192.168.1.154`（busybox telnet + HTTP）。
`/mnt/t4s/opt/t4s/bin/t4s-remote` を `--daemon --port 8484` で起動（pid 4626）。

```
$ curl -s -o /dev/null -w '%{http_code} %{size_download}\n' http://192.168.1.154:8484/
200 16856

$ curl -s http://192.168.1.154:8484/api/keys | python3 -c 'import json,sys;d=json.load(sys.stdin);print(len(d),d[0],d[-1])'
318 {'code': 0, 'name': 'RC_POWER'} {'code': 317, 'name': 'RC_CODE_TEST'}

$ curl -s http://192.168.1.154:8484/api/status        # 2.04 s
{"now":1794572040,"time":"21:14:00 JST","uptime_s":2811.70,
 "threads":{"MIXER":false,"Sink":false,"buf_buf":false,"HevcFactory":true},
 "si":{"ok":false,"reason":"no usable /tmp/adsb MH-SDT + MPT"},
 "regs":{"ok":false,"reason":"I2C read did not finish in 2 s"},
 "q_uim":{"ok":true,"qid":"0x7f","msqid":98307,"qnum":0,"lspid":279,"lrpid":476},
 "last_key":{"code":21,"name":"RC_ENTER","type":0,"age_s":189,"at":"21:10:53 JST"}}
```

送信試験（各 1 回。`RC_POWER` 押下は送っていません）:

```
POST /api/key {"code":0,"type":2}          -> 200 {"ok":true,...,"type":2,"primer":null}
POST /api/key {"name":"RC_BACK"}           -> 200 {"ok":true,"code":37,...,"primer":null}
POST /api/key {"code":21}                  -> 200 {"ok":true,"code":21,...,"primer":null}
```

実機ログ `/tmp/t4s-remote.log`:

```
[21:09:57 JST] key 0x000 RC_POWER type=2 (release only)
[21:09:57 JST] POST /api/key -> 200 (78 bytes, 0 ms)
[21:09:57 JST] key 0x025 RC_BACK type=0
[21:09:57 JST] POST /api/key -> 200 (77 bytes, 88 ms)
[21:10:53 JST] key 0x015 RC_ENTER type=0
[21:10:53 JST] POST /api/key -> 200 (78 bytes, 89 ms)
```

キュー確認（`grep 98307 /proc/sysvipc/msg`）:

```
0  98307  777  0  0  279  476  0 0 0 0  1794572010 1794572010 6
```

`qnum=0`, `lspid=279 (abs)`, `lrpid=476 (abs.T_UIM)` → 3 通とも UIM が受信・消費済み。
`/api/status` の `last_key` も `RC_ENTER 21:10:53` を示しています。

### 8.1 電源再投入後の復旧試験（2026-11-13 21:52–21:57 JST）

電源再投入後、8484 は LISTEN していませんでした（`/tmp/t4s-remote.pid` 無し、
`ps -A` に `t4s-remote` 無し）。`ondevice-remote.sh restart` で
`/mnt/t4s/opt/t4s/bin/t4s-remote`（md5 `dfb6b967…`）を pid 1333 で起動しました。

```
$ curl -s -o /dev/null -w '%{http_code} %{time_total}\n' http://192.168.1.154:8484/
200 0.007
$ curl -s http://192.168.1.154:8484/api/keys | …  # 318 件
200 0.004  (0.004s)
$ curl -s -w ' %{time_total}\n' http://192.168.1.154:8484/api/status
200 0.091 (2 回目), 0.171 (キャッシュ更新時)
$ POST /api/key {"name":"RC_BACK"}
{"ok":true,"code":37,"name":"RC_BACK","type":0,"release":true,"primer":0}
200 0.096
```

キュー（`/proc/sysvipc/msg`、直後 → 数秒後）:

```
0 98307 777 736 8 1333 471 …   # qnum=8, lspid=1333 (t4s-remote が送信)
0 98307 777   0 0  279 279 …   # 数秒後 qnum=0（UIM が消費）
```

`lspid=1333` は t4s-remote プロセス自身。`RC_BACK` は primer 0x000（再起動後の
初回のみ）→ `0x025` press → release の 3 通で、msgsnd は成功しています。
その直後 qnum が一時的に 8 まで増えたのは UIM が別作業で一時的に止まっていたためで、
数秒後に 0 へ戻りました（サーバ側の送信失敗ではありません）。

同じ時間帯、ホスト→実機の新規 TCP 接続が 2–3 回に 1 回 SYN タイムアウトしました
（`curl` の `time_connect=0`、`nc -z 8484` が 2 秒で失敗）。ただし **mirakc 40772 も
同じ確率で失敗**し、ping と telnet 2323 は通っていました。実機内
`wget http://127.0.0.1:8484/api/status` は常に成功（84 ms）。したがって 8484 の停止ではなく、
ホスト `192.168.100.100` → ゲートウェイ `192.168.100.3` → 実機の経路で新規 SYN が
落ちる負荷/ネットワーク起因です（再試行で 200）。サーバは接続ごとにスレッドを
立てるため、status が他リクエストをブロックしません。

### 8.2 msgId `0x12044` 修正と再試験（2026-11-13 22:12–22:35 JST）

`MSG_KEY` を `0x2044` → **`0x12044`** に変更し、md5 `e0c68d54…` のバイナリを
`/mnt/t4s/opt/t4s/bin/t4s-remote` に配置して再起動しました（pid 1917）。

- 連続キーの primer: `RC_CH_UP` を 2 回送ると 1 回目は `primer:null`、
  2 回目は `primer:0`。同じキーの直前で `{0x000,type2}` が挿入され、
  押下は `type0` → 80 ms → `type2` のままです。
- 22:15 の UI マクロ（`RC_4K`→1→0→1→`RC_ENTER`）に対して、製品が
  **「選局・再生に失敗しました。」** の OSD を表示しました。`0x2044` のときは
  製品が無反応だったため、`0x12044` が UIM ディスパッチに受理されている
  ことを確認できました。
- ただし SI は `141 / 45170` のままで、4K 選局自体が失敗しています。
  さらに 22:25 頃、OSD の「更新ソフトウェアがあります」に対して `ENTER` を
  送ったところ**製品が再起動**しました（電源キーは送っていません）。
- 再起動後、mirakc と t4s-remote は **pid 1028** で自動起動し、ログは
  `msg 0x12044` を出力しています（§10 の自動起動も実機で確認）。
  一方 abs は live view に戻らず（`threads` 全 false、`/tmp/adsb` に SI なし、
  OSD 黒）、**CH_UP で SI を 151 に変える確認はできませんでした**。
  live view 復帰には製品側の電源/選局操作が必要です。

### 8.3 WebUI のボタンがキーを送れない原因と修正（2026-11-14 00:12–00:22 JST）

- ホストから `/api/status` / `/api/keys` を各 5 回: 成功時は 3–80 ms、
  失敗時は `time_connect=0` で 5 s タイムアウト。**失敗したリクエストは
  `/tmp/t4s-remote.log` に記録されません**（TCP 接続が確立していない）。
  mirakc 40772 も同じ確率で失敗し、ping / telnet 2323 は通ります。
- 従来の `respond` は **`Connection: close`** を返していました。`GET /` の
  直後に TCP を閉じるため、次のボタン POST は毎回新しい SYN を要求し、
  間欠的な SYN 損失に当たるたび失敗します。ブラウザ側 fetch の 8 s abort は
  SYN 再送（1+2+4 s）の途中で切れることがありました。
- Playwright で実ページの「情報」ボタンを押した結果:
  - 修正前: `POST /api/key` が **1539 ms**（ページ接続は close 済みで新規 TCP）。
  - keep-alive 修正後: 同じクリックが **93–97 ms**、HTTP 200、
    `last_key=RC_INFO`、`primer 0x000`。`/api/status` も自動取得され画面表示。
- 修正内容:
  - 1 接続で複数リクエストを処理（`Connection: keep-alive` + read loop）。
  - UI の fetch timeout を 8 → 15 s（key）/ 12 s（status）に延長。
    ボタンは送信中に disable、statusline とログに成功/失敗理由を表示。
  - ビルド版 `2026-11-14a` を header / footer / `/api/status` の `build` に表示。
  - 画面表示時に `/api/status` を自動取得し、`last_key` / UIM / SI を表示。
  - `0x12044` の msgsnd が失敗した場合のみ `0x1115C` にフォールバック。
  - `ondevice-up.sh` で `somaxconn` / `tcp_max_syn_backlog` を 1024 に
    上げる（best-effort、失敗しても起動は継続）。
- **実機検証は電源再投入待ち**。復帰後に `/api/status` で MIXER/Sink=true,
  SI=141/45170 を確認し、キー送信は INFO 1 回だけ（type0 → 200 ms → type2、現在の既定）で行います。
  `t4s-probe uimkey` を直接使う場合も type0 単独ではなく type2 を対で送ってください。

### 8.4 終了（`RC_END 0x064`）と hold 200 ms（2026-11-14）

- 実機でユーザー操作により **EPG `0x61` が番組表を開き、INFO `0x13` が情報表示**に
  成功しました。Q_UIM `0x12044` と WebUI のキー経路は正常です。
- 画面に「更新ソフトウェアがあります」通知が定期的に再出現します。これは
  **終了 `RC_END 0x064`** で消せますが、従来の hold 80 ms では押下が
  認識されないことがありました。
- `PRESS_HOLD` の既定を **80 ms → 200 ms** に変更。`--hold-ms`（20–1000 ms）で
  変更でき、`/api/status` の `hold_ms` と起動ログに表示されます。
- **決定 `RC_ENTER 0x015` は更新通知が出ているときは押さないでください。**
  通知の「確認」にフォーカスがあるとファームウェア更新・再起動につながります。
  UI の「決定」ボタンと ENTER を含むマクロは確認ダイアログを挟み、
  パネルに警告文を表示します。通知は「終了（通知を消す）」で消してください。
  通知のモーダル表示中は INFO / EPG も吸われます。
- 本修正の実機確認は **再起動と `/api/status` のみ**で、キー送信テストは
  行っていません（`build=2026-11-14b`、`hold_ms=200` を確認）。

---

## 9. 既知の制約

1. **`regs` は I2C0 セマフォ待ちで `ok:false` になることがある。**
   読み取りはワーカースレッドで行い、**200 ms** で打ち切ってキャッシュまたは
   `ok:false` を返します。成功値は 30 秒キャッシュ。read は接続スレッドを
   ブロックしません。製品が idle なら `t4s-core` 側の 5 秒待ちで成功します。
2. **製品負荷が高いとホスト→実機の新規 TCP 接続が落ちる。** 試験時は load average
   36–43、abs 500 スレッド。`curl` が `time_connect=0` でタイムアウトする一方、
   実機内 `127.0.0.1` は常に応答しました。mirakc 40772 も同じ確率で失敗するため、
   8484 固有ではありません（ホストは既定ゲートウェイ経由）。再試行で 200 が返ります。
   サーバは接続ごとにスレッドを立て、読み取りは 5 秒で打ち切ります。
3. **4K 101 マクロは未押下。** 4K 選局の実機検証は他作業の完了後に実施してください。
4. **色キーの並び（青/緑/赤/黄 ⇔ `0x028–0x02B`）は未確定。** 名前は
   `RC_COLOR_BUTTON1–4` で、UI の色は暫定です。
5. **`GET /api/status` の `si` は `/tmp/adsb` の中身に依存。** 選局遷移中は `ok:false`。
6. busybox `ps` は同一セッションのプロセスしか表示しません。デーモンの確認は
   `ps -A` か `/tmp/t4s-remote.pid` を使ってください。
7. 認証がないため、`--bind 127.0.0.1` での利用や、リバースプロキシ配下での
   認証付与を検討してください（現状の実装は `--bind` を受け付けます）。

---

## 10. 電源再投入後の自動起動と復旧

`scripts/ondevice-up.sh` は USB マウント → mirakc 起動のあと、
`/mnt/t4s/opt/t4s/bin/t4s-remote --daemon --port 8484` を実行します。

- **冪等**: `/tmp/t4s-remote.pid` の pid が生存していれば起動せず
  `t4s-remote already running` を出します。stale pid は削除してから起動。
- **mirakc を止めない**: バイナリ欠落・8484 bind 失敗など remote の起動失敗は
  ログに出すだけで、`start_remote` は成功扱いで戻ります。mirakc は稼働継続。
- mirakc が既に動いていても remote の起動チェックは毎回実行されます
  （以前の「mirakc 稼働中なら即 exit」は廃止）。

再投入後の復旧手順（ホスト側）:

```sh
# 1. 状態（pid / ps / LISTEN / ログ末尾）
sh scripts/ondevice-remote.sh status

# 2. 停止していれば起動（生存中なら "already running" で no-op）
sh scripts/ondevice-remote.sh start

# 3. LAN 疎通（すべて 200 を確認）
curl -s -o /dev/null -w '%{http_code}\n' http://192.168.1.154:8484/
curl -s http://192.168.1.154:8484/api/keys | \
  python3 -c 'import json,sys; print(len(json.load(sys.stdin)))'   # 318
curl -s http://192.168.1.154:8484/api/status
```

実機上で直接確認する場合:

```sh
ps -A | grep t4s-remote
netstat -ltn | grep :8484
tail -5 /tmp/t4s-remote.log
/mnt/t4s/opt/t4s/bin/t4s-remote --daemon --port 8484   # 未起動なら
```

8484 が LISTEN なのにホストから接続できないときは、**mirakc 40772 と
ping / telnet 2323 も同時に確認**してください。実測ではホスト→実機の経路
（`192.168.100.100` → gateway `192.168.100.3` → `192.168.1.154`）で
新規 TCP SYN が落ちることがあり、40772 も同じ確率でタイムアウトしました。
サーバ停止ではありません。数秒おいて再試行してください。

注意: `ondevice-remote.sh start` はデバイス側で pid を確認するため、
`/mnt/t4s` がアンマウントされている／telnet が落ちている場合は
先に `ondevice-up.sh`（USB マウントと mirakc 起動）を実行してください。
