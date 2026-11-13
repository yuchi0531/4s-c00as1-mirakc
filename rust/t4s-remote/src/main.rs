//! `t4s-remote` — a LAN remote-control web UI for the SHARP 4S-C00AS1.
//!
//! The product's UI task takes remote-control keys off the Q_UIM System V
//! queue (qid 0x7F, message 0x2044, 8-byte body `{u32 keyCode, u32 type}`;
//! type 0 = press, type 2 = release).  This server posts those messages with
//! `t4s_core::tuner::send_raw_message` and serves a remote-control page over
//! plain HTTP.
//!
//! UIM drops a key that equals the previous one (its repeat filter), and the
//! previous key is 0 right after boot.  Every press therefore goes through a
//! primer when needed: `{0x000, type 2}` — or `{0x025, type 2}` when the key
//! itself is 0x000 — followed by the press and, 80 ms later, the release.
//!
//! There is no authentication.  Bind this to a trusted LAN only.

use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use t4s_core::{gr_i2c, si, tuner};

mod json;
mod keys;

/// Q_UIM: the product UI task's inbound queue.
const Q_UIM: u32 = 0x7f;
/// "Key event" message id on Q_UIM (the same path the front panel uses).
const MSG_KEY: u32 = 0x2044;
const MSGH_LIST: &str = "/tmp/bdh_out_msgh.lst";
const LOG_PATH: &str = "/tmp/t4s-remote.log";
const PID_PATH: &str = "/tmp/t4s-remote.pid";
const PRESS_HOLD: Duration = Duration::from_millis(80);
const MAX_BODY: usize = 16 * 1024;
const MAX_STEPS: usize = 64;
const MAX_STEP_DELAY_MS: u64 = 5_000;
const MAX_MACRO_MS: u64 = 30_000;
const JSON_TYPE: &str = "application/json; charset=utf-8";
const INDEX_HTML: &str = include_str!("index.html");

#[derive(Clone, Copy)]
struct Event {
    epoch: u64,
    code: u32,
    kind: u32,
}

#[derive(Default)]
struct SendState {
    /// The key code UIM currently holds as "previous".  A press of the same
    /// code would be discarded, so it needs a primer first.
    last_press: Option<u32>,
    last_event: Option<Event>,
}

struct State {
    send: Mutex<SendState>,
}

impl State {
    fn new() -> Self {
        State {
            send: Mutex::new(SendState::default()),
        }
    }
}

