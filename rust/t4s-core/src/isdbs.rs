//! Ordinary BS/CS 2K (ISDB-S) channel names → tune parameters.
//!
//! Mirakurun style: `BS15_0` is transponder 15, relative TS 0.  The TSID table
//! is the one measured by yuchi0531/pix-smb400-mirakc (`scripts/smb400-tuner.sh`,
//! derived from live NIT and recpt1/recisdb measurements).  `CS2` … `CS24` are
//! the even 110°CS transponders; their TSID is learned from the PAT by the
//! decoder, so the tuner passes 0.
//!
//! Frequencies are the actual RF downlink (LNB LO 10,678,000 kHz added to the
//! IF plan), which is what `tuner::rf_to_bcd10` expects for the non-4K FED.

pub const LNB_LO_KHZ: u32 = 10_678_000;

/// BS IF plan: BS-1 = 1,049,480 kHz, 38,360 kHz per transponder step.
fn bs_if_khz(transponder: u16) -> u32 {
    1_049_480 + u32::from(transponder - 1) / 2 * 38_360
}

/// 110°CS IF plan: CS-2 = 1,613,000 kHz, 40,000 kHz per step.
fn cs_if_khz(transponder: u16) -> u32 {
    1_613_000 + u32::from(transponder - 2) / 2 * 40_000
}

fn bs_tsid(channel: &str) -> Option<u16> {
    Some(match channel {
        "BS01_0" => 16400,
        "BS01_1" => 16401,
        "BS01_2" => 16402,
        "BS01_3" => 16400,
        "BS03_0" => 16432,
        "BS03_1" => 17969,
        "BS03_2" => 17970,
        "BS03_3" => 16432,
        "BS05_0" => 17488,
        "BS05_1" => 17489,
        "BS05_2" => 17488,
        "BS05_3" => 17488,
        "BS09_0" => 16528,
        "BS09_1" => 16530,
        "BS09_2" => 16528,
        "BS09_3" => 16528,
        "BS13_0" => 16592,
        "BS13_1" => 16593,
        "BS13_2" => 18130,
        "BS13_3" => 16592,
        "BS15_0" => 16625,
        "BS15_1" => 16626,
        "BS15_2" => 18675,
        "BS19_0" => 18224,
        "BS19_1" => 18225,
        "BS19_2" => 18226,
        "BS19_3" => 18227,
        "BS21_0" => 18256,
        "BS21_1" => 18257,
        "BS21_2" => 18258,
        "BS21_3" => 18256,
        "BS23_0" => 18288,
        "BS23_1" => 18801,
        "BS23_2" => 18803,
        "BS23_3" => 18288,
        _ => return None,
    })
}

/// 110°CS TSIDs measured from a remote Mirakurun/mirakc PAT (2026-11-13).
/// The remote server was used only as a TSID data source; no stream is retained.
fn cs_tsid(transponder: u16) -> Option<u16> {
    Some(match transponder {
        2 => 0x6020,
        4 => 0x7040,
        6 => 0x7060,
        8 => 0x6080,
        10 => 0x60a0,
        12 => 0x70c0,
        14 => 0x70e0,
        16 => 0x7100,
        18 => 0x7120,
        20 => 0x7140,
        22 => 0x7160,
        24 => 0x7180,
        _ => return None,
    })
}

/// Resolve a `BS`/`CS` channel string to `(RF kHz, TSID)`.  `TSID == 0` means
/// "learn it from the PAT" (CS has no per-relative-TS table).
pub fn resolve(channel_type: &str, channel: &str) -> Result<(u32, u16), String> {
    let upper = channel_type.to_ascii_uppercase();
    match upper.as_str() {
        "BS" => {
            let rest = channel
                .strip_prefix("BS")
                .ok_or_else(|| format!("BS channel must look like BS15_0, got {channel}"))?;
            let (tp, rel) = rest
                .split_once('_')
                .unwrap_or((rest, "0"));
            let tp: u16 = tp
                .parse()
                .map_err(|_| format!("bad BS transponder in {channel}"))?;
            let rel: u8 = rel
                .parse()
                .map_err(|_| format!("bad BS relative TS in {channel}"))?;
            if tp == 0 || tp > 23 || tp % 2 == 0 || rel > 3 {
                return Err(format!("BS channel out of range: {channel}"));
            }
            let rf = bs_if_khz(tp) + LNB_LO_KHZ;
            let tsid = bs_tsid(&format!("BS{tp:02}_{rel}"))
                .ok_or_else(|| format!("unknown BS channel {channel} (not in TSID table)"))?;
            Ok((rf, tsid))
        }
        "CS" => {
            let rest = channel
                .strip_prefix("CS")
                .or_else(|| channel.strip_prefix("ND"))
                .ok_or_else(|| format!("CS channel must look like CS2 or ND02, got {channel}"))?;
            let n: u16 = rest
                .parse()
                .map_err(|_| format!("bad CS transponder in {channel}"))?;
            if n < 2 || n > 24 || n % 2 != 0 {
                return Err(format!("CS channel out of range (even 2..24): {channel}"));
            }
            let tsid = cs_tsid(n)
                .ok_or_else(|| format!("unknown CS channel {channel}"))?;
            Ok((cs_if_khz(n) + LNB_LO_KHZ, tsid))
        }
        _ => Err(format!("{channel_type} is not an ISDB-S channel type")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bs_and_cs_match_the_documented_plan() {
        assert_eq!(resolve("BS", "BS15_0"), Ok((11_996_000, 16625)));
        assert_eq!(resolve("BS", "BS01_0"), Ok((11_727_480, 16400)));
        assert_eq!(resolve("CS", "CS2"), Ok((12_291_000, 0x6020)));
        assert_eq!(resolve("CS", "ND24"), Ok((12_731_000, 0x7180)));
        assert!(resolve("BS", "BS02_0").is_err());
        assert!(resolve("BS", "BS15_9").is_err());
        assert!(resolve("CS", "CS3").is_err());
    }
}
