//! Phase 0 draft — 衛星 FED (ISDB-S 2K) の静的解析結果を定数化したもの。
//!
//! 対象バイナリ: `.tmp/oem/tuner` (ELF32 ARM, stripped, md5 `8d2ef1dc9bb8af7f92efefdd152e9362`)。
//! 参照リポジトリの固定アドレスは使わず、REA 6.4.0 (Ghidra 12.1.4) の関数・疑似コード・
//! assembly と `.data` の生バイト列から抽出した。**実機 I/O は含まない**（計画生成のみ）。
//!
//! アドレス表記: `tuner:0x…` は VA。`.text`/`.rodata` の file offset は `VA - 0x8000`、
//! `.data` は `VA - 0x10000`。
//!
//! 未確定事項は `UNCONFIRMED` コメントで明示する。特に
//! * `eSysSleepTask` の tick → ms 換算
//! * BCD→IF 変換後の path 依存補正 (`+0x25760` / `-0x12bb0`) の適用可否
//! * `DAT_0003672c` の初期値と遷移
//! は Phase 1 の実機トレースで確定する。

#![allow(dead_code)]

/// 復調器 1 レジスタ分の書き込み（7-bit slave は `DEMOD_I2C7_BASE + page`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DemodWrite {
    pub page: u8,
    pub reg: u8,
    pub value: u8,
}

impl DemodWrite {
    /// `FUN_00025f60` (tuner:0x25f60): `slave7 = 0x10 + page`。
    /// 実際の `I2c_Write` には `FUN_0001e020` (tuner:0x1e020) が `<<1` して 8-bit で渡す。
    pub const fn slave7(self) -> u8 {
        DEMOD_I2C7_BASE + self.page
    }
}

/// 復調器への RMW を含む操作。`MaskWrite` の意味は
/// `new = (old | (value & mask)) & (value | !mask)`（`FUN_0001e084` / tuner:0x1e084）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DemodOp {
    Write(DemodWrite),
    MaskWrite { page: u8, reg: u8, mask: u8, value: u8 },
}

/// RF IC への固定バイト列（unit は別途 `rf_slave7` で決める）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RfConst {
    pub reg: u8,
    pub data: &'static [u8],
}

/// RF IC への実書込み（所有 Vec）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RfWrite {
    pub reg: u8,
    pub data: Vec<u8>,
}

// ---------------------------------------------------------------------------
// トランスポート定数
// ---------------------------------------------------------------------------

/// 復調器の 7-bit ベースアドレス。page 0 → 0x10, page 7 → 0x17。
/// 根拠: `FUN_00025f60` (tuner:0x25f60) の `... + 0x10`、`FUN_0001e020` (tuner:0x1e020) の `<<1`。
pub const DEMOD_I2C7_BASE: u8 = 0x10;

/// RF IC の 7-bit アドレス（unit 0/1/2 = 0x60/0x61/0x62）。
/// 根拠: `FUN_0001c964` (tuner:0x1c964) が unit→8-bit `0xc0/0xc2/0xc4` を選び、
/// `write_tuner` (tuner:0x1ca3c) が `I2c_Write(0, 0xc0|…)` に渡す。
pub const RF_I2C7_UNIT0: u8 = 0x60;
pub const RF_I2C7_UNIT1: u8 = 0x61;
pub const RF_I2C7_UNIT2: u8 = 0x62;

/// 製品が使うプロセス間セマフォ。attach-only で取得する（作成・初期化しない）。
pub const SEM_DEMOD: u32 = 0x3c5; // Tune_ISDB_S_* (tuner:0x1ef5a 等)
pub const SEM_RF: u32 = 0x3c6; // x_pon/x_tune/x_fin_isdbs (tuner:0x1c3dc 等)

/// RF IC の unit → 7-bit slave。
pub const fn rf_slave7(unit: u8) -> Option<u8> {
    match unit {
        0 => Some(RF_I2C7_UNIT0),
        1 => Some(RF_I2C7_UNIT1),
        2 => Some(RF_I2C7_UNIT2),
        _ => None,
    }
}

/// FED dev (2K) → RF unit。`eFed_ISDB_S_TunerSetChannel` (tuner:0x1ad80) の分岐。
pub const fn rf_unit_from_dev_2k(dev: u32) -> Option<u8> {
    match dev {
        2 => Some(0),
        4 => Some(1),
        8 => Some(2),
        _ => None,
    }
}

/// FED dev (ISDB-S3/4K) → RF unit。`eFed_ISDB_S3_TunerSetChannel` (tuner:0x1ae50) の分岐。
pub const fn rf_unit_from_dev_s3(dev: u32) -> Option<u8> {
    match dev {
        0x200 => Some(0),
        0x400 => Some(1),
        0x800 => Some(2),
        _ => None,
    }
}

/// 復調 ctx の 1 unit あたりの stride。`SetTsid` の `0x3b938 + 0xc78 * unit`
/// （unit1 = tuner:0x3c5b0, unit2 = tuner:0x3d228）。
pub const DEMOD_CTX_STRIDE: u32 = 0xc78;
pub const DEMOD_CTX_BASE: u32 = 0x3b938;

/// ADC クロック。`SetTsid` が `ctx+0x18` に書く 25,000,000。
/// literal は tuner:0x1f158（file 0x17158）= `0x017d7840`。**delay ではない**。
pub const ADC_HZ: u32 = 25_000_000;

// ---------------------------------------------------------------------------
// RF IC 書き込み列
// ---------------------------------------------------------------------------

/// `x_pon_isdbs` (tuner:0x1c3d4)。起動時 `sc1501a_Init` から unit ごとに 1 回だけ。
/// 命令: tuner:0x1c448 / 0x1c46c / 0x1c490 / 0x1c4b4。明示 delay なし。
pub const RF_POWER_ON_ISDBS: &[RfConst] = &[
    RfConst { reg: 0x41, data: &[0x07, 0x00, 0x05] },
    RfConst { reg: 0x45, data: &[0x01, 0x00] },
    RfConst { reg: 0x0c, data: &[0x14, 0x00] },
    RfConst { reg: 0x7b, data: &[0x02, 0x01] },
];

