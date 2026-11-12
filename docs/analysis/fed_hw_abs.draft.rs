//! Phase 0 draft — `abs`（ADSB/UI 層）と `xcdrv.ko`（ioctl ABI）の静的解析結果。
//!
//! 対象:
//! * `.tmp/oem/abs`      md5 `9fc3e99e628ba2893d3192d0319153e8` (ELF32 ARM, 17.6 MB)
//! * `.tmp/oem/xcdrv.ko` md5 `25bc580fb594958c9a7de4d475a4061d` (ELF32 ARM ET_REL, 2.0 MB)
//!
//! 参照リポジトリの既知 md5（`def4436…` / `3db7595…`）とは**別ビルド**。固定アドレスは流用していない。
//! 根拠は REA 6.4.0 (Ghidra 12.1.4) の疑似コード、ELF relocation、`llvm-objdump`。
//! アドレスは VA。file offset は `abs` が `VA - 0x8000`、`xcdrv.ko` が
//! `.text = VA + 0x34` / `.data = VA + 0x17b300`。
//!
//! **このファイルは計画生成のみ。実機 I/O、`ioctl`、ファイル open、sleep は含めない。**
//! 未確定は `UNCONFIRMED` コメントで明示する。

#![allow(dead_code)]

// ---------------------------------------------------------------------------
// 0. 根拠アドレス（VA。file offset は abs = VA-0x8000、tuner text = VA-0x8000）
// ---------------------------------------------------------------------------

pub mod va {
    // abs (`[DIS]`, `.dynsym` は `readelf -sW`)
    pub const BHV_TV_ADSB_TUNE_CHANGE_CHANNEL: u32 = 0xc66a84; // file 0xc5ea84
    pub const VETM_MAIN: u32 = 0x457550; // file 0x44f550
    pub const VETM_SEND_6004: u32 = 0x457858; // = VETM_MAIN + 0x308, file 0x44f858
    pub const STR_BHV_TV_LOG: u32 = 0xf90208; // file 0xe88208
    pub const STR_EADSB_COMM_TUNE: u32 = 0xddbf8c; // file 0xdd3f8c (関数未特定)

    // tuner (`[REA]`)
    pub const V_FEDM_MAIN: u32 = 0xaf28;
    pub const V_FED_ADBS_LOCAL_MAIN: u32 = 0xebe0;
    pub const V_FED_ADBS_SET_TUNING_START: u32 = 0x10004;
    pub const F_FED_DEMOD_SET_TS: u32 = 0x17610;
    pub const TUNE_ISDBS3_SET_TSID: u32 = 0x20b2c;
    pub const F_FED_TUNER_SET_FREQ: u32 = 0x17ab8;
    pub const TUNE_ISDBS3_SET_FREQ: u32 = 0x20f9c;
    pub const F_FED_TUNER_SEQ_START: u32 = 0x17b68;
    pub const TUNE_ISDBS3_SEQ_START: u32 = 0x21030;
    pub const TUNE_ISDBS_RF_STANDBY: u32 = 0x20550;
    pub const TUNE_ISDBS_OFDM_STANDBY: u32 = 0x206b4;
    pub const TUNE_ISDBS3_IS_TLV_LOCKED: u32 = 0x21304;
}

// ---------------------------------------------------------------------------
// 1. `abs` → 製品 ADSB キューの SysV メッセージ（4K 選局要求）
// ---------------------------------------------------------------------------

/// ADSB 通信キューの product 内 qid。実 msqid は起動ごとに変わるので
/// `/tmp/bdh_out_msgh.lst` から解決する。
/// evidence: `abs:0x4578da` の `movw r0, #0x36f`（`[DIS]`）。
pub const QID_ADSB_COMM: u32 = 0x36F; // 879

/// 4K/ADSB 選局要求の msgId。
/// evidence: `abs:0x4578de` の `movw r1, #0x6004`（`[DIS]`）。
pub const MSG_ADSB_TUNE_CHANGE_CHANNEL: u32 = 0x6004;

/// `0x6004` の body 長（10 word = 40 B）。
/// evidence: `abs:0x4578e4` の `movs r3, #0x28`（`[DIS]`）。
pub const MSG_ADSB_TUNE_CHANGE_CHANNEL_LEN: u32 = 0x28;

