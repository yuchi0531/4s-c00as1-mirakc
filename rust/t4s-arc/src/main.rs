//! Bounded ARC / TSI probes for the 4S-C00AS1.
//!
//! This binary is intentionally separate from `t4s-probe`: it performs a
//! small number of explicitly requested, reversible experiments and prints
//! every raw driver return value.  It never loops, never retries on its own,
//! and never touches `abs`/`adsb` processes.

use std::ffi::c_void;
use std::fs::OpenOptions;
use std::io;
use std::os::unix::io::AsRawFd;
use std::time::Duration;

use t4s_core::sys;
use t4s_core::xcdrv::{self, XcDrv};

const CUST_MSG: i32 = 0x78;
const TSI_EX: i32 = 0x53;
const MMR_BASE: i32 = 0x19;
const REQ_BYTES: usize = 0x8d0;
const STATUS_OFF: usize = 0x8c8;

struct Dev {
    file: std::fs::File,
}

impl Dev {
    fn open() -> io::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(xcdrv::DEV_PATH)?;
        Ok(Self { file })
    }

    fn fd(&self) -> i32 {
        self.file.as_raw_fd()
    }

    /// Raw libc ioctl.  Returns `(rc, errno)` without hiding a negative
    /// driver status behind `io::Error`.
    fn ioctl(&self, cmd: i32, arg: *mut c_void) -> (i32, i32) {
        let rc = unsafe { sys::ioctl(self.fd(), cmd, arg) };
        let errno = if rc < 0 {
            io::Error::last_os_error().raw_os_error().unwrap_or(0)
        } else {
            0
        };
        (rc, errno)
    }

    fn mmr_base(&self) -> io::Result<u32> {
        let mut base = 0u32;
        let (rc, errno) = self.ioctl(MMR_BASE, &mut base as *mut u32 as *mut c_void);
        if rc < 0 {
            return Err(io::Error::from_raw_os_error(if errno != 0 { errno } else { rc }));
        }
        if base == 0 || base & 3 != 0 {
            return Err(io::Error::other(format!("invalid MMR mapping {base:#x}")));
        }
        Ok(base)
    }

    fn mmr_read32(&self, base: u32, offset: u32) -> u32 {
        unsafe { std::ptr::read_volatile((base + offset) as *const u32) }
    }
}

fn put_u32(buf: &mut [u8], off: usize, value: u32) {
    buf[off..off + 4].copy_from_slice(&value.to_le_bytes());
}

fn get_u32(buf: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]])
}

fn parse_u32(s: &str) -> Option<u32> {
    let t = s.trim();
    if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        u32::from_str_radix(h, 16).ok()
    } else {
        t.parse::<u32>().ok()
    }
}

fn print_mmrs(dev: &Dev, base: u32) {
    for off in [0x6000u32, 0x6008, 0x600c, 0x61f0] {
        let v = dev.mmr_read32(base, off);
        println!("  {off:#06x} = {v:#010x}");
    }
}