/// `x_fin_isdbs` (tuner:0x1c7fc)。`eFed_ISDB_S_Tuner_standby_mode` から。
/// 命令: tuner:0x1c85a / 0x1c86c / 0x1c87e / 0x1c8a6 / 0x1c8b8 / 0x1c8ca / 0x1c8dc。
pub const RF_STANDBY_ISDBS: &[RfConst] = &[
    RfConst { reg: 0x15, data: &[0x12] },
    RfConst { reg: 0x43, data: &[0x05] },
    RfConst { reg: 0x45, data: &[0x01] },
    RfConst { reg: 0x0c, data: &[0x14, 0x00, 0x00] },
    RfConst { reg: 0x01, data: &[0x00] },
    RfConst { reg: 0x05, data: &[0x00] },
    RfConst { reg: 0x04, data: &[0xc0] },
];

/// `x_fin_isdbt` (tuner:0x1d99c)。衛星 `SetTsid` の冒頭で呼ばれる地デジ側 standby。
/// 命令: tuner:0x1d9fa / 0x1da0c / 0x1da34 / 0x1da46 / 0x1da58。
/// 既存 `gr_i2c.rs::standby_isdbt()` と一致。
pub const RF_STANDBY_ISDBT: &[RfConst] = &[
    RfConst { reg: 0x74, data: &[0x12] },
    RfConst { reg: 0x67, data: &[0x00] },
    RfConst { reg: 0x5e, data: &[0x15, 0x00, 0x00] },
    RfConst { reg: 0x88, data: &[0x00] },
    RfConst { reg: 0x87, data: &[0xc0] },
];

/// ISDB-S 2K の IF 範囲。`eFed_ISDB_S_TunerSetChannel` (tuner:0x1ad80) の clamp。
pub const IF_MIN_KHZ: u32 = 950_000;
pub const IF_MAX_KHZ_2K: u32 = 2_150_000; // 0x20ce70 (比較は 0x20ce71 未満)
pub const IF_MAX_KHZ_S3: u32 = 3_300_000;

/// RF PLL 書込み後、製品が固定で待つ時間。`x_tune_isdbs` tuner:0x1c75e–0x1c760
/// (`FUN_0001cba0` = `usleep(ms*1000)`)。
pub const DELAY_AFTER_RF_PLL_MS: u32 = 10;

/// `x_pon_isdbt` の `0x9b` 書込み後の待ち。`FUN_0001df6c(10)`。
pub const DELAY_AFTER_ISDBT_PON_MS: u32 = 10;

/// `if_khz >> 2` を 3 バイト little-endian + `ff 00 01` で `0x10` に書く。
/// `x_tune_isdbs` 命令 tuner:0x1c712–0x1c75a。
pub fn rf_pll_bytes(if_khz: u32) -> [u8; 6] {
    let n = if_khz >> 2;
    [
        (n & 0xff) as u8,
        ((n >> 8) & 0xff) as u8,
        ((n >> 16) & 0x0f) as u8,
        0xff,
        0x00,
        0x01,
    ]
}

fn rf_static(items: &[RfConst]) -> Vec<RfWrite> {
    items
        .iter()
        .map(|w| RfWrite { reg: w.reg, data: w.data.to_vec() })
        .collect()
}

pub fn rf_power_on_isdbs_plan() -> Vec<RfWrite> {
    rf_static(RF_POWER_ON_ISDBS)
}

pub fn rf_standby_isdbs_plan() -> Vec<RfWrite> {
    rf_static(RF_STANDBY_ISDBS)
}

pub fn rf_standby_isdbt_plan() -> Vec<RfWrite> {
    rf_static(RF_STANDBY_ISDBT)
}

/// `x_tune_isdbs` (tuner:0x1c500)。`is_s3=false` が 2K、`true` が ISDB-S3。
/// 書込み順は 15 列 + 10 ms 待ち（`0x10` の直後）。
pub fn rf_tune_isdbs_plan(if_khz: u32, is_s3: bool) -> Vec<RfWrite> {
    let mut out = Vec::with_capacity(15);
    let low_band = if_khz <= 0x20ce70;

    out.push(RfWrite { reg: 0x15, data: vec![0x12] });
    out.push(RfWrite { reg: 0x43, data: vec![if low_band { 0x04 } else { 0x05 }] });
    out.push(RfWrite { reg: 0x45, data: vec![if low_band { 0x01 } else { 0x00 }] });
    out.push(RfWrite { reg: 0x6a, data: vec![0x00, 0x00] });
    out.push(RfWrite { reg: 0x75, data: vec![0xf9] });
    out.push(RfWrite { reg: 0x40, data: vec![0x07] });
    out.push(RfWrite { reg: 0x41, data: vec![0x07] });
    out.push(RfWrite { reg: 0x48, data: vec![0x07] });
    out.push(RfWrite { reg: 0x01, data: vec![if low_band { 0x01 } else { 0x03 }] });
    out.push(RfWrite { reg: 0x04, data: vec![0xc4, 0x40] });

    // 0x06: 2K = 03 80 70 1e 02 24 / S3 = 03 00 b4 78 08 30 (tuner:0x1c668–0x1c6b8)
    out.push(RfWrite {
        reg: 0x06,
        data: if is_s3 {
            vec![0x03, 0x00, 0xb4, 0x78, 0x08, 0x30]
        } else {
            vec![0x03, 0x80, 0x70, 0x1e, 0x02, 0x24]
        },
    });

    // 0x0c: low band = fe 02 9e 16 / high band = fc 22 9e 16 (tuner:0x1c6ce–0x1c6f8)
    out.push(RfWrite {
        reg: 0x0c,
        data: if low_band {
            vec![0xfe, 0x02, 0x9e, 0x16]
        } else {
            vec![0xfc, 0x22, 0x9e, 0x16]
        },
    });

    let pll = rf_pll_bytes(if_khz);
    out.push(RfWrite { reg: 0x10, data: pll.to_vec() });
    // DELAY_AFTER_RF_PLL_MS = 10 は呼び出し側で挿入する（この plan は純データ）。

    out.push(RfWrite { reg: 0x05, data: vec![0x00] });
    out.push(RfWrite { reg: 0x04, data: vec![0xc0] });
    out
}

// ---------------------------------------------------------------------------
// 復調器の設定列
// ---------------------------------------------------------------------------

/// `DMD_set_tsout` system=1 (tuner:0x26d74)。順序は `0x0b` → `0xcc`。
pub const DEMOD_SET_TSOUT_ISDBS: [DemodOp; 2] = [
    DemodOp::MaskWrite { page: 0x00, reg: 0x0b, mask: 0x01, value: 0x01 },
    DemodOp::MaskWrite { page: 0x00, reg: 0xcc, mask: 0x80, value: 0x80 },
];

