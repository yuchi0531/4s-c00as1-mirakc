//! mirakc tuner command for the SHARP 4S-C00AS1.
//!
//! This binary only *tunes and extracts* the transport stream.  It never
//! touches the CAS: BS/CS 2K is emitted still scrambled and `t4s-cas`
//! (mirakc's decode-filter) descrambles it.  BS4K is already decrypted by the
//! product's hardware ACAS before its REMUX turns MMT into MPEG-TS, so it is
//! emitted clear.
//!
//! Usage from `config.yml`:
//!
//! ```text
//! t4s-tuner --type {{{channel_type}}} --channel {{{channel}}} --duration {{{duration}}} -
//! ```

use std::{
    io::{self, Write},
    process::ExitCode,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use t4s_core::{
    bs2k, channels, isdbs, remux4k, si, tuner, xcdrv,
    xcdrv::XcDrv,
    tssrc::TsSource,
};

static STOP: AtomicBool = AtomicBool::new(false);

extern "C" {
    fn signal(signum: i32, handler: usize) -> usize;
}

extern "C" fn on_signal(_: i32) {
    STOP.store(true, Ordering::SeqCst);
}

fn install_signal_handlers() {
    unsafe {
        signal(2, on_signal as *const () as usize); // SIGINT
        signal(15, on_signal as *const () as usize); // SIGTERM
    }
}

fn usage() -> ! {
    eprintln!(
        "usage: t4s-tuner --type BS|CS|BS4K --channel <value> [--duration <sec|->] [--device <path>] [--freq <kHz>] -"
    );
    std::process::exit(2)
}

struct Args {
    channel_type: String,
    channel: String,
    duration: Option<Duration>,
    device: String,
    freq_override: Option<u32>,
}

fn parse_args() -> Args {
    let mut it = std::env::args().skip(1);
    let mut channel_type = None;
    let mut channel = None;
    let mut duration = None;
    let mut device = xcdrv::DEV_PATH.to_string();
    let mut freq_override = None;
    while let Some(a) = it.next() {
        match a.as_str() {
            "--type" | "--channel-type" => channel_type = it.next(),
            "--channel" => channel = it.next(),
            "--duration" => {
                if let Some(v) = it.next() {
                    if v != "-" {
                        duration = v.parse::<u64>().ok().map(Duration::from_secs);
                    }
                }
            }
            "--device" => device = it.next().unwrap_or(device),
            "--freq" => freq_override = it.next().and_then(|v| v.parse().ok()),
            "-" => {}
            "--help" | "-h" => usage(),
            other => {
                eprintln!("t4s-tuner: unknown argument {other}");
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
        duration,
        device,
        freq_override,
    }
}

fn parse_stream_id(value: &str) -> Result<u16, String> {
    let v = value.trim();
    let parsed = if let Some(hex) = v.strip_prefix("0x").or_else(|| v.strip_prefix("0X")) {
        u16::from_str_radix(hex, 16)
    } else {
        v.parse::<u16>()
    };
    parsed.map_err(|_| format!("bad BS4K StreamID: {value}"))
}

fn main() -> ExitCode {
    install_signal_handlers();
    let args = parse_args();
    let deadline = args.duration.map(|d| Instant::now() + d);
    let result = match args.channel_type.to_ascii_uppercase().as_str() {
        "BS4K" => run_4k(&args, deadline),
        "BS" | "CS" => run_2k(&args, deadline),
        other => Err(format!("unsupported channel type: {other}")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("t4s-tuner: {e}");
            ExitCode::FAILURE
        }
    }
}

fn pump(mut source: impl TsSource, deadline: Option<Instant>) -> io::Result<()> {
    let mut out = io::stdout().lock();
    let mut buf = Vec::with_capacity(256 * 1024);
    loop {
        if STOP.load(Ordering::Relaxed) {
            return Ok(());
        }
        if deadline.is_some_and(|d| Instant::now() >= d) {
            return Ok(());
        }
        buf.clear();
        match source.read_into(&mut buf) {
            Ok(0) => std::thread::sleep(Duration::from_millis(5)),
            Ok(_) => {
                out.write_all(&buf)?;
                out.flush()?;
            }
            Err(e) => return Err(e),
        }
    }
}

fn open_driver(path: &str) -> Result<Arc<XcDrv>, String> {
    XcDrv::open_path(path)
        .map(Arc::new)
        .map_err(|e| format!("cannot open {path}: {e}"))
}

fn wait_for_decode_chain(drv: &Arc<XcDrv>) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(250));
        let Ok(live) = remux4k::discover(drv) else {
            continue;
        };
        if live.video_readers >= 1 && live.audio_readers >= 1 {
            std::thread::sleep(Duration::from_millis(500));
            return Ok(());
        }
    }
    Err("the product did not finish rebuilding its decode chain within 10 s".into())
}

fn send_change(service_id: u16, oid: u32, mode: u32) -> Result<(), String> {
    let (tsid, freq, bid) = channels::tuning_record(service_id)
        .ok_or_else(|| format!("service {service_id} is not in the advanced-BS table"))?;
    let t = tuner::Tuner::open_qid(tuner::ADSB_COMM_QID, tuner::MSGH_LIST)
        .map_err(|e| format!("tuner queue: {e}"))?;
    t.change_channel(&tuner::ChangeChannelRequest {
        oid,
        tune: mode,
        not_decode: 0,
        service_id,
        freq_khz: freq,
        tlv_stream_id: tsid,
        bid,
    })
    .map_err(|e| format!("change-channel request: {e}"))
}