fn cmd_custmsg(args: &[String]) -> Result<(), String> {
    if args.len() < 3 {
        return Err("usage: t4s-arc custmsg <word0> <word1> <word2> [sub]".into());
    }
    let word0 = parse_u32(&args[0]).ok_or("word0 parse")?;
    let word1 = parse_u32(&args[1]).ok_or("word1 parse")?;
    let word2 = parse_u32(&args[2]).ok_or("word2 parse")?;
    let sub = args
        .get(3)
        .and_then(|v| parse_u32(v))
        .unwrap_or(0) as u16;

    let dev = Dev::open().map_err(|e| format!("open {}: {e}", xcdrv::DEV_PATH))?;

    // cmd 0x78 is not a blind passthrough: general_ctrl_cmd treats word2 as a
    // physical buffer address and word1 as its length, then flushes the
    // dcache over it.  word2=0 faults inside v7_flush_kern_dcache_area
    // (observed on the device).  When the caller passes 0, allocate a tiny
    // owned ring and substitute a valid physical range.
    if word2 == 0 {
        return Err(
            "cmd 0x78 requires a driver-owned physical buffer in word2; word2=0 \
             (and any userspace page tried so far) oopses in flush_dcache. Refusing to send."
                .into(),
        );
    }

    let mut req = vec![0u8; REQ_BYTES];
    put_u32(&mut req, 0, word0);
    put_u32(&mut req, 4, word1);
    put_u32(&mut req, 8, word2);

    println!(
        "custmsg word0={word0:#010x} word1={word1:#010x} word2={word2:#010x} sub={sub:#06x}"
    );
    println!("  buffer={REQ_BYTES:#x} status_off={STATUS_OFF:#x} (user copy-back is 12B only)");

    let commands = [((sub as i32) << 16) | CUST_MSG, CUST_MSG];
    let mut chosen = None;
    for (idx, cmd) in commands.iter().enumerate() {
        if idx == 1 && chosen.is_some() {
            println!("skip ioctl {cmd:#010x}: first form succeeded");
            continue;
        }
        let (rc, errno) = dev.ioctl(*cmd, req.as_mut_ptr() as *mut c_void);
        let status = if rc >= 0 {
            rc
        } else if errno != 0 {
            -errno
        } else {
            rc
        };
        println!("  ioctl {cmd:#010x}: rc={rc} errno={errno} status={status} ({status:#x})");
        println!(
            "  user req+{STATUS_OFF:#x} = {:#010x} (kernel status is returned by ioctl)",
            get_u32(&req, STATUS_OFF)
        );
        if rc >= 0 {
            chosen = Some(*cmd);
            break;
        }
        if commands[0] == commands[1] {
            break;
        }
    }


    match chosen {
        Some(cmd) => println!("custmsg accepted by {cmd:#010x}"),
        None => println!("custmsg: neither form returned success; nothing was accepted"),
    }
    Ok(())
}

fn cmd_tsi_read(args: &[String]) -> Result<(), String> {
    if !args.is_empty() {
        return Err("usage: t4s-arc tsi-read".into());
    }
    let dev = Dev::open().map_err(|e| format!("open {}: {e}", xcdrv::DEV_PATH))?;
    let base = dev.mmr_base().map_err(|e| format!("mmr base: {e}"))?;
    println!("tsi-read mmr_base={base:#x}");
    print_mmrs(&dev, base);
    Ok(())
}

/// Send one 44-byte config_ex request.  `physical` is the 10-word physical
/// description (its first word is the logical port; the rest is the config).
fn send_tsi_ex(dev: &Dev, physical: &[u32; 10], asi: u32) -> (i32, i32) {
    let mut req = [0u32; 11];
    req[..10].copy_from_slice(physical);
    req[10] = asi;
    println!("  ioctl 0x53 req={:08x?}", req);
    dev.ioctl(TSI_EX, req.as_mut_ptr() as *mut c_void)
}