/// `FUN_00022d4c` param=0x2d (tuner:0x22d4c)。TSID は絶対選択（`0x31` bit3=0）。
/// 命令: tuner:0x22db6/0x22db8 (mask), 0x22de4 (0x2f), 0x22e0c (0x30)。
pub fn demod_set_tsid_ops(tsid: u16) -> [DemodOp; 3] {
    [
        DemodOp::MaskWrite { page: 0x02, reg: 0x31, mask: 0x08, value: 0x00 },
        DemodOp::Write(DemodWrite { page: 0x02, reg: 0x2f, value: (tsid >> 8) as u8 }),
        DemodOp::Write(DemodWrite { page: 0x02, reg: 0x30, value: (tsid & 0xff) as u8 }),
    ]
}

/// `Tune_ISDB_S_SeqStart` → `FUN_000228b0` system=1 (tuner:0x228b0)。
pub const DEMOD_SEQ_START_ISDBS: DemodWrite =
    DemodWrite { page: 0x02, reg: 0x92, value: 0x3f };

/// ISDB-S の `SetTsid` 全体（set_system の 168 列を含む版）。
/// `include_recommend` は起動後はじめての system 切替だけ true。
pub fn demod_isdbs_settsid_ops(tsid: u16, include_recommend: bool) -> Vec<DemodOp> {
    let mut out = Vec::new();
    if include_recommend {
        out.extend(DEMOD_REGSET_RECOMMEND_DIFF.iter().copied().map(DemodOp::Write));
    }
    out.extend(DEMOD_REGSET_ISDBS.iter().copied().map(DemodOp::Write));
    out.extend(DEMOD_SET_TSOUT_ISDBS);
    out.extend(demod_set_tsid_ops(tsid));
    out
}

// ---------------------------------------------------------------------------
// 周波数変換（BCD → LNB IF）
// ---------------------------------------------------------------------------

/// メッセージの BCD 周波数（例 `0x01172748`）を十進 kHz に展開する。
/// `FUN_00014518` (tuner:0x14518)、命令 tuner:0x145c0–0x14672。
pub fn wire_bcd_to_khz(bcd: u32) -> u32 {
    let mut out = 0u32;
    let mut mul = 1u32;
    let mut v = bcd;
    for _ in 0..8 {
        out += (v & 0xf) * mul;
        v >>= 4;
        mul *= 10;
    }
    out
}

/// LNB LO。`local = khz * 10 - 0xa2eef0`（tuner:0x14726–0x1472e の `movw #0x1110 / movt #0xff5d`）。
pub const LNB_LO_OFFSET_KHZ: u32 = 0x00a2_eef0; // 10,678,000 (10.678 GHz)

/// `wire_bcd_to_khz(0x01172748)` = 1,172,748 → `*10` → `- 10,678,000` = 1,049,480 kHz。
/// 返り値は補正前の IF kHz。
pub fn wire_bcd_to_if_khz(bcd: u32) -> Option<u32> {
    let khz = wire_bcd_to_khz(bcd).checked_mul(10)?;
    khz.checked_sub(LNB_LO_OFFSET_KHZ)
}

/// UNCONFIRMED: `FUN_00014518` の path 依存補正。
/// `tFemTuningSubParam[path].+8 == 1` のときだけ、帯域に応じて `+0x25760` / `-0x12bb0` が入る
/// (tuner:0x1475c–0x14856)。BS 2K パスで適用されるか未確定。ここでは計算しない。
pub const IF_TRIM_PLUS_KHZ: u32 = 0x0002_5760; // 153,440
pub const IF_TRIM_MINUS_KHZ: u32 = 0x0001_2bb0; // 76,720

// ---------------------------------------------------------------------------
// 上位シーケンスのタイミング（製品値）
// ---------------------------------------------------------------------------

/// 2K BS/CS の tune timeout。`vFedmMain` (tuner:0xaf29) が TuningStart msg `0x20` で
/// `eStmExSetTimerAfter(…, 0x36, …, 2000)` を張る。
pub const TUNE_TIMEOUT_MS_2K: u32 = 2000;

/// ADBS/4K の tune timeout。msg `0x2a` → timer `0x3b` = 2000 ms。
pub const TUNE_TIMEOUT_MS_ADBS: u32 = 2000;

/// `eSysSleepTask` の SetFreq 再試行前。`FUN_00014518` tuner:0x148a8 の `movs r0,#0x1e`。
/// UNCONFIRMED: tick の実時間（ms 換算）は未確定。
pub const SETFREQ_RETRY_SLEEP_TICKS: u32 = 0x1e;

/// FEM 割込み進捗待ちの上限回数（`FUN_00014518` 冒頭、`eSysSleepTask(1)` × 0x65）。
pub const FEM_INT_WAIT_TICKS: u32 = 0x65;

/// BS サーチ再試行の timer（`vFedLocalMain` msg `0x39`）。
pub const BS_SEARCH_RETRY_MS: u32 = 100;

/// 参考: CN/status poll。`vFedLocalMain` msg `0x11ff9`、`vFedAdbsLocalMain` も 2000 ms。
pub const STATUS_POLL_MS: u32 = 2000;

// ---------------------------------------------------------------------------
// 付録: 復調テーブル実体（`.data` の生バイト列を triple 展開したもの）
// ---------------------------------------------------------------------------