/// What one key injection actually put on the wire.
struct Sent {
    code: u32,
    kind: u32,
    release: bool,
    primer: Option<u32>,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// `HH:MM:SS JST` without pulling in a time-zone crate.  The box runs JST.
fn hms(epoch: u64) -> String {
    let s = epoch + 9 * 3600;
    format!(
        "{:02}:{:02}:{:02} JST",
        (s / 3600) % 24,
        (s / 60) % 60,
        s % 60
    )
}

static LOG_LOCK: Mutex<()> = Mutex::new(());

/// Append one line to stdout and to `/tmp/t4s-remote.log`.
///
/// In `--daemon` mode stdout *is* the log file, so the file append is skipped
/// to avoid every line appearing twice.
fn log(msg: &str) {
    let line = format!("[{}] {msg}\n", hms(now_secs()));
    let mut out = io::stdout();
    let _ = out.write_all(line.as_bytes());
    let _ = out.flush();

    let _guard = LOG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let stdout_is_log = std::fs::read_link("/proc/self/fd/1")
        .map(|path| path == std::path::Path::new(LOG_PATH))
        .unwrap_or(false);
    if stdout_is_log {
        return;
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(LOG_PATH)
    {
        let _ = file.write_all(line.as_bytes());
    }
}

/// Detach from the controlling terminal and keep running after telnet closes.
///
/// The busybox `setsid` applet on this box does not survive the session closing
/// (`setsid sleep 300 &` is gone as soon as the prompt returns), so the binary
/// daemonizes itself: fork, new session, stdout/stderr → log file.
fn daemonize(log_path: &str) -> io::Result<()> {
    extern "C" {
        fn fork() -> i32;
        fn setsid() -> i32;
        fn dup2(old_fd: i32, new_fd: i32) -> i32;
    }

    // SAFETY: fork/setsid/dup2 are plain syscalls; between fork and the
    // reopen the child only touches the async-signal-safe subset.
    unsafe {
        let pid = fork();
        if pid < 0 {
            return Err(io::Error::last_os_error());
        }
        if pid > 0 {
            // The telnet shell gets its prompt back right away.
            std::process::exit(0);
        }
        // After fork the child is not a process-group leader, so this succeeds.
        let _ = setsid();

        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)?;
        let fd = std::os::unix::io::AsRawFd::as_raw_fd(&file);
        dup2(fd, 1);
        dup2(fd, 2);
        // `file` must stay open: closing it would close fd 1/2 as well.
        std::mem::forget(file);
    }
    Ok(())
}

fn raw(code: u32, kind: u32) -> io::Result<()> {
    tuner::send_raw_message(Q_UIM, MSG_KEY, 8, &[code, kind])
}

/// Send one key: primer (when needed), press, then release after 80 ms.
fn inject(state: &State, code: u32, kind: u32, release: bool) -> io::Result<Sent> {
    let mut send = state.send.lock().unwrap_or_else(|e| e.into_inner());
    let mut report = Sent {
        code,
        kind,
        release,
        primer: None,
    };

    if kind == 2 {
        raw(code, 2)?;
        send.last_press = Some(code);
        send.last_event = Some(Event {
            epoch: now_secs(),
            code,
            kind: 2,
        });
        log(&format!(
            "key {code:#05x} {} type=2 (release only)",
            keys::name(code)
        ));
        return Ok(report);
    }

    let primer = match send.last_press {
        Some(prev) if prev != code => None,
        // prev == code (repeat would be dropped) or no key sent yet (prev = 0
        // in a freshly booted UIM).  0x025 for code 0 so the primer is not the
        // same value the press is about to carry.
        _ => Some(if code == 0 { 0x025 } else { 0x000 }),
    };
    if let Some(p) = primer {
        raw(p, 2)?;
        report.primer = Some(p);
        log(&format!("primer {p:#05x} type=2 (UIM repeat filter)"));
    }

    raw(code, 0)?;
    log(&format!(
        "key {code:#05x} {} type=0{}",
        keys::name(code),
        if release { "" } else { " (hold, no release)" }
    ));
    if release {
        std::thread::sleep(PRESS_HOLD);
        raw(code, 2)?;
    }
    send.last_press = Some(code);
    send.last_event = Some(Event {
        epoch: now_secs(),
        code,
        kind,
    });
    Ok(report)
}

struct Step {
    code: u32,
    kind: u32,
    release: bool,
    delay_ms: u64,
}

fn run_macro(state: &State, steps: &[Step]) -> Result<(), String> {
    for (i, step) in steps.iter().enumerate() {
        inject(state, step.code, step.kind, step.release)
            .map_err(|e| format!("step {} ({}): {e}", i + 1, keys::name(step.code)))?;
        if step.delay_ms > 0 {
            std::thread::sleep(Duration::from_millis(step.delay_ms));
        }
    }
    Ok(())
}

struct Request {
    method: String,
    path: String,
    body: Vec<u8>,
}

fn read_request(stream: &mut TcpStream) -> io::Result<Request> {
    let mut buf = Vec::with_capacity(4096);
    let mut chunk = [0u8; 2048];
    let head_end = loop {
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "connection closed",
            ));
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos + 4;
        }
        if buf.len() > MAX_BODY + 8192 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "request header too large",
            ));
        }
    };

    let head = std::str::from_utf8(&buf[..head_end])
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "request head is not UTF-8"))?;
    let mut lines = head.split("\r\n");
    let request_line = lines.next().unwrap_or("");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_ascii_uppercase();
    let target = parts.next().unwrap_or("");
    if method.is_empty() || target.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "malformed request line",
        ));
    }
    let path = target.split('?').next().unwrap_or("").to_string();

    let mut length = 0usize;
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                length = value.trim().parse().map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidData, "bad Content-Length")
                })?;
            }
        }
    }
    if length > MAX_BODY {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "request body too large",
        ));
    }
    while buf.len() < head_end + length {
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "truncated request body",
            ));
        }
        buf.extend_from_slice(&chunk[..n]);
    }

    Ok(Request {
        method,
        path,
        body: buf[head_end..head_end + length].to_vec(),
    })
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Internal Server Error",
    }
}

