//! Read-only probe of `/dev/xcfb_disp`.
//!
//! Every `XCFB_CMD_DISP_*` request is issued as `ioctl(fd, 0x61000, arg)`
//! where the command enum lives in the first word of `arg`, not in the request
//! number.  The numbers below were recovered from `abs` (see
//! `docs/analysis/phase4-bsp4k.md` §12).  All HDMI sub-commands go through
//! either the generic wrapper `HDMI_SYS_XcfbDispCommand` at `abs:0x22dcc8` or
//! the direct builder in `abs:HDMI_SYS_TMDS_Enable` at `abs:0x22eb46`.  Both
//! send
//!
//! ```text
//! struct xcfb_disp_arg { u32 cmd; u32 token; u32 arg1; u32 arg2; }
//! ```
//!
//! `token` is the value the preceding `XCFB_CMD_DISP_HDMI_LOCK` (0x500) wrote
//! into its `token` field (`abs:0x22dd2a`-`0x22dd32` saves it, `abs:0x22dd38`
//! echoes it back); `arg1`/`arg2` are the command-specific in/out words
//! (`abs:0x22dd40`-`0x22dd5e`).
//!
//! `LOCK`/`UNLOCK` (0x500/0x501) are the display access mutex that brackets
//! every command, including the product's own reads.  They are not register or
//! HDMI-state writes.  This module issues only read sub-commands: no
//! WRITE_REG (0x521), SPD, HDCP, or SET_METADATA is ever sent.

use std::ffi::c_void;
use std::fs::OpenOptions;
use std::os::unix::io::AsRawFd;
use t4s_core::sys;

/// ioctl request number shared by every XCFB DISP command.
pub const XCFB_DISP_IOCTL: i32 = 0x61000;

// --- command enums (from abs, see §12 table) -----------------------------
const CMD_HDMI_LOCK: u32 = 0x500;
const CMD_HDMI_UNLOCK: u32 = 0x501;
/// `HDMI_SYS_CheckPlugState`: `arg1 = HPD` before combining.
const CMD_HDMI_CHECK_PLUG_STATE: u32 = 0x523;
const CMD_HDMI_CHECK_PLUG_STATE_EX: u32 = 0x524;
/// `HDMI_SYS_TMDS_Enable` read path: `arg1 = reg addr (in)`, `arg2 = value (out)`.
const CMD_HDMI_READ_REG: u32 = 0x520;
/// EDID block count (out: `arg1`).
const CMD_HDMI_EDID_GET_BLOCK_NUM: u32 = 0x50b;
/// EDID block read: `arg1 = block index (in)`, `arg2 = buffer ptr (in/out)`.
const CMD_HDMI_EDID_READ_BLOCK: u32 = 0x50c;

/// The 16-byte argument object every DISP ioctl takes.
#[repr(C)]
#[derive(Default, Clone, Copy)]
struct XcfbArg {
    cmd: u32,
    token: u32,
    arg1: u32,
    arg2: u32,
}

pub struct Xcfb {
    file: std::fs::File,
    token: u32,
}

impl Xcfb {
    /// Open `/dev/xcfb_disp` read-write.  The driver is opened with write
    /// intent even for read sub-commands (`abs:0x22d39c` uses `open64(path, 2)`),
    /// so O_RDWR is required here too — but no write command is ever sent.
    pub fn open(path: &str) -> std::io::Result<Self> {
        let file = OpenOptions::new().read(true).write(true).open(path)?;
        Ok(Xcfb { file, token: 0 })
    }

    fn call(&self, arg: &mut XcfbArg) -> std::io::Result<i32> {
        let fd = self.file.as_raw_fd();
        unsafe { sys::ioctl_ptr(fd, XCFB_DISP_IOCTL, arg as *mut XcfbArg as *mut c_void) }
    }

    /// Acquire the display mutex and remember the session token it returns.
    pub fn lock(&mut self) -> std::io::Result<u32> {
        let mut arg = XcfbArg { cmd: CMD_HDMI_LOCK, ..Default::default() };
        self.call(&mut arg)?;
        self.token = arg.token;
        Ok(arg.token)
    }

    /// Release the display mutex.
    pub fn unlock(&self) -> std::io::Result<()> {
        let mut arg = XcfbArg { cmd: CMD_HDMI_UNLOCK, token: self.token, ..Default::default() };
        self.call(&mut arg).map(|_| ())
    }

    /// Issue one command with the session token, returning `(arg1, arg2)`.
    fn command(&self, cmd: u32, arg1: u32, arg2: u32) -> std::io::Result<(u32, u32)> {
        let mut arg = XcfbArg { cmd, token: self.token, arg1, arg2 };
        self.call(&mut arg)?;
        Ok((arg.arg1, arg.arg2))
    }

    /// Read one HDMI register.  `abs:0x22ece6` builds `{0x520, token, addr, 0}`
    /// and reads the value back from `arg2`; `abs:0x22ed2c` logs
    /// `Read reg (addr, data) = (0x%04X, 0x%04X)`.
    pub fn read_reg(&self, addr: u32) -> std::io::Result<u32> {
        Ok(self.command(CMD_HDMI_READ_REG, addr, 0)?.1)
    }

    /// Cable/plug state.  `abs:0x22d8e0` issues 0x523 (result in `arg1`), then
    /// 0x524 (result in `arg1`), and forms `combined = a523 | (a524 << 1)` at
    /// `abs:0x22d924`.  The log then labels bit0 `HPD` and bit1 `RSEN`.
    ///
    /// Returns `(combined, hpd_from_0x523, rsen_from_0x524)` so the raw words
    /// stay visible even if the bit interpretation differs on this revision.
    pub fn plug_state(&self) -> std::io::Result<(u32, u32, u32)> {
        let (hpd, _) = self.command(CMD_HDMI_CHECK_PLUG_STATE, 0, 0)?;
        let (rsen, _) = self.command(CMD_HDMI_CHECK_PLUG_STATE_EX, 0, 0)?;
        Ok((hpd | (rsen << 1), hpd, rsen))
    }

    /// Number of 128-byte EDID blocks the sink reports (0 when none).
    pub fn edid_block_count(&self) -> std::io::Result<u32> {
        Ok(self.command(CMD_HDMI_EDID_GET_BLOCK_NUM, 0, 0)?.0)
    }

    /// Read one 128-byte EDID block.  `abs:0x2365f0` passes `arg1 = index`,
    /// `arg2 = &buffer`.
    pub fn edid_read_block(&self, index: u32) -> std::io::Result<[u8; 128]> {
        let mut buf = [0u8; 128];
        self.command(CMD_HDMI_EDID_READ_BLOCK, index, buf.as_mut_ptr() as u32)?;
        Ok(buf)
    }
}