/// `regset_ISDBS` — VA `0x36c70`, file `0x26c70`, 168 entries + `ff ff ff`.
pub const DEMOD_REGSET_ISDBS: [DemodWrite; 168] = [
    // page 0x00 / slave7 0x10
    DemodWrite { page: 0x00, reg: 0x00, value: 0x18 }, // #0 @ 0x36c70
    DemodWrite { page: 0x00, reg: 0x01, value: 0x8e }, // #1 @ 0x36c73
    DemodWrite { page: 0x00, reg: 0x02, value: 0x21 }, // #2 @ 0x36c76
    DemodWrite { page: 0x00, reg: 0x03, value: 0xd4 }, // #3 @ 0x36c79
    DemodWrite { page: 0x00, reg: 0x04, value: 0x21 }, // #4 @ 0x36c7c
    DemodWrite { page: 0x00, reg: 0x05, value: 0x42 }, // #5 @ 0x36c7f
    DemodWrite { page: 0x00, reg: 0x07, value: 0x10 }, // #6 @ 0x36c82
    DemodWrite { page: 0x00, reg: 0x0b, value: 0x00 }, // #7 @ 0x36c85
    DemodWrite { page: 0x00, reg: 0x0c, value: 0x00 }, // #8 @ 0x36c88
    DemodWrite { page: 0x00, reg: 0x10, value: 0x00 }, // #9 @ 0x36c8b
    DemodWrite { page: 0x00, reg: 0x11, value: 0x00 }, // #10 @ 0x36c8e
    DemodWrite { page: 0x00, reg: 0x12, value: 0x00 }, // #11 @ 0x36c91
    DemodWrite { page: 0x00, reg: 0x16, value: 0x00 }, // #12 @ 0x36c94
    DemodWrite { page: 0x00, reg: 0x18, value: 0x00 }, // #13 @ 0x36c97
    DemodWrite { page: 0x00, reg: 0x1a, value: 0x10 }, // #14 @ 0x36c9a
    DemodWrite { page: 0x00, reg: 0x1c, value: 0x00 }, // #15 @ 0x36c9d
    DemodWrite { page: 0x00, reg: 0x1d, value: 0x01 }, // #16 @ 0x36ca0
    DemodWrite { page: 0x00, reg: 0x2a, value: 0xc4 }, // #17 @ 0x36ca3
    DemodWrite { page: 0x00, reg: 0x2b, value: 0xe0 }, // #18 @ 0x36ca6
    DemodWrite { page: 0x00, reg: 0x2c, value: 0x00 }, // #19 @ 0x36ca9
    DemodWrite { page: 0x00, reg: 0x39, value: 0x1c }, // #20 @ 0x36cac
    DemodWrite { page: 0x00, reg: 0x3a, value: 0x29 }, // #21 @ 0x36caf
    DemodWrite { page: 0x00, reg: 0x3d, value: 0x1c }, // #22 @ 0x36cb2
    DemodWrite { page: 0x00, reg: 0x3e, value: 0x29 }, // #23 @ 0x36cb5
    DemodWrite { page: 0x00, reg: 0x41, value: 0x1c }, // #24 @ 0x36cb8
    DemodWrite { page: 0x00, reg: 0x42, value: 0x29 }, // #25 @ 0x36cbb
    DemodWrite { page: 0x00, reg: 0x45, value: 0x1c }, // #26 @ 0x36cbe
    DemodWrite { page: 0x00, reg: 0x46, value: 0x29 }, // #27 @ 0x36cc1
    DemodWrite { page: 0x00, reg: 0x63, value: 0x17 }, // #28 @ 0x36cc4
    DemodWrite { page: 0x00, reg: 0x65, value: 0x50 }, // #29 @ 0x36cc7
    DemodWrite { page: 0x00, reg: 0x6c, value: 0x82 }, // #30 @ 0x36cca
    DemodWrite { page: 0x00, reg: 0x84, value: 0x00 }, // #31 @ 0x36ccd
    DemodWrite { page: 0x00, reg: 0x85, value: 0x05 }, // #32 @ 0x36cd0
    DemodWrite { page: 0x00, reg: 0x97, value: 0xe1 }, // #33 @ 0x36cd3
    DemodWrite { page: 0x00, reg: 0xc6, value: 0x01 }, // #34 @ 0x36cd6
    DemodWrite { page: 0x00, reg: 0xc9, value: 0x00 }, // #35 @ 0x36cd9
    DemodWrite { page: 0x00, reg: 0xcd, value: 0x41 }, // #36 @ 0x36cdc
    DemodWrite { page: 0x00, reg: 0xd9, value: 0x00 }, // #37 @ 0x36cdf
    // page 0x01 / slave7 0x11
    DemodWrite { page: 0x01, reg: 0x01, value: 0x12 }, // #38 @ 0x36ce2
    DemodWrite { page: 0x01, reg: 0x02, value: 0xab }, // #39 @ 0x36ce5
    DemodWrite { page: 0x01, reg: 0x03, value: 0x96 }, // #40 @ 0x36ce8
    DemodWrite { page: 0x01, reg: 0x04, value: 0x78 }, // #41 @ 0x36ceb
    DemodWrite { page: 0x01, reg: 0x07, value: 0x26 }, // #42 @ 0x36cee
    DemodWrite { page: 0x01, reg: 0x09, value: 0x1b }, // #43 @ 0x36cf1
    DemodWrite { page: 0x01, reg: 0x13, value: 0x02 }, // #44 @ 0x36cf4
    DemodWrite { page: 0x01, reg: 0x16, value: 0x9f }, // #45 @ 0x36cf7
    DemodWrite { page: 0x01, reg: 0x1c, value: 0x00 }, // #46 @ 0x36cfa
    DemodWrite { page: 0x01, reg: 0x23, value: 0x01 }, // #47 @ 0x36cfd
    DemodWrite { page: 0x01, reg: 0x24, value: 0x02 }, // #48 @ 0x36d00
    DemodWrite { page: 0x01, reg: 0x2c, value: 0x12 }, // #49 @ 0x36d03
    DemodWrite { page: 0x01, reg: 0x3e, value: 0x00 }, // #50 @ 0x36d06
    DemodWrite { page: 0x01, reg: 0x46, value: 0x68 }, // #51 @ 0x36d09
    DemodWrite { page: 0x01, reg: 0x50, value: 0x70 }, // #52 @ 0x36d0c
    DemodWrite { page: 0x01, reg: 0x54, value: 0x0c }, // #53 @ 0x36d0f
    DemodWrite { page: 0x01, reg: 0x55, value: 0xe0 }, // #54 @ 0x36d12
    DemodWrite { page: 0x01, reg: 0x61, value: 0x00 }, // #55 @ 0x36d15
    DemodWrite { page: 0x01, reg: 0x68, value: 0xf0 }, // #56 @ 0x36d18
    DemodWrite { page: 0x01, reg: 0x69, value: 0x04 }, // #57 @ 0x36d1b
    DemodWrite { page: 0x01, reg: 0x6a, value: 0x03 }, // #58 @ 0x36d1e
    DemodWrite { page: 0x01, reg: 0x7c, value: 0x02 }, // #59 @ 0x36d21
    DemodWrite { page: 0x01, reg: 0xe5, value: 0x00 }, // #60 @ 0x36d24
    // page 0x02 / slave7 0x12
    DemodWrite { page: 0x02, reg: 0x11, value: 0x12 }, // #61 @ 0x36d27
    // page 0x03 / slave7 0x13
    DemodWrite { page: 0x03, reg: 0x00, value: 0x50 }, // #62 @ 0x36d2a
    DemodWrite { page: 0x03, reg: 0x09, value: 0x0e }, // #63 @ 0x36d2d
    DemodWrite { page: 0x03, reg: 0x0b, value: 0x14 }, // #64 @ 0x36d30
    DemodWrite { page: 0x03, reg: 0x18, value: 0x00 }, // #65 @ 0x36d33
    DemodWrite { page: 0x03, reg: 0x19, value: 0x00 }, // #66 @ 0x36d36
    DemodWrite { page: 0x03, reg: 0x23, value: 0xc9 }, // #67 @ 0x36d39
    DemodWrite { page: 0x03, reg: 0x24, value: 0x02 }, // #68 @ 0x36d3c
    DemodWrite { page: 0x03, reg: 0x25, value: 0x30 }, // #69 @ 0x36d3f
    DemodWrite { page: 0x03, reg: 0x26, value: 0x69 }, // #70 @ 0x36d42
    DemodWrite { page: 0x03, reg: 0x27, value: 0x1f }, // #71 @ 0x36d45
    DemodWrite { page: 0x03, reg: 0x3d, value: 0xed }, // #72 @ 0x36d48
    DemodWrite { page: 0x03, reg: 0x3e, value: 0x96 }, // #73 @ 0x36d4b
    DemodWrite { page: 0x03, reg: 0x3f, value: 0x4a }, // #74 @ 0x36d4e
    DemodWrite { page: 0x03, reg: 0x42, value: 0xc8 }, // #75 @ 0x36d51
    DemodWrite { page: 0x03, reg: 0x43, value: 0x2d }, // #76 @ 0x36d54
    DemodWrite { page: 0x03, reg: 0x44, value: 0x48 }, // #77 @ 0x36d57
    DemodWrite { page: 0x03, reg: 0x49, value: 0x07 }, // #78 @ 0x36d5a
    DemodWrite { page: 0x03, reg: 0x4a, value: 0xf0 }, // #79 @ 0x36d5d
    DemodWrite { page: 0x03, reg: 0x58, value: 0x57 }, // #80 @ 0x36d60
    DemodWrite { page: 0x03, reg: 0x90, value: 0xed }, // #81 @ 0x36d63
    DemodWrite { page: 0x03, reg: 0x9d, value: 0x65 }, // #82 @ 0x36d66
    DemodWrite { page: 0x03, reg: 0xe1, value: 0x08 }, // #83 @ 0x36d69
    DemodWrite { page: 0x03, reg: 0xe2, value: 0xae }, // #84 @ 0x36d6c
    // page 0x04 / slave7 0x14
    DemodWrite { page: 0x04, reg: 0x00, value: 0xb0 }, // #85 @ 0x36d6f
    DemodWrite { page: 0x04, reg: 0x05, value: 0x60 }, // #86 @ 0x36d72
    DemodWrite { page: 0x04, reg: 0x06, value: 0x5f }, // #87 @ 0x36d75
    DemodWrite { page: 0x04, reg: 0x07, value: 0x27 }, // #88 @ 0x36d78
    DemodWrite { page: 0x04, reg: 0x08, value: 0xb0 }, // #89 @ 0x36d7b
    DemodWrite { page: 0x04, reg: 0x09, value: 0xff }, // #90 @ 0x36d7e
    DemodWrite { page: 0x04, reg: 0x0a, value: 0xc0 }, // #91 @ 0x36d81
    DemodWrite { page: 0x04, reg: 0x0b, value: 0xaa }, // #92 @ 0x36d84
    DemodWrite { page: 0x04, reg: 0x0c, value: 0xbb }, // #93 @ 0x36d87
    DemodWrite { page: 0x04, reg: 0x0d, value: 0xee }, // #94 @ 0x36d8a
    DemodWrite { page: 0x04, reg: 0x0e, value: 0xaa }, // #95 @ 0x36d8d
    DemodWrite { page: 0x04, reg: 0x0f, value: 0xaa }, // #96 @ 0x36d90
    DemodWrite { page: 0x04, reg: 0x10, value: 0x10 }, // #97 @ 0x36d93
    DemodWrite { page: 0x04, reg: 0x11, value: 0x62 }, // #98 @ 0x36d96
    DemodWrite { page: 0x04, reg: 0x12, value: 0x0d }, // #99 @ 0x36d99
    DemodWrite { page: 0x04, reg: 0x13, value: 0x77 }, // #100 @ 0x36d9c
    DemodWrite { page: 0x04, reg: 0x14, value: 0x1d }, // #101 @ 0x36d9f
    DemodWrite { page: 0x04, reg: 0x15, value: 0x1c }, // #102 @ 0x36da2
    DemodWrite { page: 0x04, reg: 0x21, value: 0x00 }, // #103 @ 0x36da5
    DemodWrite { page: 0x04, reg: 0x22, value: 0xa0 }, // #104 @ 0x36da8
    DemodWrite { page: 0x04, reg: 0x2e, value: 0x00 }, // #105 @ 0x36dab
    DemodWrite { page: 0x04, reg: 0x2f, value: 0x0c }, // #106 @ 0x36dae
    DemodWrite { page: 0x04, reg: 0x30, value: 0x3b }, // #107 @ 0x36db1
    DemodWrite { page: 0x04, reg: 0x32, value: 0x0c }, // #108 @ 0x36db4
    DemodWrite { page: 0x04, reg: 0x34, value: 0xb1 }, // #109 @ 0x36db7
    DemodWrite { page: 0x04, reg: 0x35, value: 0xed }, // #110 @ 0x36dba
    DemodWrite { page: 0x04, reg: 0x36, value: 0x60 }, // #111 @ 0x36dbd
    DemodWrite { page: 0x04, reg: 0x37, value: 0xcc }, // #112 @ 0x36dc0
    DemodWrite { page: 0x04, reg: 0x38, value: 0x6c }, // #113 @ 0x36dc3
    DemodWrite { page: 0x04, reg: 0x39, value: 0x7d }, // #114 @ 0x36dc6
    DemodWrite { page: 0x04, reg: 0x3a, value: 0xb1 }, // #115 @ 0x36dc9
    DemodWrite { page: 0x04, reg: 0x3b, value: 0xed }, // #116 @ 0x36dcc
    DemodWrite { page: 0x04, reg: 0x43, value: 0x97 }, // #117 @ 0x36dcf
    DemodWrite { page: 0x04, reg: 0x46, value: 0xa5 }, // #118 @ 0x36dd2
    DemodWrite { page: 0x04, reg: 0x47, value: 0x7e }, // #119 @ 0x36dd5
    DemodWrite { page: 0x04, reg: 0x48, value: 0x1d }, // #120 @ 0x36dd8
    DemodWrite { page: 0x04, reg: 0x49, value: 0xa5 }, // #121 @ 0x36ddb
    DemodWrite { page: 0x04, reg: 0x4a, value: 0x76 }, // #122 @ 0x36dde
    DemodWrite { page: 0x04, reg: 0x4b, value: 0xa5 }, // #123 @ 0x36de1
    DemodWrite { page: 0x04, reg: 0x4c, value: 0x76 }, // #124 @ 0x36de4
    DemodWrite { page: 0x04, reg: 0x4d, value: 0xa4 }, // #125 @ 0x36de7
    DemodWrite { page: 0x04, reg: 0x4e, value: 0x76 }, // #126 @ 0x36dea
    DemodWrite { page: 0x04, reg: 0x50, value: 0x55 }, // #127 @ 0x36ded
    DemodWrite { page: 0x04, reg: 0x53, value: 0x4f }, // #128 @ 0x36df0
    DemodWrite { page: 0x04, reg: 0x54, value: 0xc0 }, // #129 @ 0x36df3
    DemodWrite { page: 0x04, reg: 0x55, value: 0x73 }, // #130 @ 0x36df6
    DemodWrite { page: 0x04, reg: 0x62, value: 0x4b }, // #131 @ 0x36df9
    DemodWrite { page: 0x04, reg: 0x79, value: 0x12 }, // #132 @ 0x36dfc
    DemodWrite { page: 0x04, reg: 0x7a, value: 0xe2 }, // #133 @ 0x36dff
    DemodWrite { page: 0x04, reg: 0x7b, value: 0x00 }, // #134 @ 0x36e02
    DemodWrite { page: 0x04, reg: 0x81, value: 0x40 }, // #135 @ 0x36e05
    DemodWrite { page: 0x04, reg: 0xc9, value: 0x85 }, // #136 @ 0x36e08
    // page 0x05 / slave7 0x15
    DemodWrite { page: 0x05, reg: 0xe5, value: 0x00 }, // #137 @ 0x36e0b
    // page 0x07 / slave7 0x17
    DemodWrite { page: 0x07, reg: 0x02, value: 0x03 }, // #138 @ 0x36e0e
    DemodWrite { page: 0x07, reg: 0x1e, value: 0xc0 }, // #139 @ 0x36e11
    DemodWrite { page: 0x07, reg: 0x1f, value: 0xa8 }, // #140 @ 0x36e14
    DemodWrite { page: 0x07, reg: 0x20, value: 0x64 }, // #141 @ 0x36e17
    DemodWrite { page: 0x07, reg: 0x21, value: 0x64 }, // #142 @ 0x36e1a
    DemodWrite { page: 0x07, reg: 0x33, value: 0x10 }, // #143 @ 0x36e1d
    DemodWrite { page: 0x07, reg: 0x3b, value: 0x07 }, // #144 @ 0x36e20
    DemodWrite { page: 0x07, reg: 0x72, value: 0x00 }, // #145 @ 0x36e23
    DemodWrite { page: 0x07, reg: 0x74, value: 0x80 }, // #146 @ 0x36e26
    DemodWrite { page: 0x07, reg: 0x76, value: 0x80 }, // #147 @ 0x36e29
    DemodWrite { page: 0x07, reg: 0x8d, value: 0x00 }, // #148 @ 0x36e2c
    DemodWrite { page: 0x07, reg: 0x91, value: 0x17 }, // #149 @ 0x36e2f
    DemodWrite { page: 0x07, reg: 0xd3, value: 0x1f }, // #150 @ 0x36e32
    DemodWrite { page: 0x07, reg: 0xd4, value: 0xff }, // #151 @ 0x36e35
    // page 0x08 / slave7 0x18
    DemodWrite { page: 0x08, reg: 0x15, value: 0x01 }, // #152 @ 0x36e38
    DemodWrite { page: 0x08, reg: 0x16, value: 0x02 }, // #153 @ 0x36e3b
    DemodWrite { page: 0x08, reg: 0x1a, value: 0xc6 }, // #154 @ 0x36e3e
    DemodWrite { page: 0x08, reg: 0x1d, value: 0x6e }, // #155 @ 0x36e41
    DemodWrite { page: 0x08, reg: 0x1f, value: 0xa8 }, // #156 @ 0x36e44
    DemodWrite { page: 0x08, reg: 0x20, value: 0xf0 }, // #157 @ 0x36e47
    DemodWrite { page: 0x08, reg: 0x2f, value: 0x01 }, // #158 @ 0x36e4a
    DemodWrite { page: 0x08, reg: 0x43, value: 0x00 }, // #159 @ 0x36e4d
    DemodWrite { page: 0x08, reg: 0x4a, value: 0x02 }, // #160 @ 0x36e50
    DemodWrite { page: 0x08, reg: 0x4b, value: 0x02 }, // #161 @ 0x36e53
    DemodWrite { page: 0x08, reg: 0x4c, value: 0x02 }, // #162 @ 0x36e56
    DemodWrite { page: 0x08, reg: 0x4d, value: 0x02 }, // #163 @ 0x36e59
    DemodWrite { page: 0x08, reg: 0x4e, value: 0xbb }, // #164 @ 0x36e5c
    DemodWrite { page: 0x08, reg: 0x4f, value: 0xbb }, // #165 @ 0x36e5f
    DemodWrite { page: 0x08, reg: 0x50, value: 0xbb }, // #166 @ 0x36e62
    DemodWrite { page: 0x08, reg: 0x51, value: 0xbb }, // #167 @ 0x36e65
];

