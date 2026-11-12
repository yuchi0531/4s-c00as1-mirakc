//! mirakc decode-filter for the SHARP 4S-C00AS1.
//!
//! BS/CS 2K arrives from `t4s-tuner` still MULTI2-scrambled.  This filter
//! tracks PAT/PMT/ECM, asks the product's own `card` daemon for the keys and
//! decrypts every program in the multiplex.  BS4K is already clear (the
//! product's ACAS decrypted it before the hardware REMUX), so it is copied
//! straight through.
//!
//! Usage from `config.yml`:
//!
//! ```text
//! t4s-cas --type {{{channel_type}}} --channel {{{channel}}}
//! ```

use std::{
    io::{self, Read, Write},
    process::ExitCode,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use t4s_core::{
    cas::NativeCard,
    descramble::Decoder,
    isdbs,
};

static STOP: AtomicBool = AtomicBool::new(false);

extern "C" {
    fn signal(signum: i32, handler: usize) -> usize;
}

extern "C" fn on_signal(_: i32) {
    STOP.store(true, Ordering::SeqCst);
}

fn usage() -> ! {
    eprintln!("usage: t4s-cas --type BS|CS|BS4K --channel <value> [--tsid <hex>]");
    std::process::exit(2)
}

struct Args {
    channel_type: String,
    channel: String,
    tsid: Option<u16>,
    verbose: bool,
}

fn parse_tsid(v: &str) -> Result<u16, String> {
    let v = v.trim();
    let r = if let Some(h) = v.strip_prefix("0x").or_else(|| v.strip_prefix("0X")) {
        u16::from_str_radix(h, 16)
    } else {
        v.parse::<u16>()
    };
    r.map_err(|_| format!("bad TSID: {v}"))
}

fn parse_args() -> Args {
    let mut it = std::env::args().skip(1);
    let mut channel_type = None;
    let mut channel = None;
    let mut tsid = None;
    let mut verbose = false;
    while let Some(a) = it.next() {
        match a.as_str() {
            "--type" | "--channel-type" => channel_type = it.next(),
            "--channel" => channel = it.next(),
            "--tsid" => tsid = it.next().and_then(|v| parse_tsid(&v).ok()),
            "--verbose" | "-v" => verbose = true,
            "--help" | "-h" => usage(),
            other => {
                eprintln!("t4s-cas: unknown argument {other}");
                usage()
            }
        }
    }
    let (Some(channel_type), Some(channel)) = (channel_type, channel) else {
        usage()
    };
    Args {
        channel_type,
        channel,
        tsid,
        verbose,
    }
}

fn main() -> ExitCode {
    unsafe {
        signal(2, on_signal as *const () as usize);
        signal(15, on_signal as *const () as usize);
    }
    let args = parse_args();
    let result = match args.channel_type.to_ascii_uppercase().as_str() {
        "BS4K" => passthrough(),
        "BS" | "CS" => descramble(&args),
        other => Err(format!("unsupported channel type: {other}")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("t4s-cas: {e}");
            ExitCode::FAILURE
        }
    }
}

fn passthrough() -> Result<(), String> {
    let mut input = io::stdin().lock();
    let mut output = io::stdout().lock();
    let mut buf = vec![0u8; 188 * 1024];
    loop {
        if STOP.load(Ordering::Relaxed) {
            return Ok(());
        }
        let n = input.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            return Ok(());
        }
        output.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        output.flush().ok();
    }
}

fn descramble(args: &Args) -> Result<(), String> {
    let (tsid, _learn) = match args.tsid {
        Some(t) => (t, t == 0),
        None => match args.channel_type.to_ascii_uppercase().as_str() {
            "BS" => {
                let (_, t) = isdbs::resolve("BS", &args.channel)?;
                (t, false)
            }
            _ => (0, true),
        },
    };
    let card = NativeCard::open().map_err(|e| format!("native card: {e}"))?;
    let (system, iv) = card
        .parameters()
        .map_err(|e| format!("card parameters: {e}"))?;
    let mut decoder = Decoder::new(card, system, iv, tsid)
        .all_services()
        .learn_tsid();
    let mut input = io::stdin().lock();
    let mut output = io::stdout().lock();
    let mut inbuf = vec![0u8; 188 * 1024];
    let mut outbuf = Vec::with_capacity(188 * 256);
    let mut last_report = Instant::now();
    loop {
        if STOP.load(Ordering::Relaxed) {
            break;
        }
        let n = input.read(&mut inbuf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        outbuf.clear();
        decoder
            .push(&inbuf[..n], &mut outbuf)
            .map_err(|e| format!("decode: {e}"))?;
        if !outbuf.is_empty() {
            output.write_all(&outbuf).map_err(|e| e.to_string())?;
            output.flush().ok();
        }
        if args.verbose && last_report.elapsed() > Duration::from_secs(10) {
            eprintln!(
                "t4s-cas: {} packets, {} decrypted, {} ECM updates, {} passthrough PIDs",
                decoder.packets,
                decoder.decrypted,
                decoder.updates,
                decoder.encrypted_passthrough.len()
            );
            last_report = Instant::now();
        }
    }
    if args.verbose {
        eprintln!(
            "t4s-cas: closed: {} packets, {} decrypted, {} ECM updates",
            decoder.packets, decoder.decrypted, decoder.updates
        );
    }
    Ok(())
}