/// Tune the product through its own channel-change message and confirm with
/// the SI files it publishes under /tmp/adsb.  This is the firmware-independent
/// path: it does not read fixed addresses inside `adsb`.
fn tune_4k(drv: &Arc<XcDrv>, service_id: u16) -> Result<si::LiveService, String> {
    let mut oid: u32 = 0x40;
    let live_now = si::live_service(si::ADSB_SI_DIR);
    if live_now.is_some_and(|l| l.service_id == service_id) && remux4k::discover(drv).is_ok() {
        wait_for_decode_chain(drv)?;
        return live_now.ok_or_else(|| "selection disappeared".to_string());
    }

    send_change(service_id, oid, tuner::TUNE_CHANNEL_CHANGE)?;
    let begin = Instant::now();
    let mut resumed = false;
    let mut last = String::from("SI never confirmed the requested service");
    while begin.elapsed() < Duration::from_secs(15) {
        std::thread::sleep(Duration::from_millis(250));
        if let Some(live) = si::live_service(si::ADSB_SI_DIR) {
            if live.service_id == service_id {
                match wait_for_decode_chain(drv) {
                    Ok(()) => {
                        eprintln!(
                            "t4s-tuner: confirmed service {} TLV {:#06x}",
                            live.service_id, live.tlv_stream_id
                        );
                        return Ok(live);
                    }
                    Err(e) => last = e,
                }
            }
        }
        if !resumed && begin.elapsed() > Duration::from_secs(5) && remux4k::discover(drv).is_err()
        {
            oid += 1;
            send_change(service_id, oid, tuner::TUNE_RESUME)?;
            resumed = true;
        }
    }
    Err(format!("4K selection not confirmed for {service_id}: {last}"))
}

fn run_4k(args: &Args, deadline: Option<Instant>) -> Result<(), String> {
    let stream_id = parse_stream_id(&args.channel)?;
    let (tsid, _freq, service_id) = channels::advbs_for_stream(stream_id)
        .ok_or_else(|| format!("unknown BS4K StreamID {stream_id}"))?;
    let drv = open_driver(&args.device)?;

    let live = tune_4k(&drv, service_id)?;
    eprintln!(
        "t4s-tuner: BS4K {stream_id} -> service {service_id}, TLV {:#06x}",
        live.tlv_stream_id
    );
    let no_decoder = remux4k::discover(&drv).is_ok_and(|es| {
        es.video_readers == 0 || es.audio_readers == 0
    });
    let cfg = remux4k::TapConfig {
        stream_id: tsid,
        service_id,
        allow_no_decoder: true,
        es_level_abort: if no_decoder { 101 } else { 48 },
        stall_abort_ms: 5000,
        ..remux4k::TapConfig::default()
    };
    let tap = match remux4k::Tap::open(Arc::clone(&drv), cfg) {
        Ok(tap) => tap,
        Err(first) => {
            eprintln!("t4s-tuner: REMUX open failed ({first}); retrying once");
            std::thread::sleep(Duration::from_millis(1000));
            wait_for_decode_chain(&drv)?;
            remux4k::Tap::open(Arc::clone(&drv), cfg).map_err(|e| format!("REMUX open: {e}"))?
        }
    };
    pump(tap, deadline).map_err(|e| format!("BS4K stream: {e}"))
}

fn run_2k(args: &Args, deadline: Option<Instant>) -> Result<(), String> {
    let (freq, tsid) = args
        .freq_override
        .map(|f| Ok((f, 0)))
        .unwrap_or_else(|| isdbs::resolve(&args.channel_type, &args.channel))?;
    let drv = open_driver(&args.device)?;
    let legacy = bs2k::legacy_frontend().unwrap_or(false);
    let dirty = Arc::new(AtomicBool::new(false));
    let is_cs = args.channel_type.eq_ignore_ascii_case("CS");
    let native_tsi = std::env::var("T4S_NATIVE_TSI").is_ok();
    // (physical TSI port, logical TSI port, use logical1->physical0 remap)
    let (physical_port, logical_port, remap) = if is_cs {
        // Product 2K route: physical TSI1, logical TSI0.  No crossbar write.
        (1, 0, false)
    } else if native_tsi {
        (0, 0, false)
    } else {
        (0, 1, true)
    };
    let check_tsid = !is_cs;
    let source = bs2k::Source::open(
        Arc::clone(&drv),
        freq,
        tsid,
        // 0 = raw.  CAS/MULTI2 belongs to t4s-cas, not to the tuner.
        0,
        check_tsid,
        physical_port,
        logical_port,
        remap,
        &AtomicBool::new(legacy),
        Arc::clone(&dirty),
    )
    .map_err(|e| format!("ISDB-S open: {e}"))?;
    eprintln!(
        "t4s-tuner: {} {} -> RF {freq} kHz TSID {:04X} (raw, descramble in t4s-cas)",
        args.channel_type, args.channel, tsid
    );
    let result = pump(source, deadline).map_err(|e| format!("ISDB-S stream: {e}"));
    if dirty.load(Ordering::SeqCst) {
        return Err("graph cleanup failed; refusing to restore the frontend".into());
    }
    // Hand the frontend back to the product so a following BS4K request can
    // acquire its ADBS package.
    if let Err(e) = bs2k::restore_frontend() {
        eprintln!("t4s-tuner: frontend restore warning: {e}");
    }
    result
}