/// `regset_TLV_SPI_1BIT` — VA `0x3b3cc`, file `0x2b3cc`, 12 entries + `ff ff ff`.
pub const DEMOD_REGSET_TLV_SPI_1BIT: [DemodWrite; 12] = [
    // page 0x00 / slave7 0x10
    DemodWrite { page: 0x00, reg: 0xd9, value: 0x00 }, // #0 @ 0x3b3cc
    DemodWrite { page: 0x00, reg: 0x0b, value: 0x00 }, // #1 @ 0x3b3cf
    DemodWrite { page: 0x00, reg: 0x0c, value: 0x00 }, // #2 @ 0x3b3d2
    DemodWrite { page: 0x00, reg: 0x13, value: 0x04 }, // #3 @ 0x3b3d5
    DemodWrite { page: 0x00, reg: 0x16, value: 0x21 }, // #4 @ 0x3b3d8
    DemodWrite { page: 0x00, reg: 0x18, value: 0x00 }, // #5 @ 0x3b3db
    // page 0x07 / slave7 0x17
    DemodWrite { page: 0x07, reg: 0x0a, value: 0x00 }, // #6 @ 0x3b3de
    DemodWrite { page: 0x07, reg: 0x72, value: 0x02 }, // #7 @ 0x3b3e1
    DemodWrite { page: 0x07, reg: 0x73, value: 0x15 }, // #8 @ 0x3b3e4
    DemodWrite { page: 0x07, reg: 0x7b, value: 0x67 }, // #9 @ 0x3b3e7
    DemodWrite { page: 0x07, reg: 0x7d, value: 0x81 }, // #10 @ 0x3b3ea
    DemodWrite { page: 0x07, reg: 0x8d, value: 0x00 }, // #11 @ 0x3b3ed
];

