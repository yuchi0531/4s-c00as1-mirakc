# PROGRESS — SHARP 4S-C00AS1 + mirakc

日付: 2026-11-15。実機 192.168.1.154 (root telnet 2323)。本体ストレージ（/mmc, /mmc2, squashfs）は未変更。データは USB 上の ext4 イメージ `/media/usb/t4s.img` → `/mnt/t4s`。

## 1. 動くもの（実証済み）

- **2K（BS/CS）直接 TS 取得**: `t4s-tuner --type BS --channel BS13_0`（physical TSI0→logical TSI1 remap）、`--type CS --channel CS8`（CS も同じ remap 経路が既定）。DEMUX ring を 18MiB に拡張したことで**リブートなしで連続取得**が成立。
  - 実測（同一ブート、10秒間隔）: CS8 31,490,752B rc=0 / BS13_0 16,088,100B rc=0 / CS8 32,459,704B rc=0。`os error 1038` なし。
- **2K mirakc API**: `/api/channels/CS/CS8/stream?decode=0` 5/5 成功、`/api/channels/BS/BS13_0/stream?decode=0` 4/4 成功（HTTP 200、先頭 0x47、188B sync 1.0000）。ffprobe: CS8 program 55 がクリア（MPEG-2 1440x1080 + AAC LC）。
- **mirakc チャンネル登録**: `/api/channels` 24件（BS 6 / CS 12 / BS4K 6）。
- **4K（BS4K）直接取得**: 製品が緑（4K 視聴中）のとき `t4s-tuner --type BS4K --channel 45170` で 3回連続 rc=0 / 1038 なし（30.8MB / 31.1MB / 31.1MB）。ffprobe: HEVC Main10 3840x2160 yuv420p10le / arib-std-b67(HLG) / bt2020 + aac_latm 48kHz 2ch。
- **4K mirakc API 単発**: `/api/channels/BS4K/45170/stream?decode=1` → HTTP 200 / 66.9MB / HEVC HLG。
- **WebUI リモコン `t4s-remote`** (8484): msgId `0x12044`（`0x2044` は無効）、type0=押下 / type2=離上、keep-alive、hold 200ms 可変、終了ボタン、ENTER 警告。build 2026-11-14b。
- **mirakc-webui 静的 SPA**: `http://192.168.1.154:40772/www`（303 → `/www/index.html` 200）。
- **起動手順**: `sh /media/usb/ondevice-up.sh`（vfat を rw remount → loop0 → /mnt/t4s → mirakc + t4s-remote）。

## 2. 重要な発見

- **HDMI シンクは 4K に必須ではない。** `/dev/xcfb_disp` ioctl `0x61000` の read-only プローブで **HPD=0 / RSEN=0 / EDID 0ブロック** を実測。その状態で 4K live・取得・切替に成功した。
- **必要なのは「緑状態」= 製品が実際に 4K をデコードしていること**（MIXER/Sink/buf_buf=true, discover=Ok, video_readers=1 / audio_readers=2）。
- **緑に入る方法は本体ボタンでの電源ON（前回チャンネル復元）**。ソフトリブート（busybox reboot）では緑に入らない。
- **4K 切替**: `0x6004`（`t4s-probe send <service> 0 0`）は有効。CH UP キー（uimkey 0x00C）は 4K 局を選べない。
- **discover=Err の主因**: mirakc API の同一局連続取得（短間隔）と、`discover=Err` 時に 0x6004 を送るフォールバック。後者は Err を悪化させ、復帰に物理ボタン＋再起動が必要になる。
- **Err からのソフト復帰は不可**（send / resume / adsbinit いずれも無効）。
- **CS のスクランブル**: CS8 は program 55 がクリア、218 はスクランブル。`decode=1` は契約（provider 0x0800 未契約）により不可。
- **XCFB ioctl ABI**: 要求番号 `0x61000` 固定、arg struct `{cmd, token, arg1, arg2}`（16B）。READ_REG 0x520 / WRITE_REG 0x521 / plug_state 0x523,0x524 / EDID_GET_BLOCK_NUM 0x50b / EDID_READ_BLOCK 0x50c。

## 3. 現状（制約・未解決）

- **4K mirakc API の連続取得は不安定**。SIGTERM 明示 close、Tap::open の 1038 リトライ、TSOUT ring 24MiB、close 後 settle 待ち、discover リトライ、t4s-tuner からの 0x6004 撤去（commit `9b41ea4`）を実装済み。ただし**実機での 5回連続検証は未完**（旧版では 3/3 合格と 2/3 で 408 の両方が観測され非決定）。
- **2K の `decode=1`** は `t4s-cas` の起動ゲート修正（commit `d23cae7`、部分 PMT 開始）でコード修正済みだが**実機未検証**。
- **`/api/services` は 0件**（scan-services 無効・services DB 未作成）。チャンネル単位の `/api/channels/{type}/{channel}/stream` は使える。
- **4K の運用条件**: 人が本体ボタンで緑にしてから **1局1回**。録画予約・自動化・連続配信は不可。
- **4K の Err 復帰**: 物理ボタン＋再起動のみ。

## 4. 主要 commit（直近）

```
9b41ea4 fix(t4s-tuner): make repeated 4K API captures survive teardown races
4dcfcec docs(bsp4k): §18.5 SIGTERM-fix verification (inconclusive)
c64ab6c fix(t4s-tuner): finish 4K tap teardown inside mirakc's SIGTERM grace
dccd493 docs(bsp4k): §17 API 4K consecutive capture still fails
4cc9060 docs(bsp4k): §16.7 passing 3x consecutive 4K capture
a11000f fix(remux4k): idempotent 4K tap teardown + XCE_TIMEOUT retry
d23cae7 fix(t4s-cas): start BS2K with arrived PMTs
82a4e1f feat(config): register all BS/CS/BS4K channels
5981b11 fix(scripts): remount,rw vfat then bind for the USB image
5fbf021 feat(t4s-remote): keep-alive, 200ms hold, END button, ENTER warning
cae19c3 feat(webui): mount static mirakc-webui SPA at /www
```

## 5. 次の一手

1. 4K API 5回連続の実機検証（緑＋再起動 → 新バイナリ md5 `82f6b2b183bf3b48eb70bd8ff4e284a6` 配置 → 10秒間隔で5回）。
2. 2K `decode=1` の実機検証（t4s-cas 部分 PMT 開始）。
3. 実運用は 2K を主、4K は手動・単発の制約付き。

## 6. 参考ドキュメント

`docs/analysis/phase4-bsp4k.md`（§9–§19: HDMI 不要、緑必須、Err 条件、teardown/SIGTERM）、`docs/4K-NO-HDMI.md`、`docs/CS-STATUS.md`、`docs/ARCHITECTURE.md`、`docs/REMOTE-UI.md`、`docs/WEBUI.md`。