/// 関連 msgId。4K 列での使用有無は未確定。
/// evidence: `abs:0x457934` (`0x6005`, 3 word), `abs:0x457976` (`0x6006`, 2 word)。
pub const MSG_ADSB_6005: u32 = 0x6005;
pub const MSG_ADSB_6006: u32 = 0x6006;

/// 4K 選局要求 body の word index（`vEtmMain+0x308` @ `abs:0x457858`）。
/// UNCONFIRMED: 受信側の解釈は `eAdsbCommTuneChangeChannel`（関数未特定）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdsbTuneBody {
    pub reserved: u32,    // +0x00 常に 0
    pub oid: u32,         // +0x04 操作連番
    pub dev: u32,         // +0x08 9/10/11
    pub tune: u32,        // +0x0c Tune (0/1/3/7)
    pub nid: u32,         // +0x10 network id (4K は 0x0b)
    pub sid: u32,         // +0x14 service id
    pub rf_khz: u32,      // +0x18 RF downlink kHz
    pub tsid: u32,        // +0x1c TLV stream id
    pub not_decode: u32,  // +0x20 NotDecode
    pub bid: u32,         // +0x24 remote key id
}

impl AdsbTuneBody {
    /// body を 10 word に展開する。実 I/O は含まない。
    pub fn to_words(self) -> [u32; 10] {
        [
            self.reserved,
            self.oid,
            self.dev,
            self.tune,
            self.nid,
            self.sid,
            self.rf_khz,
            self.tsid,
            self.not_decode,
            self.bid,
        ]
    }
}

/// `BhvTvAdsbTuneChangeChannel` が `Dev` に写像する device bit。
/// evidence: `abs:0xc66b64–0xc66b96`（`0x200→9`, `0x400→0xa`, `0x800→0xb`）。
pub const DEV_ADBS1: u32 = 0x200; // → 9
pub const DEV_ADBS2: u32 = 0x400; // → 10
pub const DEV_ADBS3: u32 = 0x800; // → 11

pub const fn adsb_dev_code(dev_bit: u32) -> Option<u32> {
    match dev_bit {
        DEV_ADBS1 => Some(9),
        DEV_ADBS2 => Some(10),
        DEV_ADBS3 => Some(11),
        _ => None,
    }
}

/// `BhvTvAdsbTuneChangeChannel` (`abs:0xc66a84` / file `0xc5ea84`) の引数構造体。
/// `arg2` のオフセットは `[DIS] 0xc66c02–0xc66c38`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TuneChangeParam {
    pub nid: u16,     // +0x14
    pub sid: u16,     // +0x18
    pub rf_khz: u32,  // +0x1c
    pub tsid: u16,    // +0x20
}

/// `BhvTvAdsbTuneChangeChannel` の park / timer 定数。
/// evidence: `abs:0xc66abc` (`memcpy 0x5c`), `0xc66ad2` (`0x12096`), `0xc66aca` (`100`).
pub const TUNE_PARK_LEN: usize = 0x5C;
pub const TUNE_TIMER_EVENT: u32 = 0x12096;
pub const TUNE_TIMER_ARG: u32 = 100; // UNCONFIRMED: ms か tick か

/// UI 層の関数骨格（実装はまだ。実 I/O なし）。
/// 1 回目は park + timer、成立時に `vEtmMain+0x308` 相当の送信を行う、という観測。
/// UNCONFIRMED: 送信を起こすのが timer か 2 回目の UI 呼び出しか。
#[derive(Debug, Clone, Copy)]
pub enum TuneChangeStep {
    Park { param: TuneChangeParam, tune: u32, len: usize },
    ArmTimer { event: u32, arg: u32 },
    BuildBody { dev: u32, tune: u32, param: TuneChangeParam },
    Send { qid: u32, msg_id: u32, len: u32 },
}