/// `regset_Recommend_DIFF` — VA `0x38c30`, file `0x28c30`, 62 entries + `ff ff ff`.
pub const DEMOD_REGSET_RECOMMEND_DIFF: [DemodWrite; 62] = [
    // page 0x00 / slave7 0x10
    DemodWrite { page: 0x00, reg: 0x00, value: 0x94 }, // #0 @ 0x38c30
    DemodWrite { page: 0x00, reg: 0x01, value: 0x8c }, // #1 @ 0x38c33
    DemodWrite { page: 0x00, reg: 0x05, value: 0x52 }, // #2 @ 0x38c36
    DemodWrite { page: 0x00, reg: 0x07, value: 0x10 }, // #3 @ 0x38c39
    DemodWrite { page: 0x00, reg: 0x10, value: 0x00 }, // #4 @ 0x38c3c
    DemodWrite { page: 0x00, reg: 0x11, value: 0x00 }, // #5 @ 0x38c3f
    DemodWrite { page: 0x00, reg: 0x14, value: 0x08 }, // #6 @ 0x38c42
    DemodWrite { page: 0x00, reg: 0x16, value: 0x44 }, // #7 @ 0x38c45
    DemodWrite { page: 0x00, reg: 0x18, value: 0x80 }, // #8 @ 0x38c48
    DemodWrite { page: 0x00, reg: 0x1c, value: 0x01 }, // #9 @ 0x38c4b
    DemodWrite { page: 0x00, reg: 0x1d, value: 0x01 }, // #10 @ 0x38c4e
    DemodWrite { page: 0x00, reg: 0x85, value: 0x05 }, // #11 @ 0x38c51
    DemodWrite { page: 0x00, reg: 0x97, value: 0xe1 }, // #12 @ 0x38c54
    DemodWrite { page: 0x00, reg: 0xbc, value: 0x25 }, // #13 @ 0x38c57
    DemodWrite { page: 0x00, reg: 0xc6, value: 0x01 }, // #14 @ 0x38c5a
    DemodWrite { page: 0x00, reg: 0xcd, value: 0x41 }, // #15 @ 0x38c5d
    // page 0x01 / slave7 0x11
    DemodWrite { page: 0x01, reg: 0x77, value: 0x00 }, // #16 @ 0x38c60
    // page 0x05 / slave7 0x15
    DemodWrite { page: 0x05, reg: 0x02, value: 0x40 }, // #17 @ 0x38c63
    DemodWrite { page: 0x05, reg: 0x10, value: 0x15 }, // #18 @ 0x38c66
    DemodWrite { page: 0x05, reg: 0x2c, value: 0x9d }, // #19 @ 0x38c69
    DemodWrite { page: 0x05, reg: 0x2d, value: 0xff }, // #20 @ 0x38c6c
    DemodWrite { page: 0x05, reg: 0x2e, value: 0x02 }, // #21 @ 0x38c6f
    DemodWrite { page: 0x05, reg: 0x30, value: 0xcb }, // #22 @ 0x38c72
    DemodWrite { page: 0x05, reg: 0x34, value: 0x39 }, // #23 @ 0x38c75
    DemodWrite { page: 0x05, reg: 0x36, value: 0x67 }, // #24 @ 0x38c78
    DemodWrite { page: 0x05, reg: 0x37, value: 0x62 }, // #25 @ 0x38c7b
    DemodWrite { page: 0x05, reg: 0x3b, value: 0x30 }, // #26 @ 0x38c7e
    DemodWrite { page: 0x05, reg: 0x3c, value: 0x18 }, // #27 @ 0x38c81
    DemodWrite { page: 0x05, reg: 0x41, value: 0x37 }, // #28 @ 0x38c84
    DemodWrite { page: 0x05, reg: 0x43, value: 0x37 }, // #29 @ 0x38c87
    DemodWrite { page: 0x05, reg: 0x4d, value: 0xe4 }, // #30 @ 0x38c8a
    DemodWrite { page: 0x05, reg: 0x6e, value: 0x4c }, // #31 @ 0x38c8d
    DemodWrite { page: 0x05, reg: 0x70, value: 0xff }, // #32 @ 0x38c90
    DemodWrite { page: 0x05, reg: 0x75, value: 0xc9 }, // #33 @ 0x38c93
    DemodWrite { page: 0x05, reg: 0x77, value: 0x14 }, // #34 @ 0x38c96
    DemodWrite { page: 0x05, reg: 0x87, value: 0x69 }, // #35 @ 0x38c99
    DemodWrite { page: 0x05, reg: 0x88, value: 0x4a }, // #36 @ 0x38c9c
    DemodWrite { page: 0x05, reg: 0x89, value: 0x31 }, // #37 @ 0x38c9f
    DemodWrite { page: 0x05, reg: 0x8a, value: 0x24 }, // #38 @ 0x38ca2
    DemodWrite { page: 0x05, reg: 0x8b, value: 0x1d }, // #39 @ 0x38ca5
    DemodWrite { page: 0x05, reg: 0x8c, value: 0x1d }, // #40 @ 0x38ca8
    DemodWrite { page: 0x05, reg: 0x95, value: 0x10 }, // #41 @ 0x38cab
    DemodWrite { page: 0x05, reg: 0x96, value: 0x08 }, // #42 @ 0x38cae
    DemodWrite { page: 0x05, reg: 0x97, value: 0x0f }, // #43 @ 0x38cb1
    DemodWrite { page: 0x05, reg: 0xe5, value: 0x80 }, // #44 @ 0x38cb4
    // page 0x07 / slave7 0x17
    DemodWrite { page: 0x07, reg: 0x02, value: 0x03 }, // #45 @ 0x38cb7
    DemodWrite { page: 0x07, reg: 0x1e, value: 0xc0 }, // #46 @ 0x38cba
    DemodWrite { page: 0x07, reg: 0x1f, value: 0xa8 }, // #47 @ 0x38cbd
    DemodWrite { page: 0x07, reg: 0x20, value: 0x64 }, // #48 @ 0x38cc0
    DemodWrite { page: 0x07, reg: 0x21, value: 0x64 }, // #49 @ 0x38cc3
    DemodWrite { page: 0x07, reg: 0x32, value: 0x81 }, // #50 @ 0x38cc6
    DemodWrite { page: 0x07, reg: 0x33, value: 0x10 }, // #51 @ 0x38cc9
    DemodWrite { page: 0x07, reg: 0x3b, value: 0x07 }, // #52 @ 0x38ccc
    DemodWrite { page: 0x07, reg: 0x74, value: 0x80 }, // #53 @ 0x38ccf
    DemodWrite { page: 0x07, reg: 0x76, value: 0x80 }, // #54 @ 0x38cd2
    DemodWrite { page: 0x07, reg: 0x91, value: 0x17 }, // #55 @ 0x38cd5
    DemodWrite { page: 0x07, reg: 0x95, value: 0x18 }, // #56 @ 0x38cd8
    DemodWrite { page: 0x07, reg: 0x96, value: 0x19 }, // #57 @ 0x38cdb
    DemodWrite { page: 0x07, reg: 0x97, value: 0x2e }, // #58 @ 0x38cde
    DemodWrite { page: 0x07, reg: 0x98, value: 0x2f }, // #59 @ 0x38ce1
    DemodWrite { page: 0x07, reg: 0x99, value: 0x34 }, // #60 @ 0x38ce4
    DemodWrite { page: 0x07, reg: 0x9a, value: 0x35 }, // #61 @ 0x38ce7
];

