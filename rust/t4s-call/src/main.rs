//! Call abs:BhvTvAdsbTuneChangeChannel (0xc66a84) inside the running process.
//! Diagnostic only: ptrace, a temporary UDF breakpoint at the epilogue, and
//! a zeroed 92-byte tune struct in abs BSS.  A failure can wedge abs; reboot
//! recovers the box.
use std::ffi::c_void;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::io::AsRawFd;

type CInt = i32;
type CLong = i32;
#[repr(C)]
#[derive(Clone, Copy)]
struct Regs {
    uregs: [u32; 18],
}

extern "C" {
    fn ptrace(request: CInt, pid: CInt, addr: *mut c_void, data: *mut c_void) -> CLong;
    fn waitpid(pid: CInt, status: *mut CInt, options: CInt) -> CInt;
    fn kill(pid: CInt, sig: CInt) -> CInt;
}

const PTRACE_PEEKTEXT: CInt = 1;
const PTRACE_POKETEXT: CInt = 4;
const PTRACE_GETREGS: CInt = 12;
const PTRACE_SETREGS: CInt = 13;
const PTRACE_ATTACH: CInt = 16;
const PTRACE_DETACH: CInt = 17;
const PTRACE_CONT: CInt = 7;

const ENTRY: u32 = 0x00c66d50; // wrapper: BhvTvAdsbTuneChangeChannelByService
const RETURN_BP: u32 = 0x00c66d86;
const SCRATCH: u32 = 0x02030000;
const SCRATCH_LEN: usize = 92;

fn find_pid(name: &str) -> Option<i32> {
    for e in fs::read_dir("/proc").ok()? {
        let e = e.ok()?;
        let p = e.path();
        let comm = p.join("comm");
        if let Ok(s) = fs::read_to_string(comm) {
            if s.trim() == name {
                return e.file_name().to_str()?.parse().ok();
            }
        }
    }
    None
}

fn read_mem(pid: i32, addr: u32, len: usize) -> std::io::Result<Vec<u8>> {
    let mut f = File::open(format!("/proc/{pid}/mem"))?;
    f.seek(SeekFrom::Start(u64::from(addr)))?;
    let mut b = vec![0u8; len];
    f.read_exact(&mut b)?;
    Ok(b)
}
fn write_mem(pid: i32, addr: u32, data: &[u8]) -> std::io::Result<()> {
    let mut f = OpenOptions::new().write(true).open(format!("/proc/{pid}/mem"))?;
    f.seek(SeekFrom::Start(u64::from(addr)))?;
    f.write_all(data)
}
fn getregs(pid: i32) -> std::io::Result<Regs> {
    let mut r = Regs { uregs: [0; 18] };
    if unsafe { ptrace(PTRACE_GETREGS, pid, std::ptr::null_mut(), &mut r as *mut _ as *mut c_void) } < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(r)
}
fn setregs(pid: i32, r: &Regs) -> std::io::Result<()> {
    if unsafe { ptrace(PTRACE_SETREGS, pid, std::ptr::null_mut(), r as *const _ as *mut c_void) } < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

fn main() {
    let service: u32 = std::env::args().nth(1).and_then(|v| v.parse().ok()).unwrap_or(141);
    let mode: u32 = std::env::args().nth(2).and_then(|v| v.parse().ok()).unwrap_or(0);
    let pid = if let Some(t) = std::env::args().nth(3).and_then(|v| v.parse::<i32>().ok()) {
        t
    } else {
        match find_pid("abs") {
            Some(p) => p,
            None => { eprintln!("abs not found"); std::process::exit(1); }
        }
    };
    println!("abs pid={pid} service={service} mode={mode}");
    let arc = unsafe { ptrace(PTRACE_ATTACH, pid, std::ptr::null_mut(), std::ptr::null_mut()) };
    println!("attach_rc={arc}");
    if arc < 0 {
        eprintln!("attach: {}", std::io::Error::last_os_error());
        std::process::exit(1);
    }
    let mut st = 0;
    unsafe { waitpid(pid, &mut st, 0) };
    let old_regs = match getregs(pid) { Ok(r) => r, Err(e) => { eprintln!("getregs: {e}"); unsafe { ptrace(PTRACE_DETACH, pid, std::ptr::null_mut(), std::ptr::null_mut()); } return; } };
    println!("old_pc={:#x} old_sp={:#x}", old_regs.uregs[15], old_regs.uregs[13]);
    let old_bp = read_mem(pid, RETURN_BP, 4).unwrap_or_default();
    let old_scratch = read_mem(pid, SCRATCH, SCRATCH_LEN).unwrap_or_default();
    if let Err(e) = write_mem(pid, RETURN_BP, &[0x00, 0xde, 0x00, 0xde]) {
        eprintln!("patch bp: {e}");
    }
    if let Err(e) = write_mem(pid, SCRATCH, &[0; SCRATCH_LEN]) {
        eprintln!("zero scratch: {e}");
    }
    let mut r = old_regs;
    r.uregs[0] = service;
    r.uregs[1] = mode;
    r.uregs[2] = SCRATCH;
    r.uregs[15] = ENTRY | 1; // Thumb
    if let Err(e) = setregs(pid, &r) {
        eprintln!("setregs: {e}");
    } else {
        println!("pc_set={:#x} sp={:#x}", r.uregs[15], r.uregs[13]);
        let rc = unsafe { ptrace(PTRACE_CONT, pid, std::ptr::null_mut(), std::ptr::null_mut()) };
        println!("cont_rc={rc}");
        let mut hit = false;
        for _ in 0..50 {
            let mut s2 = 0;
            let w = unsafe { waitpid(pid, &mut s2, 1) }; // WNOHANG=1
            if w == pid {
                hit = true;
                println!("stopped status={s2:#x}");
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        if !hit { println!("no breakpoint within 5s"); unsafe { kill(pid, 19); waitpid(pid, &mut st, 0); } }
    }
    // restore
    if old_bp.len() == 4 { let _ = write_mem(pid, RETURN_BP, &old_bp); }
    if old_scratch.len() == SCRATCH_LEN { let _ = write_mem(pid, SCRATCH, &old_scratch); }
    let _ = setregs(pid, &old_regs);
    unsafe { ptrace(PTRACE_DETACH, pid, std::ptr::null_mut(), std::ptr::null_mut()); }
    unsafe { kill(pid, 18); } // SIGCONT: never leave abs stopped
    println!("done");
}