/// `bAdsbInitialized` が 0 の間は park + timer のみ。初期化後は body を組んで送信する。
/// evidence: `[REA]` `abs:0xc66a84` の `if (bAdsbInitialized == 0) { … return 1; }`。
pub fn bhv_tv_adsb_tune_change_channel_plan(
    dev_bit: u32,
    tune: u32,
    param: TuneChangeParam,
    adsb_initialized: bool,
) -> Option<Vec<TuneChangeStep>> {
    if !adsb_initialized {
        return Some(vec![
            TuneChangeStep::Park { param, tune, len: TUNE_PARK_LEN },
            TuneChangeStep::ArmTimer { event: TUNE_TIMER_EVENT, arg: TUNE_TIMER_ARG },
        ]);
    }
    let dev = adsb_dev_code(dev_bit)?;
    Some(vec![
        TuneChangeStep::BuildBody { dev, tune, param },
        TuneChangeStep::Send {
            qid: QID_ADSB_COMM,
            msg_id: MSG_ADSB_TUNE_CHANGE_CHANNEL,
            len: MSG_ADSB_TUNE_CHANGE_CHANNEL_LEN,
        },
    ])
}

// ---------------------------------------------------------------------------
// 2. `xcdrv.ko` ioctl ABI（要求整形層 + 実行層）
// ---------------------------------------------------------------------------

/// `check_ioctl`（`xcdrv:0x17fe8` / file `0x1801c`）の表サイズ。
/// `cmd = *(u16 *)(req+0x8bc)` が `0x98` 未満のとき `cmd_check_dispatch[cmd]` を呼ぶ。
/// evidence: `[REA]` `check_ioctl` 疑似コード。
pub const XCDRV_CMD_TABLE_LEN: u16 = 0x98;

/// `cmd_check_dispatch` の実体: `.data + 0x1e0` / file `0x17b4e0`、4 B × 0x98。
pub const XCDRV_CMD_TABLE_VA: u32 = 0x1e0;
pub const XCDRV_CMD_TABLE_FILE: u32 = 0x17b4e0;

/// 各 cmd の要求サイズ（ハンドラが `req+0x8b8` に `strb` する値）。
/// `None` = null entry またはサブコマンド依存。
/// evidence: `[DIS]`/`[REA]` 各ハンドラ。完全表は `docs/analysis/phase0-abs.md §2.2`。
pub const fn request_size(cmd: u16) -> Option<u32> {
    match cmd {
        0x01 => Some(4),
        0x02 => Some(12),
        0x03 => Some(12),
        0x04 => Some(40),
        0x05 => Some(4),
        0x08 => Some(16),
        0x09 => Some(0),
        0x0a => Some(12),
        0x0b => Some(68),
        0x0c => Some(12),
        0x0d => Some(32),
        0x0e => Some(4),
        0x0f | 0x10 => Some(16),
        0x11 => Some(4),
        0x13 => Some(96),
        0x14 => Some(4),
        0x15 => Some(90),
        0x16 => Some(0),
        0x17 => Some(4),
        0x18 => Some(4),
        0x19 => Some(4),
        0x1a => Some(16),
        0x1b => Some(4),
        0x1c => Some(24),
        0x1d => Some(16),
        0x1e => Some(4),
        0x1f => Some(28),
        0x20 | 0x21 => Some(16),
        0x25 => Some(15),
        0x27 => Some(9),
        0x28 => Some(24),
        0x29 => Some(4),
        0x2a | 0x2b => None, // サブコマンド依存
        0x2c => Some(8),
        0x2d => Some(24),
        0x32 => Some(36),
        0x33 => Some(28),
        0x34 => Some(4),
        0x35 | 0x36 => None, // サブコマンド依存
        0x37 => Some(8),
        0x38 => Some(4),
        0x39 => Some(8),
        0x3a => Some(4),
        0x3b => Some(40),
        0x3c => Some(16),
        0x3d => Some(44),
        0x3e => Some(24),
        0x3f => Some(8),
        0x40 => Some(56),
        0x41 | 0x42 => Some(8),
        0x43 => Some(72),
        0x44 => Some(8),
        0x45 => Some(8),
        0x46 => Some(4),
        0x47 => Some(4),
        0x48 => Some(12),
        0x49 => Some(24),
        0x4a => Some(32),
        0x4b => Some(8),
        0x4c => Some(24),
        0x4d => Some(68),
        0x4e => Some(8),
        0x50 => Some(40),
        0x51 => Some(36),
        0x52 => Some(4),
        0x53 => Some(44),
        0x5a => Some(76),
        0x5b => None, // xc_crypto_dma_func: 疑似コードで即値なし
        0x5c => Some(16),
        0x5d => Some(4),
        0x5e => Some(28),
        0x5f => Some(3),
        0x60 => Some(28),
        0x64 => Some(16),
        0x65 => Some(12),
        0x66 => Some(36),
        0x67 => Some(4),
        0x68 => Some(0),
        0x6e => Some(12),
        0x6f => Some(40),
        0x70 => Some(4),
        0x71 => Some(4),
        0x72 => Some(12),
        0x73 => Some(16),
        0x76 => Some(12),
        0x78 => Some(12),
        0x7a => Some(12),
        0x7b => Some(8),
        0x7c => Some(12),
        0x82 => Some(36),
        0x83 => Some(40),
        0x84 => Some(20),
        0x85 => Some(76),
        0x86 => Some(32),
        0x87 => Some(36),
        0x88 => Some(4),
        0x89 => Some(36),
        0x8a => Some(36),
        0x8b => Some(8),
        0x8c => Some(8),
        0x8d => Some(8),
        0x8e | 0x8f | 0x90 => Some(4),
        0x91 => Some(12),
        0x92 => Some(8),
        0x93 => Some(8),
        0x94 => Some(12),
        0x95 => Some(12),
        0x96 => Some(8),
        0x97 => Some(24),
        _ => None,
    }
}