// ---------------------------------------------------------------------------
// 参考: 2K 選局の上位順序（純データ。実 I/O なし）
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TuneStep {
    RfStandbyIsdbt,
    DemodSetTsid { tsid: u16, include_recommend: bool },
    RfTune { if_khz: u32 },
    DemodSeqStart,
}

/// `FUN_00014518` (tuner:0x14518) の 2K パスを順序だけで表したもの。
/// 実際の I2C 送信・sleep・セマフォ取得は Phase 1 実装側の責務。
pub fn isdbs_2k_tune_steps(tsid: u16, if_khz: u32, include_recommend: bool) -> Vec<TuneStep> {
    vec![
        TuneStep::RfStandbyIsdbt,
        TuneStep::DemodSetTsid { tsid, include_recommend },
        TuneStep::RfTune { if_khz },
        TuneStep::DemodSeqStart,
    ]
}

// ---------------------------------------------------------------------------
// UNCONFIRMED / Phase 1 で確定する項目（コードには値を持たせない）
// ---------------------------------------------------------------------------
//
// 1. `I2c_Write` / `I2c_Read`（libi2c.so）のアドレス規約は 8-bit と推定。
//    RF は 8-bit を直接、復調は 7-bit を `<<1` して渡す 2 箇所から判断（libi2c 本体は未取得）。
// 2. `eSysSleepTask` / `eStmExSetTimerAfter` の tick 分解能。
// 3. IF trim (`+0x25760` / `-0x12bb0`) が BS 2K パスで有効か。
// 4. `DAT_0003672c` の初期値・遷移（初回 set_system を決めるフラグ）。
// 5. page2 `0x92 = 0x3f` の機能名（SeqStart と推定）。
// 6. page2 `0x5e` bit5 が TMCC lock と TS lock の両方に使われる理由。
// 7. `regset_PSEQ_ISDBS3` の解釈（4K 側。2K では未使用）。
// 8. 4K/ADBS が `FUN_0001ec98` 統合関数と `FUN_00014518` のどちらを通るか。