fn respond(stream: &mut TcpStream, status: u16, ctype: &str, body: &[u8]) {
    let head = format!(
        "HTTP/1.1 {status} {}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\n\
         Cache-Control: no-store\r\nConnection: close\r\n\r\n",
        reason(status),
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
    let _ = stream.flush();
}

fn json_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn error_json(message: &str) -> String {
    format!("{{\"ok\":false,\"error\":\"{}\"}}\n", json_escape(message))
}

fn route(state: &State, request: &Request) -> (u16, &'static str, Vec<u8>) {
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/") | ("GET", "/index.html") => (
            200,
            "text/html; charset=utf-8",
            INDEX_HTML.as_bytes().to_vec(),
        ),
        ("GET", "/api/keys") => (200, JSON_TYPE, keys_json().into_bytes()),
        ("GET", "/api/status") => (200, JSON_TYPE, status_json(state).into_bytes()),
        ("POST", "/api/key") => handle_key(state, &request.body),
        ("POST", "/api/macro") => handle_macro(state, &request.body),
        ("GET", "/favicon.ico") => (204, "text/plain", Vec::new()),
        (_, path) if path.starts_with("/api/") => (
            404,
            JSON_TYPE,
            error_json("unknown API endpoint").into_bytes(),
        ),
        (method, path) if method != "GET" && path == "/" => {
            (405, JSON_TYPE, error_json("GET only").into_bytes())
        }
        _ => (404, "text/plain; charset=utf-8", b"not found\n".to_vec()),
    }
}

fn keys_json() -> String {
    let mut out = String::with_capacity(keys::KEY_NAMES.len() * 40 + 4);
    out.push('[');
    for (code, name) in keys::KEY_NAMES.iter().enumerate() {
        if code > 0 {
            out.push(',');
        }
        out.push_str(&format!("{{\"code\":{code},\"name\":\"{name}\"}}"));
    }
    out.push_str("]\n");
    out
}

fn key_code(value: &json::Value) -> Result<u32, String> {
    if let Some(code) = value.get("code").and_then(json::Value::as_u32) {
        if code > 0x13d {
            return Err(format!("key code {code:#x} is outside 0x000-0x13d"));
        }
        return Ok(code);
    }
    if let Some(name) = value.get("name").and_then(json::Value::as_str) {
        return keys::resolve(name).ok_or_else(|| format!("unknown key name {name:?}"));
    }
    Err("body needs `code` or `name`".to_string())
}

fn handle_key(state: &State, body: &[u8]) -> (u16, &'static str, Vec<u8>) {
    let text = match std::str::from_utf8(body) {
        Ok(text) => text,
        Err(_) => return (400, JSON_TYPE, error_json("body is not UTF-8").into_bytes()),
    };
    let value = match json::parse(text) {
        Ok(value) => value,
        Err(e) => {
            return (
                400,
                JSON_TYPE,
                error_json(&format!("invalid JSON: {e}")).into_bytes(),
            );
        }
    };
    let code = match key_code(&value) {
        Ok(code) => code,
        Err(e) => return (400, JSON_TYPE, error_json(&e).into_bytes()),
    };
    let kind = value.get("type").and_then(json::Value::as_u32).unwrap_or(0);
    if kind != 0 && kind != 2 {
        return (
            400,
            JSON_TYPE,
            error_json("`type` must be 0 (press) or 2 (release)").into_bytes(),
        );
    }
    // A plain press always releases unless the caller asks for a hold.
    let release = value
        .get("release")
        .and_then(json::Value::as_bool)
        .unwrap_or(kind != 2);

    match inject(state, code, kind, release) {
        Ok(sent) => {
            let body = format!(
                "{{\"ok\":true,\"code\":{},\"name\":\"{}\",\"type\":{},\"release\":{},\"primer\":{}}}\n",
                sent.code,
                keys::name(sent.code),
                sent.kind,
                sent.release,
                match sent.primer {
                    Some(p) => format!("{p}"),
                    None => "null".to_string(),
                }
            );
            (200, JSON_TYPE, body.into_bytes())
        }
        Err(e) => (
            500,
            JSON_TYPE,
            error_json(&format!("Q_UIM send failed: {e}")).into_bytes(),
        ),
    }
}