/// 既存 `xcdrv.rs` と一致した主要 ioctl。値は本ビルドの `cmd_check_dispatch` と
/// `kdrv_dev_ioctl` で確認済み。
pub mod ioctl {
    pub const SDEV_CREATE: u16 = 0x28; // 24 B, xc_sdev_create_func
    pub const SDEV_DESTROY: u16 = 0x29; // 4 B (値渡し)
    pub const SDEV_CTRL: u16 = 0x2A; // sub = class*100+idx
    pub const SDEV_INFO: u16 = 0x2B; // sub
    pub const SBUF_CREATE: u16 = 0x32; // 36 B
    pub const SBUF_DESTROY: u16 = 0x34; // 4 B
    pub const SBUF_CTRL: u16 = 0x35; // sub
    pub const SBUF_INFO: u16 = 0x36; // sub
    pub const SBUF_CREATE_READ: u16 = 0x37; // 8 B
    pub const SBUF_DESTROY_READ: u16 = 0x38; // 4 B
    pub const SBUF_GET_READ: u16 = 0x3B; // 40 B
    pub const SBUF_COMMIT_READ: u16 = 0x3C; // 16 B
    pub const SBUF_CANCEL_GET: u16 = 0x3F; // 8 B
    pub const PIPELINE_TIME: u16 = 0x48; // 12 B
    pub const MEM_MAP: u16 = 0x7A; // 12 B
    pub const MEM_UNMAP: u16 = 0x7B; // 8 B
    pub const TSI_CONFIG: u16 = 0x50; // 40 B
    pub const TSI_STATUS: u16 = 0x51; // 36 B
    pub const GET_GRAPH: u16 = 0x86; // 32 B
    pub const GET_TEMPERATURE: u16 = 0x64; // 16 B

    /// `0x0001_0035` set_ready / `0x0003_0035` force_reset。
    pub const fn sub(cmd: u16, sub: u32) -> u32 {
        (sub << 16) | cmd as u32
    }
    pub const SBUF_SET_READY: u32 = sub(SBUF_CTRL, 0x0001);
    pub const SBUF_FORCE_RESET: u32 = sub(SBUF_CTRL, 0x0003);
    pub const SBUF_INFO_STATUS: u32 = sub(SBUF_INFO, 0x0001);

    pub const SDEV_RUN: u32 = sub(SDEV_CTRL, 0x0001);
    pub const SDEV_STOP: u32 = sub(SDEV_CTRL, 0x0003);
    pub const SDEV_CONNECT_INPUT: u32 = sub(SDEV_CTRL, 0x0007);
    pub const SDEV_CONNECT_OUTPUT: u32 = sub(SDEV_CTRL, 0x0008);
    pub const SDEV_DISCONNECT: u32 = sub(SDEV_CTRL, 0x0009);
    pub const SDEV_SET_PIPELINE: u32 = sub(SDEV_CTRL, 0x000C);
    pub const SDEV_GET_STATUS: u32 = sub(SDEV_INFO, 0x0001);