fn cmd_tsi_ex(args: &[String]) -> Result<(), String> {
    if args.len() != 3 {
        return Err("usage: t4s-arc tsi-ex <logical_port> <packet> <asi_port>".into());
    }
    let port = parse_u32(&args[0]).ok_or("port parse")?;
    let packet = parse_u32(&args[1]).ok_or("packet parse")?;
    let asi = parse_u32(&args[2]).ok_or("asi parse")?;
    if port > 1 {
        return Err("logical port must be 0 or 1 (product wires only TSI0/TSI1)".into());
    }
    if asi > 1 {
        return Err("asi must be 0 or 1 (bounded one-step experiment)".into());
    }

    let drv = XcDrv::open_path(xcdrv::DEV_PATH).map_err(|e| format!("open xcdrv: {e}"))?;

    let saved_mmrs = [
        drv.mmr_read32(0x6000).map_err(|e| e.to_string())?,
        drv.mmr_read32(0x6008).map_err(|e| e.to_string())?,
        drv.mmr_read32(0x600c).map_err(|e| e.to_string())?,
        drv.mmr_read32(0x61f0).map_err(|e| e.to_string())?,
    ];

    println!("tsi-ex logical={port} packet={packet} asi={asi}");
    println!(
        "before: 0x6000={:#010x} 0x6008={:#010x} 0x600c={:#010x} 0x61f0={:#010x}",
        saved_mmrs[0], saved_mmrs[1], saved_mmrs[2], saved_mmrs[3]
    );

    if saved_mmrs[3] != 0x10 {
        return Err(format!(
            "0x61f0 is {:#x}, not the identity 0x10; refusing to write (product state is not the expected baseline)",
            saved_mmrs[3]
        ));
    }

    let snapshot = drv
        .snapshot_tsi_mapping()
        .map_err(|e| format!("snapshot: {e}"))?;
    let saved0 = snapshot.request(0).ok_or("no physical port0 request")?;
    let saved1 = snapshot.request(1).ok_or("no physical port1 request")?;

    let req = [
        port, packet, 0, 1, 0, 0, 1, 0, 1, 0, asi,
    ];
    let (rc, errno) = {
        let dev = Dev::open().map_err(|e| format!("open {}: {e}", xcdrv::DEV_PATH))?;
        let mut r = req;
        println!("  ioctl 0x53 req={:08x?}", r);
        dev.ioctl(TSI_EX, r.as_mut_ptr() as *mut c_void)
    };
    let status = if rc >= 0 { rc } else { -errno };
    println!("ioctl 0x53 rc={rc} errno={errno} status={status} ({status:#x})");

    std::thread::sleep(Duration::from_millis(150));

    let after = [
        drv.mmr_read32(0x6000).map_err(|e| e.to_string())?,
        drv.mmr_read32(0x6008).map_err(|e| e.to_string())?,
        drv.mmr_read32(0x600c).map_err(|e| e.to_string())?,
        drv.mmr_read32(0x61f0).map_err(|e| e.to_string())?,
    ];
    println!(
        "after:  0x6000={:#010x} 0x6008={:#010x} 0x600c={:#010x} 0x61f0={:#010x}",
        after[0], after[1], after[2], after[3]
    );

    // Rollback: two config_ex calls put both crossbar nibbles and both
    // physical ports back to the saved snapshot (0x53 port=1/asi=1,
    // then 0x53 port=0/asi=0).
    let dev = Dev::open().map_err(|e| format!("open {}: {e}", xcdrv::DEV_PATH))?;
    let (r1, e1) = send_tsi_ex(&dev, &saved1, 1);
    println!("  restore logical1/asi1 rc={r1} errno={e1}");
    let (r0, e0) = send_tsi_ex(&dev, &saved0, 0);
    println!("  restore logical0/asi0 rc={r0} errno={e0}");
    std::thread::sleep(Duration::from_millis(150));

    let restored = [
        drv.mmr_read32(0x6000).map_err(|e| e.to_string())?,
        drv.mmr_read32(0x6008).map_err(|e| e.to_string())?,
        drv.mmr_read32(0x600c).map_err(|e| e.to_string())?,
        drv.mmr_read32(0x61f0).map_err(|e| e.to_string())?,
    ];
    println!(
        "restored: 0x6000={:#010x} 0x6008={:#010x} 0x600c={:#010x} 0x61f0={:#010x}",
        restored[0], restored[1], restored[2], restored[3]
    );
    if restored != saved_mmrs || restored[3] != 0x10 {
        return Err(format!(
            "RESTORE FAILED: saved={:08x?} restored={:08x?}; reboot required",
            saved_mmrs, restored
        ));
    }
    println!("restore verified: all four MMRs exactly match the saved values");
    Ok(())
}

fn cmd_destroy_buffer(args: &[String]) -> Result<(), String> {
    if args.len() != 1 {
        return Err("usage: t4s-arc destroy-buffer <handle>".into());
    }
    let handle = parse_u32(&args[0]).ok_or("handle parse")?;
    let drv = XcDrv::open_path(xcdrv::DEV_PATH).map_err(|e| format!("open xcdrv: {e}"))?;
    match drv.force_reset_buffer(handle) {
        Ok(()) => println!("force_reset_buffer {handle:#x}: ok"),
        Err(e) => println!("force_reset_buffer {handle:#x}: {e}"),
    }
    drv.destroy_buffer(handle)
        .map_err(|e| format!("destroy_buffer {handle:#x}: {e}"))?;
    println!("destroy_buffer {handle:#x}: ok");
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = args.first() else {
        eprintln!("usage: t4s-arc <custmsg|tsi-read|tsi-ex|destroy-buffer> ...");
        std::process::exit(2);
    };
    let rest = &args[1..];
    let result = match cmd.as_str() {
        "custmsg" => cmd_custmsg(rest),
        "tsi-read" => cmd_tsi_read(rest),
        "tsi-ex" => cmd_tsi_ex(rest),
        "destroy-buffer" => cmd_destroy_buffer(rest),
        other => Err(format!("unknown command {other}")),
    };
    if let Err(e) = result {
        eprintln!("t4s-arc: {e}");
        std::process::exit(1);
    }
}