fn parse_steps(value: &json::Value) -> Result<Vec<Step>, String> {
    let items = value
        .get("steps")
        .and_then(json::Value::as_array)
        .ok_or("body needs a `steps` array")?;
    if items.len() > MAX_STEPS {
        return Err(format!("too many steps (max {MAX_STEPS})"));
    }

    let mut steps = Vec::with_capacity(items.len());
    let mut total_ms = 0u64;
    for (i, item) in items.iter().enumerate() {
        let code = key_code(item).map_err(|e| format!("step {}: {e}", i + 1))?;
        let kind = item.get("type").and_then(json::Value::as_u32).unwrap_or(0);
        if kind != 0 && kind != 2 {
            return Err(format!("step {}: `type` must be 0 or 2", i + 1));
        }
        let release = item
            .get("release")
            .and_then(json::Value::as_bool)
            .unwrap_or(kind != 2);
        let delay_ms = item
            .get("delay_ms")
            .and_then(json::Value::as_u32)
            .unwrap_or(0) as u64;
        if delay_ms > MAX_STEP_DELAY_MS {
            return Err(format!(
                "step {}: delay_ms max is {MAX_STEP_DELAY_MS}",
                i + 1
            ));
        }
        total_ms += delay_ms;
        steps.push(Step {
            code,
            kind,
            release,
            delay_ms,
        });
    }
    if total_ms > MAX_MACRO_MS {
        return Err(format!(
            "total delay {total_ms} ms exceeds {MAX_MACRO_MS} ms"
        ));
    }
    Ok(steps)
}

fn handle_macro(state: &State, body: &[u8]) -> (u16, &'static str, Vec<u8>) {
    let text = match std::str::from_utf8(body) {
        Ok(text) => text,
        Err(_) => return (400, JSON_TYPE, error_json("body is not UTF-8").into_bytes()),
    };
    let value = match json::parse(text) {
        Ok(value) => value,
        Err(e) => {
            return (
                400,
                JSON_TYPE,
                error_json(&format!("invalid JSON: {e}")).into_bytes(),
            );
        }
    };
    let steps = match parse_steps(&value) {
        Ok(steps) => steps,
        Err(e) => return (400, JSON_TYPE, error_json(&e).into_bytes()),
    };

    let started = Instant::now();
    log(&format!("macro start ({} steps)", steps.len()));
    match run_macro(state, &steps) {
        Ok(()) => {
            let elapsed = started.elapsed().as_millis();
            log(&format!("macro done ({} steps, {elapsed} ms)", steps.len()));
            let body = format!(
                "{{\"ok\":true,\"steps\":{},\"elapsed_ms\":{elapsed}}}\n",
                steps.len()
            );
            (200, JSON_TYPE, body.into_bytes())
        }
        Err(e) => {
            log(&format!("macro failed: {e}"));
            (500, JSON_TYPE, error_json(&e).into_bytes())
        }
    }
}