    /// REMUX class 11。`ABI.md §2.1` と全一致。
    pub const REMUX_CONFIG: u32 = sub(SDEV_CTRL, 0x044C); // 28 B
    pub const REMUX_ADD_PROGRAM: u32 = sub(SDEV_CTRL, 0x044D); // 44 B
    pub const REMUX_REMOVE_PROGRAM: u32 = sub(SDEV_CTRL, 0x044E); // 8 B
    pub const REMUX_ADD_VIDEO: u32 = sub(SDEV_CTRL, 0x044F); // 28 B
    pub const REMUX_ADD_AUDIO: u32 = sub(SDEV_CTRL, 0x0450); // 28 B
    pub const REMUX_ADD_PSI: u32 = sub(SDEV_CTRL, 0x0452); // 20 B
    pub const REMUX_REMOVE_STREAM: u32 = sub(SDEV_CTRL, 0x0454); // 12 B
    pub const REMUX_CONFIG_VIDEO: u32 = sub(SDEV_CTRL, 0x0456); // 36 B
    pub const REMUX_SET_COPY_PERMISSION: u32 = sub(SDEV_CTRL, 0x0459); // 8 B

    /// TSI/DEMUX class 5。
    pub const SDEV_CONFIG_TSI: u32 = sub(SDEV_CTRL, 0x01F4);
    pub const DEMUX_ADD_PID_FILTER: u32 = sub(SDEV_CTRL, 0x01F5); // 36 B
    pub const DEMUX_REMOVE_PID_FILTER: u32 = sub(SDEV_CTRL, 0x01F6); // 12 B
    pub const DEMUX_PID_FILTER_INFO: u32 = sub(SDEV_INFO, 0x01F5); // 16 B
}

/// `0x2A` / `0x2B` のサブコマンド分解。
/// `class = sub / 100`, `index = sub % 100`（`xc_sdev_ctrl_func` / `xc_sdev_info_func`）。
pub const fn split_sub(sub: u32) -> (u32, u32) {
    (sub / 100, sub % 100)
}

// ---------------------------------------------------------------------------
// 3. tuner FED メッセージ / 電源 / semaphore
// ---------------------------------------------------------------------------

/// tuner の FEM キュー（`eSysSendMessage(0x1c7, 0x2a, …)` の宛先）。
/// evidence: `[REA]` `vFedAdbsSetTuningStart` (`tuner:0x10004`) 末尾。
pub const QID_FEM: u32 = 0x1C7; // 455

/// `vFedAdbsLocalMain` が扱う FED メッセージ ID。
/// evidence: `[REA]`/`[STR]` `tuner:0xebe0`。
pub const MSG_FED_SET_TUNING_START: u32 = 0x2;
pub const MSG_FED_DISCONNECT: u32 = 0x4;
pub const MSG_FED_CTRL_TUNER_POWER_ON: u32 = 0xC;
pub const MSG_FED_CTRL_TUNER_POWER_OFF: u32 = 0xD;

/// FEM が扱うメッセージ名（`tuner:0x28b9c`, `0x28bbc`）。
pub const MSG_FEM_CHANGE_DEVICE: u32 = 0x2;
pub const MSG_FEM_DISCONNECT_DEVICE: u32 = 0x4;

/// 4K FED の device bit（tuner 側）。2K は `2/4/8`。
/// evidence: `[REA]` `eFed_ISDB_S3_TunerSetChannel` (`0x1ae50`) / `eFed_ISDB_S_TunerSetChannel` (`0x1ad80`)。
pub const DEV_ADBS_S3: [u32; 3] = [0x200, 0x400, 0x800];
pub const DEV_ISDBS_2K: [u32; 3] = [0x2, 0x4, 0x8];
pub const DEV_DISCONNECTED: u32 = 1;

pub fn rf_unit_from_dev_s3(dev: u32) -> Option<u8> {
    match dev {
        0x200 => Some(0),
        0x400 => Some(1),
        0x800 => Some(2),
        _ => None,
    }
}

pub fn rf_unit_from_dev_2k(dev: u32) -> Option<u8> {
    match dev {
        2 => Some(0),
        4 => Some(1),
        8 => Some(2),
        _ => None,
    }
}