#[cfg(test)]
mod compile_shape_tests {
    use super::*;

    #[test]
    fn rf_addresses_are_8bit_forms_of_7bit_units() {
        assert_eq!(rf_slave7(0), Some(0x60));
        assert_eq!(rf_slave7(1), Some(0x61));
        assert_eq!(rf_slave7(2), Some(0x62));
        assert_eq!(rf_unit_from_dev_2k(2), Some(0));
        assert_eq!(rf_unit_from_dev_s3(0x800), Some(2));
    }

    #[test]
    fn pll_bytes_match_extracted_formula() {
        // BS 朝日 0x01172748 → 1,049,480 kHz → 262,370 = 0x0400E2
        let if_khz = wire_bcd_to_if_khz(0x0117_2748).unwrap();
        assert_eq!(if_khz, 1_049_480);
        assert_eq!(rf_pll_bytes(if_khz), [0xe2, 0x00, 0x04, 0xff, 0x00, 0x01]);
    }

    #[test]
    fn tables_have_the_extracted_lengths() {
        assert_eq!(DEMOD_REGSET_ISDBS.len(), 168);
        assert_eq!(DEMOD_REGSET_TLV_SPI_1BIT.len(), 12);
        assert_eq!(DEMOD_REGSET_RECOMMEND_DIFF.len(), 62);
        assert_eq!(RF_POWER_ON_ISDBS.len(), 4);
        assert_eq!(RF_STANDBY_ISDBS.len(), 7);
        assert_eq!(RF_STANDBY_ISDBT.len(), 5);
        assert_eq!(rf_tune_isdbs_plan(1_049_480, false).len(), 15);
    }
}

// ---------------------------------------------------------------------------
// ADSB / xcdrv high-level layer (Phase 0 static analysis)
// ---------------------------------------------------------------------------
pub mod adsb {
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

}
