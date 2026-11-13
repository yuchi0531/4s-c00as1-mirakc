use std::sync::Arc;
use std::time::{Duration, Instant};
use t4s_core::tssrc::TsSource;
use t4s_core::{
    channels,
    graph::{Graph, ObjReader},
    remux4k, si, tuner,
    xcdrv::XcDrv,
};

fn graph_dump(drv: &Arc<XcDrv>) {
    let g = Graph::read().expect("graph");
    println!("ctx={:#x}", g.ctx);
    let obj = ObjReader::new(Arc::clone(drv));
    if let Some((count, _, rows)) = g.tables.get("device") {
        println!("devices count={count}");
        for r in rows {
            println!("  dev[{}] handle={:#x} type={:#x} pipe={:#x}", r.index, r.words[0], r.words[1], r.words[2]);
        }
    }
    if let Some((_, _, rows)) = g.tables.get("buffer") {
        println!("buffers");
        for r in rows {
            let h = r.words[0];
            match obj.object(h) {
                Ok(b) => {
                    print!(
                        "  buf[{}] h={:#x} size={:#x} data={:#x} wpos={:#x} raw0..8={:?}",
                        r.index, h, b.size, b.data, b.wpos, &b.raw[..8]
                    );
                    match drv.sbuf_status_raw(h) {
                        Ok(st) => println!(" status={st:?}"),
                        Err(e) => println!(" status_err={e}"),
                    }
                }
                Err(e) => println!("  buf[{}] h={:#x} err={e}", r.index, h),
            }
        }
    }
    if let Some((_, _, rows)) = g.tables.get("port") {
        println!("ports");
        for r in rows {
            println!("  port[{}] {:?}", r.index, r.words);
        }
    }
    println!("mmr6000={:#x} mmr6008={:#x} mmr600c={:#x} mmr61f0={:#x}",
        drv.mmr_read32(0x6000).unwrap_or(0),
        drv.mmr_read32(0x6008).unwrap_or(0),
        drv.mmr_read32(0x600c).unwrap_or(0),
        drv.mmr_read32(0x61f0).unwrap_or(0));
}