/// 製品のプロセス間セマフォ。**attach-only**（作成・初期化しない）。
/// evidence: `[REA]`.
pub const SEM_DEMOD: u32 = 0x3C5;
pub const SEM_RF: u32 = 0x3C6;

/// tuner FED の delay / timeout（`phase0-tuner.md §5` と整合）。
pub const DELAY_AFTER_RF_PLL_MS: u32 = 10;
pub const TUNE_TIMEOUT_MS_ADBS: u32 = 2000;
pub const FEM_INT_WAIT_TICKS: u32 = 0x65; // 101 × eSysSleepTask(1)
pub const LOCK_POLL_SHORT_TICKS: u32 = 0x0C; // × eSysSleepTask(10)
pub const LOCK_POLL_LONG_TICKS: u32 = 0x14; // × eSysSleepTask(0x32)

/// 4K 選局の FED 手順。順序は `SetTsid → SetFreq → SeqStart → lock`。
/// evidence: `[REA]` `vFedAdbsSetTuningStart` (`tuner:0x10004`)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FedTuneStep {
    WaitFemProgress { max_ticks: u32 },
    TakeSem { id: u32 },
    SetTsid { dev: u32, tsid: u16, mode: u32 },
    SetFreq { dev: u32, if_khz: u32, mode: u32 },
    SeqStart { dev: u32, mode: u32 },
    PublishStarted,
    Reply { qid: u32, msg_id: u32, len: u32 },
    ReleaseSem { id: u32 },
}

pub fn adsb_tune_steps(dev: u32, tsid: u16, if_khz: u32) -> Option<Vec<FedTuneStep>> {
    if !DEV_ADBS_S3.contains(&dev) {
        return None;
    }
    if !(950_000..=3_300_000).contains(&if_khz) {
        return None;
    }
    Some(vec![
        FedTuneStep::WaitFemProgress { max_ticks: FEM_INT_WAIT_TICKS },
        FedTuneStep::TakeSem { id: SEM_DEMOD },
        FedTuneStep::SetTsid { dev, tsid, mode: 0xD },
        FedTuneStep::SetFreq { dev, if_khz, mode: 0xD },
        FedTuneStep::SeqStart { dev, mode: 0xD },
        FedTuneStep::PublishStarted,
        FedTuneStep::Reply { qid: QID_FEM, msg_id: MSG_FED_SET_TUNING_START, len: 0x14 },
        FedTuneStep::ReleaseSem { id: SEM_DEMOD },
    ])
}

/// `msg 0xD`（電源 OFF）の tuner 側手順。
/// evidence: `[REA]` `vFedAdbsLocalMain` / `fFedSetRfStandby` / `fFedSetDemodStandby`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FedPowerOffStep {
    LogTunerOff { dev: u32 },
    RfStandby { dev: u32, standby: bool },
    OfdmStandby { dev: u32, standby: bool },
    ClearTuningParam,
    Send { qid: u32, msg_id: u32, len: u32 },
}

pub fn fed_power_off_steps(dev: u32) -> Vec<FedPowerOffStep> {
    vec![
        FedPowerOffStep::LogTunerOff { dev },
        FedPowerOffStep::RfStandby { dev, standby: true },
        FedPowerOffStep::OfdmStandby { dev, standby: true },
        FedPowerOffStep::ClearTuningParam,
        FedPowerOffStep::Send { qid: 0x2A0, msg_id: 0x0, len: 0x0 }, // UNCONFIRMED: msgId/len
    ]
}

// ---------------------------------------------------------------------------
// 4. I2C（RF IC / SC1501A）の純粋な計画
// ---------------------------------------------------------------------------

/// RF IC の 7-bit slave。unit 0/1/2。
/// evidence: `[REA]` `FUN_0001c964` (`0x1c964`) → `I2c_Write(0, 0xc0/0xc2/0xc4, …)`。
pub const RF_I2C7_UNIT0: u8 = 0x60;
pub const RF_I2C7_UNIT1: u8 = 0x61;
pub const RF_I2C7_UNIT2: u8 = 0x62;

pub fn rf_slave7(unit: u8) -> Option<u8> {
    match unit {
        0 => Some(RF_I2C7_UNIT0),
        1 => Some(RF_I2C7_UNIT1),
        2 => Some(RF_I2C7_UNIT2),
        _ => None,
    }
}

