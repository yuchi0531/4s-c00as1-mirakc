# リモコン Web UI (`t4s-remote`)

SHARP 4S-C00AS1 の製品 UI に、LAN 経由でリモコンキーを送る小さな HTTP サーバです。
リモコン受光部が壊れているため、製品の通常キー経路である **Q_UIM キュー**
(qid `0x7F`、メッセージ `0x2044`、body `{u32 keyCode; u32 type}`) に直接メッセージを
書き込みます。送信には `t4s-core` の `tuner::send_raw_message` を使います。

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
sh scripts/ondevice-remote.sh start

# 状態
sh scripts/ondevice-remote.sh status

# 停止
sh scripts/ondevice-remote.sh stop
```

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
- `release`: 既定 `true`。`true` なら `type 0` → 80 ms → `type 2`。
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
  **ワーカースレッドで読み、2 秒待っても終わらなければ `ok:false` を返します**。
  成功値は 30 秒キャッシュし、その間は再読込しません。
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
| `RC_ENTER` | `0x015` | 決定 |
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

---

## 9. 既知の制約

1. **`regs` は I2C0 セマフォ待ちで `ok:false` になることがある。**
   実機では他作業の `t4s-probe` が I2C を保持していたため、2 秒で打ち切りました。
   製品が idle なら `t4s-core` 側の 5 秒待ちで成功します（成功値は 30 秒キャッシュ）。
2. **製品負荷が高いと HTTP が不安定。** 試験時は load average 32、abs 281 スレッド。
   同一ホストから mirakc の API も時々タイムアウトしたため、ネットワーク/負荷起因であり
   サーバ固有の問題ではありません。再試行で 200 が返ります。
3. **4K 101 マクロは未押下。** 4K 選局の実機検証は他作業の完了後に実施してください。
4. **色キーの並び（青/緑/赤/黄 ⇔ `0x028–0x02B`）は未確定。** 名前は
   `RC_COLOR_BUTTON1–4` で、UI の色は暫定です。
5. **`GET /api/status` の `si` は `/tmp/adsb` の中身に依存。** 選局遷移中は `ok:false`。
6. busybox `ps` は同一セッションのプロセスしか表示しません。デーモンの確認は
   `ps -A` か `/tmp/t4s-remote.pid` を使ってください。
7. 認証がないため、`--bind 127.0.0.1` での利用や、リバースプロキシ配下での
   認証付与を検討してください（現状の実装は `--bind` を受け付けます）。
