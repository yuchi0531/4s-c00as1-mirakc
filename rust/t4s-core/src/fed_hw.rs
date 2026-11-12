//! Phase 0 skeleton for a self-contained satellite FED layer.
//!
//! **Nothing in this module is executed yet.**  The tables below are
//! placeholders until the static analysis of the OEM `tuner` / `libvixs.so`
//! and `abs` / `xcdrv.ko` pairs is complete for *this* firmware build.
//!
//! The reference firmware used by Karin-Laboratory/mkra_4S-C00AS1 has a
//! different hash set:
//!
//! | file | reference md5 | this unit |
//! |---|---|---|
//! | `/usr/bin/tuner` | `0416f546e87bd2b0c7a2e764ea0ee292` | `8d2ef1dc9bb8af7f92efefdd152e9362` |
//! | `/usr/bin/abs` | `def4436239553ede33f0febf86eab318` | `9fc3e99e628ba2893d3192d0319153e8` |
//! | `/disk/xc6drv/xcdrv.ko` | `3db75958a29ecbb880c5a658584556d3` | `25bc580fb594958c9a7de4d475a4061d` |
//! | `/usr/vixs/lib/libvixs.so` | `27ed7eda99d63977a5989b9bdba7e571` | `80f28d4a4b1242d9a451d4ea6427a527` |
//!
//! Therefore fixed addresses from the reference analysis must not be trusted.
//! The tables will be filled from `docs/analysis/phase0-*.md` with per-entry
//! evidence (Ghidra function address, assembly address, string xref).

use std::io;

/// 7-bit I2C address of the satellite RF IC (Sony CXD2868ER) on `/dev/i2c-0`.
pub const RF_I2C_ADDR: u8 = 0x60;

/// 7-bit I2C address of the SC1501A demodulator (page selected separately).
pub const DEMOD_I2C_ADDR: u8 = 0x10;

/// A single register write in a recovered sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Write {
    pub addr: u8,
    /// SC1501A page/bank, if the target is the demodulator.
    pub page: Option<u8>,
    pub reg: u8,
    pub value: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Write(Write),
    DelayMs(u32),
    /// Poll `reg` until `(value & mask) == expected`, up to `timeout_ms`.
    Poll {
        addr: u8,
        page: Option<u8>,
        reg: u8,
        mask: u8,
        expected: u8,
        timeout_ms: u32,
        interval_ms: u32,
    },
}

/// Minimal bus abstraction.  The ARM implementation will wrap `/dev/i2c-0`
/// with the same attach/semaphore discipline as `gr_i2c.rs`.
pub trait I2cBus {
    fn read(&mut self, addr: u8, reg: u8) -> io::Result<u8>;
    fn write(&mut self, addr: u8, reg: u8, value: u8) -> io::Result<()>;
    fn set_page(&mut self, addr: u8, page: u8) -> io::Result<()>;
}

/// Placeholder: recovered RF standby/power sequence.
///
/// UNCONFIRMED.  Do not execute.  Fill from `phase0-tuner.md` once each write
/// has an assembly/function citation.
pub const RF_STANDBY: &[Step] = &[];

/// Placeholder: recovered RF power-on sequence.  UNCONFIRMED.
pub const RF_POWER_ON: &[Step] = &[];

/// Placeholder: recovered demodulator setup for ISDB-S3 / TLV.  UNCONFIRMED.
pub const DEMOD_TLV_SETUP: &[Step] = &[];

/// Placeholder: recovered TSID / frequency / SeqStart ordering.  UNCONFIRMED.
pub const TSID_FREQ_START: &[Step] = &[];

/// Apply a sequence through the supplied bus.  No caller exists in Phase 0.
pub fn apply(bus: &mut dyn I2cBus, seq: &[Step]) -> io::Result<()> {
    for step in seq {
        match *step {
            Step::Write(w) => {
                if let Some(page) = w.page {
                    bus.set_page(w.addr, page)?;
                }
                bus.write(w.addr, w.reg, w.value)?;
            }
            Step::DelayMs(ms) => std::thread::sleep(std::time::Duration::from_millis(u64::from(ms))),
            Step::Poll {
                addr,
                page,
                reg,
                mask,
                expected,
                timeout_ms,
                interval_ms,
            } => {
                if let Some(page) = page {
                    bus.set_page(addr, page)?;
                }
                let deadline = std::time::Instant::now()
                    + std::time::Duration::from_millis(u64::from(timeout_ms));
                loop {
                    if bus.read(addr, reg)? & mask == expected {
                        break;
                    }
                    if std::time::Instant::now() >= deadline {
                        return Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            "FED poll timed out",
                        ));
                    }
                    std::thread::sleep(std::time::Duration::from_millis(u64::from(interval_ms)));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_placeholder_sequences_apply_as_noops() {
        struct Null;
        impl I2cBus for Null {
            fn read(&mut self, _: u8, _: u8) -> io::Result<u8> {
                Ok(0)
            }
            fn write(&mut self, _: u8, _: u8, _: u8) -> io::Result<()> {
                Ok(())
            }
            fn set_page(&mut self, _: u8, _: u8) -> io::Result<()> {
                Ok(())
            }
        }
        apply(&mut Null, RF_STANDBY).unwrap();
        apply(&mut Null, RF_POWER_ON).unwrap();
        apply(&mut Null, DEMOD_TLV_SETUP).unwrap();
        apply(&mut Null, TSID_FREQ_START).unwrap();
    }
}