fn read_uptime() -> Option<f64> {
    std::fs::read_to_string("/proc/uptime")
        .ok()?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

/// Product threads that indicate the UI pipeline is up (MIXER/Sink/buf_buf)
/// or that a 4K decode path has been created (HevcFactory).
const WATCH_THREADS: [&str; 4] = ["MIXER", "Sink", "buf_buf", "HevcFactory"];

fn scan_threads() -> [bool; 4] {
    let mut hits = [false; 4];
    let Ok(procs) = std::fs::read_dir("/proc") else {
        return hits;
    };
    for proc_entry in procs.flatten() {
        let pid = proc_entry.file_name();
        let pid = pid.to_string_lossy();
        if !pid.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let Ok(tasks) = std::fs::read_dir(format!("/proc/{pid}/task")) else {
            continue;
        };
        for task in tasks.flatten() {
            let Ok(comm) = std::fs::read_to_string(task.path().join("comm")) else {
                continue;
            };
            let comm = comm.trim_end();
            // Product threads are named `abs.MIXER_*`, `abs.Sink_*`, ... on this
            // box, so match anywhere in the name rather than at byte 0.
            for (slot, want) in hits.iter_mut().zip(WATCH_THREADS) {
                if comm.contains(want) {
                    *slot = true;
                }
            }
        }
    }
    hits
}

fn si_json() -> String {
    match si::live_service(si::ADSB_SI_DIR) {
        Some(live) => format!(
            "{{\"ok\":true,\"tlv_stream_id\":{},\"original_network_id\":{},\"service_id\":{},\
             \"service_id_hex\":\"{:#06x}\"}}",
            live.tlv_stream_id, live.original_network_id, live.service_id, live.service_id
        ),
        None => "{\"ok\":false,\"reason\":\"no usable /tmp/adsb MH-SDT + MPT\"}".to_string(),
    }
}

fn read_regs() -> String {
    match gr_i2c::read_output_regs() {
        Ok(regs) => format!(
            "{{\"ok\":true,\"lock\":{},\"bank2_selection\":[{},{},{}]}}",
            regs.lock, regs.bank2_selection[0], regs.bank2_selection[1], regs.bank2_selection[2]
        ),
        Err(e) => format!(
            "{{\"ok\":false,\"error\":\"{}\"}}",
            json_escape(&e.to_string())
        ),
    }
}

/// The product's I2C0 semaphore can be held by the tuner for several seconds
/// (or by another diagnostic run), so the register read never blocks `/api/status`:
/// it runs on a worker thread and the newest successful answer is reused while
/// a read is still in flight.
struct RegsCache {
    in_flight: bool,
    last: Option<String>,
    last_at: u64,
}

static REGS: Mutex<RegsCache> = Mutex::new(RegsCache {
    in_flight: false,
    last: None,
    last_at: 0,
});

const REGS_CACHE_SECS: u64 = 30;
const REGS_WAIT: Duration = Duration::from_secs(2);

fn regs_json() -> String {
    let (tx, rx) = std::sync::mpsc::channel();
    {
        let mut cache = REGS.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(last) = &cache.last {
            let age = now_secs().saturating_sub(cache.last_at);
            if !cache.in_flight && age < REGS_CACHE_SECS {
                return last.clone();
            }
        }
        if cache.in_flight {
            return cache.last.clone().unwrap_or_else(|| {
                "{\"ok\":false,\"reason\":\"I2C read in progress\"}".to_string()
            });
        }
        cache.in_flight = true;
    }

    std::thread::spawn(move || {
        let text = read_regs();
        let ok = text.contains("\"ok\":true");
        let mut cache = REGS.lock().unwrap_or_else(|e| e.into_inner());
        cache.in_flight = false;
        if ok {
            cache.last = Some(text.clone());
            cache.last_at = now_secs();
        }
        let _ = tx.send(text);
    });

    match rx.recv_timeout(REGS_WAIT) {
        Ok(text) => text,
        Err(_) => {
            let cache = REGS.lock().unwrap_or_else(|e| e.into_inner());
            cache.last.clone().unwrap_or_else(|| {
                "{\"ok\":false,\"reason\":\"I2C read did not finish in 2 s\"}".to_string()
            })
        }
    }
}

fn q_uim_json() -> String {
    let msqid = match tuner::resolve_msqid(Q_UIM, MSGH_LIST) {
        Ok(msqid) => msqid,
        Err(e) => {
            return format!(
                "{{\"ok\":false,\"qid\":\"0x7f\",\"error\":\"{}\"}}",
                json_escape(&e.to_string())
            );
        }
    };
    let table = match std::fs::read_to_string("/proc/sysvipc/msg") {
        Ok(table) => table,
        Err(e) => {
            return format!(
                "{{\"ok\":false,\"qid\":\"0x7f\",\"msqid\":{msqid},\"error\":\"{}\"}}",
                json_escape(&e.to_string())
            );
        }
    };
    for line in table.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        // key msqid perms cbytes qnum lspid lrpid ...
        if fields.len() < 7 {
            continue;
        }
        if fields[1].parse::<i32>().ok() != Some(msqid) {
            continue;
        }
        return format!(
            "{{\"ok\":true,\"qid\":\"0x7f\",\"msqid\":{msqid},\"qnum\":{},\"lspid\":{},\"lrpid\":{}}}",
            fields[4], fields[5], fields[6]
        );
    }
    format!("{{\"ok\":true,\"qid\":\"0x7f\",\"msqid\":{msqid},\"qnum\":null}}")
}