fn main() {
    let arg = std::env::args().nth(1).unwrap_or_default();
    let drv = Arc::new(XcDrv::open_path(t4s_core::xcdrv::DEV_PATH).expect("driver"));
    if arg == "cleanup2" {
        let dev = 0xdu32;
        let conn = 0x1e009700u32;
        let buf = 0x28d66f00u32;
        for i in 0..3 {
            match drv.disconnect_product_demux_timeout(dev, conn, 10000) {
                Ok(()) => println!("disconnect {i} ok"),
                Err(e) => println!("disconnect {i} ERR {e}"),
            }
            match drv.stop_device_timeout(dev, 10000) {
                Ok(()) => println!("stop {i} ok"),
                Err(e) => println!("stop {i} ERR {e}"),
            }
        }
        match drv.force_reset_buffer(buf) {
            Ok(()) => println!("reset ok"),
            Err(e) => println!("reset ERR {e}"),
        }
        match drv.destroy_buffer(buf) {
            Ok(()) => println!("destroy buf ok"),
            Err(e) => println!("destroy buf ERR {e}"),
        }
        match drv.destroy_device(dev) {
            Ok(()) => println!("destroy dev ok"),
            Err(e) => println!("destroy dev ERR {e}"),
        }
        return;
    }
    if arg == "cleanup" {
        // Remove only graph objects created by our failed tuner sessions.
        let g = t4s_core::graph::Graph::read().expect("graph");
        let product_pipes: std::collections::BTreeSet<u32> = g
            .tables
            .get("device")
            .unwrap()
            .2
            .iter()
            .filter(|r| r.words[0] <= 0xb)
            .map(|r| r.words[2])
            .filter(|&p| p != 0)
            .collect();
        let leaked: Vec<(u32, u32)> = g
            .tables
            .get("device")
            .unwrap()
            .2
            .iter()
            .filter(|r| r.words[0] != 0 && r.words[0] > 0xb && !product_pipes.contains(&r.words[2]))
            .map(|r| (r.words[0], r.words[2]))
            .collect();
        let ports = &g.tables.get("port").unwrap().2;
        let buffers: Vec<u32> = g
            .tables
            .get("buffer")
            .unwrap()
            .2
            .iter()
            .map(|r| r.words[0])
            .filter(|&h| h >= 0x1f00_0000)
            .collect();
        println!("product_pipes={product_pipes:?} leaked={leaked:?} buffers={buffers:#x?}");
        for &(dev, _) in &leaked {
            for r in ports {
                if r.words[1] == 0x0b {
                    // host connection: [reader, 0x0b, buffer, 0, 0]
                    let reader = r.words[0];
                    let _ = drv.cancel_get_read(reader);
                    match drv.destroy_reader(reader) {
                        Ok(()) => println!("destroy_reader {reader:#x} ok"),
                        Err(e) => println!("destroy_reader {reader:#x} ERR {e}"),
                    }
                }
                if r.words[1] == 1 && r.words[2] == dev {
                    let conn = r.words[0];
                    match drv.disconnect(dev, conn) {
                        Ok(()) => println!("disconnect dev {dev:#x} conn {conn:#x} ok"),
                        Err(e) => println!("disconnect dev {dev:#x} conn {conn:#x} ERR {e}"),
                    }
                }
            }
            match drv.stop_device(dev) {
                Ok(()) => println!("stop dev {dev:#x} ok"),
                Err(e) => println!("stop dev {dev:#x} ERR {e}"),
            }
            match drv.destroy_device(dev) {
                Ok(()) => println!("destroy dev {dev:#x} ok"),
                Err(e) => println!("destroy dev {dev:#x} ERR {e}"),
            }
        }
        for &b in &buffers {
            match drv.force_reset_buffer(b) {
                Ok(()) => println!("reset buf {b:#x} ok"),
                Err(e) => println!("reset buf {b:#x} ERR {e}"),
            }
            match drv.destroy_buffer(b) {
                Ok(()) => println!("destroy buf {b:#x} ok"),
                Err(e) => println!("destroy buf {b:#x} ERR {e}"),
            }
        }
        let mut pipes = std::collections::BTreeSet::new();
        for &(_, p) in &leaked {
            if p != 0 && !product_pipes.contains(&p) {
                pipes.insert(p);
            }
        }
        for p in pipes {
            match drv.stop_pipeline(p) {
                Ok(()) => println!("stop pipe {p:#x} ok"),
                Err(e) => println!("stop pipe {p:#x} ERR {e}"),
            }
            match drv.destroy_pipeline(p) {
                Ok(()) => println!("destroy pipe {p:#x} ok"),
                Err(e) => println!("destroy pipe {p:#x} ERR {e}"),
            }
        }
        return;
    }
    if arg == "restore-tsi" {
        println!("before 61f0={:#x}", drv.mmr_read32(0x61f0).unwrap_or(0));
        match drv.restore_tsi_identity_from_current() {
            Ok(()) => println!("restored 61f0={:#x}", drv.mmr_read32(0x61f0).unwrap_or(0)),
            Err(e) => println!("restore ERR {e}"),
        }
        return;
    }
    if arg == "graph" {
        graph_dump(&drv);
        return;
    }
    if arg == "fed2k" {
        let drv = Arc::new(XcDrv::open_path(t4s_core::xcdrv::DEV_PATH).expect("driver"));
        let rf: u32 = std::env::args().nth(2).and_then(|v| v.parse().ok()).unwrap_or(12_291_000);
        let tsid: u16 = std::env::args().nth(3).and_then(|v| v.parse().ok()).unwrap_or(0);
        let fem = tuner::Tuner::open(tuner::MSGH_LIST).expect("fem");
        fem.set_device(&tuner::FemDeviceRequest::assign(1, 2)).expect("assign 2K");
        std::thread::sleep(Duration::from_millis(300));
        let req = tuner::FedTuneRequest::isdb_s(1, tsid, rf).expect("req");
        let t = tuner::Tuner::open_qid(req.fed_qid, tuner::MSGH_LIST).expect("fed queue");
        t.tune_fed(&req).expect("tune");
        std::thread::sleep(Duration::from_secs(2));
        match t4s_core::gr_i2c::read_isdbs_status() {
            Ok(v) => println!("fed2k rf={rf} tsid={tsid:#06x} lock={} ts_pin={} ts_serial={}", v.lock, v.ts_pin, v.ts_serial),
            Err(e) => println!("fed2k status ERR {e}"),
        }
        println!("tsi0={:?}", drv.tsi_status(0));
        println!("tsi1={:?}", drv.tsi_status(1));
        return;
    }
    if arg == "isdbs" {
        match t4s_core::gr_i2c::read_isdbs_status() {
            Ok(v) => println!("status lock={} ts_pin={} ts_serial={}", v.lock, v.ts_pin, v.ts_serial),
            Err(e) => println!("status ERR {e}"),
        }
        match t4s_core::gr_i2c::read_isdbs_tmcc() {
            Ok(v) => println!("tmcc locks={:?} sel={:?} tsids={:?}", v.locks, v.selectors, v.indexed_tsids),
            Err(e) => println!("tmcc ERR {e}"),
        }
        match t4s_core::gr_i2c::read_output_regs() {
            Ok(v) => println!("out lock={:#x} bank2={:?} bank0={:?}", v.lock, v.bank2_selection, v.bank0),
            Err(e) => println!("out ERR {e}"),
        }
        return;
    }
    if arg == "fed" {
        let sid: u16 = std::env::args().nth(2).and_then(|v| v.parse().ok()).unwrap_or(101);
        let (tsid, freq, _) = channels::tuning_record(sid).expect("sid");
        let fem = tuner::Tuner::open(tuner::MSGH_LIST).expect("fem queue");
        fem.set_device(&tuner::FemDeviceRequest::assign(1, 0x200)).expect("assign ADBS");
        std::thread::sleep(Duration::from_millis(300));
        let req = tuner::FedTuneRequest::advanced_bs(tsid, freq);
        let t = tuner::Tuner::open_qid(tuner::FED_ADBS1_QID, tuner::MSGH_LIST).expect("fed queue");
        t.tune_fed(&req).expect("fed tune");
        println!("fed tune sid={sid} tsid={tsid:#06x} rf={freq}");
        let begin = Instant::now();
        let mut locked = false;
        while begin.elapsed() < Duration::from_secs(15) {
            std::thread::sleep(Duration::from_millis(200));
            if let Ok(r) = t4s_core::gr_i2c::read_output_regs() {
                if r.lock & 0x20 != 0 && r.bank2_selection[..2] == tsid.to_be_bytes() {
                    println!("locked after {:?}", begin.elapsed());
                    locked = true;
                    break;
                }
            }
        }
        if !locked { println!("no lock/tsid confirmation"); }
        return;
    }
    if arg == "dupout" {
        let g = Graph::read().expect("graph");
        let tlv = g.tables.get("buffer").unwrap().2[0].words[0];
        let dev = g.tables.get("device").unwrap().2.iter().find(|r| r.words[1] == 0x24).unwrap().words[0];
        println!("tlv ring {tlv:#x} dev {dev:#x}");
        let out = drv.create_capture_linear_buffer(0x200000).expect("ring");
        println!("out {out:#x}");
        match drv.connect_output(dev, out) {
            Ok(conn) => {
                println!("connect_output ok conn={conn:#x}");
                let _ = drv.set_buffer_ready(out);
                let src = t4s_core::tssrc::XcReaderSource::attach(Arc::clone(&drv), out, 200);
                match src {
                    Ok(mut src) => {
                        let mut total = 0usize;
                        let mut first = Vec::new();
                        let begin = Instant::now();
                        while begin.elapsed() < Duration::from_secs(3) {
                            let mut buf = Vec::new();
                            match src.read_into(&mut buf) {
                                Ok(n) => {
                                    total += n;
                                    if first.len() < 64 { first.extend_from_slice(&buf); }
                                }
                                Err(e) => { println!("read ERR {e}"); break; }
                            }
                        }
                        println!("total={total} first={:02x?}", first);
                        drop(src);
                    }
                    Err(e) => println!("reader ERR {e}"),
                }
                match drv.disconnect(dev, conn) {
                    Ok(()) => println!("disconnect ok"),
                    Err(e) => println!("disconnect ERR {e}"),
                }
            }
            Err(e) => println!("connect_output ERR {e}"),
        }
        let _ = drv.destroy_buffer(out);
        return;
    }
    if arg == "tap" {
        let g = Graph::read().expect("graph");
        let tlv = g.tables.get("buffer").unwrap().2[0].words[0];
        println!("tlv ring {tlv:#x}");
        let dev = drv.create_ts_recorder().expect("ts recorder");
        println!("dev {dev:#x}");
        match drv.connect_input(dev, tlv) {
            Ok(conn) => {
                println!("connect_input ok conn={conn:#x}");
                match drv.disconnect(dev, conn) {
                    Ok(()) => println!("disconnect ok"),
                    Err(e) => println!("disconnect ERR {e}"),
                }
            }
            Err(e) => println!("connect_input ERR {e}"),
        }
        match drv.destroy_device(dev) {
            Ok(()) => println!("destroy dev ok"),
            Err(e) => println!("destroy dev ERR {e}"),
        }
        return;
    }
    if arg == "status" {
        let g = Graph::read().expect("graph");
        if let Some((_, _, rows)) = g.tables.get("buffer") {
            for r in rows {
                let h = r.words[0];
                println!("h={h:#x} status={:?}", drv.sbuf_status_raw(h));
            }
        }
        return;
    }
    println!("si={:?}", si::live_service(si::ADSB_SI_DIR));
    println!("discover={:?}", remux4k::discover(&drv));
    let (sid, mode, nd) = match arg.as_str() {
        "resume" => (101u16, tuner::TUNE_RESUME, 0),
        "change" => (101u16, tuner::TUNE_CHANNEL_CHANGE, 0),
        "change141" => (141u16, tuner::TUNE_CHANNEL_CHANGE, 0),
        "send" => {
            let sid = std::env::args().nth(2).and_then(|v| v.parse().ok()).unwrap_or(101);
            let mode = std::env::args().nth(3).and_then(|v| v.parse().ok()).unwrap_or(0);
            let nd = std::env::args().nth(4).and_then(|v| v.parse().ok()).unwrap_or(0);
            (sid, mode, nd)
        }
        _ => return,
    };
    let (tsid, freq, bid) = channels::tuning_record(sid).unwrap();
    let t = tuner::Tuner::open_qid(tuner::ADSB_COMM_QID, tuner::MSGH_LIST).unwrap();
    t.change_channel(&tuner::ChangeChannelRequest {
        oid: 0x41,
        tune: mode,
        not_decode: nd,
        service_id: sid,
        freq_khz: freq,
        tlv_stream_id: tsid,
        bid,
    })
    .unwrap();
    let begin = Instant::now();
    while begin.elapsed() < Duration::from_secs(20) {
        std::thread::sleep(Duration::from_millis(500));
        println!(
            "t={:?} si={:?} discover={:?}",
            begin.elapsed().as_secs_f32(),
            si::live_service(si::ADSB_SI_DIR).map(|l| (l.service_id, l.tlv_stream_id)),
            remux4k::discover(&drv).map(|e| (e.video_readers, e.audio_readers, e.mmt_demux))
        );
    }
}