/// SC1501A の 7-bit page アドレス。`0x10 + page`（page 0..8）。
/// evidence: `[REA]` `FUN_00025f60` (`0x25f60`)、`FUN_0001e020` (`0x1e020`)。
pub const DEMOD_I2C7_BASE: u8 = 0x10;

pub fn demod_slave7(unit: u8, page: u8) -> Option<u8> {
    let base = (unit & 7) * 0x20 + DEMOD_I2C7_BASE;
    base.checked_add(page)
}

/// 1 回の `I2c_Write` で送る byte 列。tuner は 1/2/4/6 byte を分割しない。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RfWrite {
    pub unit: u8,
    pub reg: u8,
    pub data: Vec<u8>,
}

/// RF PLL の `0x10` 書込み列（`if_khz >> 2` を 3 byte + `ff 00 01`）。
/// evidence: `[REA]` `x_tune_isdbs` (`0x1c500`)。詳細は `fed_hw_tuner.draft.rs`。
pub fn rf_pll_bytes(if_khz: u32) -> [u8; 6] {
    let n = if_khz >> 2;
    [
        (n & 0xFF) as u8,
        ((n >> 8) & 0xFF) as u8,
        ((n >> 16) & 0x0F) as u8,
        0xFF,
        0x00,
        0x01,
    ]
}

/// TSID 設定の復調操作。4K は `FUN_00022d4c` 相当（`0x31` mask, `0x2f`, `0x30`）。
/// UNCONFIRMED: 4K の `FUN_00022be4(…, 0x18, tsid)` の最終 reg 列は Phase 1 で確認。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DemodOp {
    Write { unit: u8, page: u8, reg: u8, value: u8 },
    MaskWrite { unit: u8, page: u8, reg: u8, mask: u8, value: u8 },
}

pub fn set_tsid_ops(unit: u8, tsid: u16) -> [DemodOp; 3] {
    [
        DemodOp::MaskWrite { unit, page: 2, reg: 0x31, mask: 0x08, value: 0x00 },
        DemodOp::Write { unit, page: 2, reg: 0x2F, value: (tsid >> 8) as u8 },
        DemodOp::Write { unit, page: 2, reg: 0x30, value: tsid as u8 },
    ]
}

// ---------------------------------------------------------------------------
// 5. lock 待ちの計画
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockProbe {
    TlvLocked { dev: u32 },
    TmccLocked { dev: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LockPollPlan {
    pub probe: LockProbe,
    pub timeout_ms: u32,
    pub short_poll_ticks: u32,
    pub short_poll_count: u32,
    pub long_poll_ticks: u32,
    pub long_poll_count: u32,
}

pub const fn adsb_lock_poll_plan(dev: u32) -> LockPollPlan {
    LockPollPlan {
        probe: LockProbe::TlvLocked { dev },
        timeout_ms: TUNE_TIMEOUT_MS_ADBS,
        short_poll_ticks: 10,
        short_poll_count: LOCK_POLL_SHORT_TICKS,
        long_poll_ticks: 0x32,
        long_poll_count: LOCK_POLL_LONG_TICKS,
    }
}

// ---------------------------------------------------------------------------
// 6. 実機 I/O を持たない関数骨格（Phase 1 で実装する）
// ---------------------------------------------------------------------------

/// Phase 1: `/tmp/bdh_out_msgh.lst` から qid→msqid を解決する。
pub fn resolve_msqid(_qid: u32) -> Option<i32> {
    // 実装しない（実機 I/O 禁止）。既存 `rust/t4s-core/src/tuner.rs::resolve_msqid` を再利用する。
    None
}

/// Phase 1: `0x6004` を `msgsnd` する。ここでは計画だけ。
pub fn send_adsb_tune_change(_body: AdsbTuneBody) -> Result<(), &'static str> {
    Err("UNCONFIRMED: 受信側関数と msqid 解決が未了")
}

/// Phase 1: FED 選局列を実機へ適用する。ここでは計画だけ。
pub fn run_adsb_tune(_dev: u32, _tsid: u16, _if_khz: u32) -> Result<(), &'static str> {
    Err("UNCONFIRMED: lock poll / I2C 列は Phase 1")
}