fn last_key_json(state: &State) -> String {
    let send = state.send.lock().unwrap_or_else(|e| e.into_inner());
    match send.last_event {
        Some(event) => {
            let now = now_secs();
            let age = now.saturating_sub(event.epoch);
            format!(
                "{{\"code\":{},\"name\":\"{}\",\"type\":{},\"age_s\":{age},\"at\":\"{}\"}}",
                event.code,
                keys::name(event.code),
                event.kind,
                hms(event.epoch)
            )
        }
        None => "null".to_string(),
    }
}

fn status_json(state: &State) -> String {
    let now = now_secs();
    let uptime = match read_uptime() {
        Some(u) => format!("{u:.2}"),
        None => "null".to_string(),
    };
    let hits = scan_threads();
    let threads = format!(
        "{{\"MIXER\":{},\"Sink\":{},\"buf_buf\":{},\"HevcFactory\":{}}}",
        hits[0], hits[1], hits[2], hits[3]
    );
    let fields = [
        format!("\"now\":{now}"),
        format!("\"time\":\"{}\"", hms(now)),
        format!("\"uptime_s\":{uptime}"),
        format!("\"threads\":{threads}"),
        format!("\"si\":{}", si_json()),
        format!("\"regs\":{}", regs_json()),
        format!("\"q_uim\":{}", q_uim_json()),
        format!("\"last_key\":{}", last_key_json(state)),
    ];
    format!("{{{}}}\n", fields.join(","))
}

fn handle_conn(mut stream: TcpStream, state: Arc<State>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));
    let started = Instant::now();
    match read_request(&mut stream) {
        Ok(request) => {
            let (status, ctype, body) = route(&state, &request);
            log(&format!(
                "{} {} -> {} ({} bytes, {} ms)",
                request.method,
                request.path,
                status,
                body.len(),
                started.elapsed().as_millis()
            ));
            respond(&mut stream, status, ctype, &body);
        }
        Err(e) => {
            log(&format!("bad request: {e}"));
            respond(
                &mut stream,
                400,
                "text/plain; charset=utf-8",
                format!("bad request: {e}\n").as_bytes(),
            );
        }
    }
}

fn parse_args() -> Result<(String, u16, bool), String> {
    let mut bind = "0.0.0.0".to_string();
    let mut port = 8484u16;
    let mut daemon = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--port" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--port needs a value".to_string())?;
                port = value.parse().map_err(|_| format!("invalid port {value}"))?;
            }
            "--bind" => {
                bind = args
                    .next()
                    .ok_or_else(|| "--bind needs a value".to_string())?;
            }
            "--daemon" | "-d" => daemon = true,
            "--help" | "-h" => {
                println!("usage: t4s-remote [--port 8484] [--bind 0.0.0.0] [--daemon]");
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok((bind, port, daemon))
}

fn main() {
    let (bind, port, daemon) = match parse_args() {
        Ok(values) => values,
        Err(e) => {
            eprintln!("t4s-remote: {e}");
            eprintln!("usage: t4s-remote [--port 8484] [--bind 0.0.0.0] [--daemon]");
            std::process::exit(2);
        }
    };

    if daemon {
        if let Err(e) = daemonize(LOG_PATH) {
            eprintln!("t4s-remote: cannot daemonize: {e}");
            std::process::exit(1);
        }
    }

    let listener = match TcpListener::bind((bind.as_str(), port)) {
        Ok(listener) => listener,
        Err(e) => {
            log(&format!("cannot bind {bind}:{port}: {e}"));
            std::process::exit(1);
        }
    };
    let _ = std::fs::write(PID_PATH, format!("{}\n", std::process::id()));
    log(&format!(
        "t4s-remote pid={} listening on {bind}:{port} (Q_UIM qid {Q_UIM:#x}, msg {MSG_KEY:#x})",
        std::process::id()
    ));
    log("unauthenticated LAN-only remote; primer is inserted per key as needed");

    let state = Arc::new(State::new());
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let state = Arc::clone(&state);
                std::thread::spawn(move || handle_conn(stream, state));
            }
            Err(e) => log(&format!("accept error: {e}")),
        }
    }
}
