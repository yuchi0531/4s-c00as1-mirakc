#!/usr/bin/env python3
# tsid_probe.py — read only the PAT TSID from a Mirakurun/mirakc stream.
#
# The remote server is used **only as a TSID data source**: the TS is read until
# the first PAT is found and the bytes are discarded.  No stream is stored or
# fed into the local mirakc/tuner.
#
# Usage:
#   tsid_probe.py http://192.168.100.111:40772 --types CS
#   tsid_probe.py http://192.168.100.111:40772 --types BS CS --channels CS2,CS4
import argparse
import json
import sys
import urllib.request


def parse_pat(packet: bytes):
    if len(packet) != 188 or packet[0] != 0x47:
        return None
    pid = ((packet[1] & 0x1F) << 8) | packet[2]
    if pid != 0 or not (packet[1] & 0x40):
        return None
    off = 4
    if packet[3] & 0x20:
        off = 5 + packet[4]
    if off >= 188:
        return None
    pointer = packet[off]
    start = off + 1 + pointer
    if start + 12 > 188:
        return None
    section = packet[start:]
    if section[0] != 0x00:
        return None
    length = ((section[1] & 0x0F) << 8) | section[2]
    if 3 + length > len(section) or length < 9:
        return None
    tsid = (section[3] << 8) | section[4]
    programs = []
    for i in range(8, 3 + length - 4, 4):
        sid = (section[i] << 8) | section[i + 1]
        pmt = ((section[i + 2] & 0x1F) << 8) | section[i + 3]
        if sid != 0:
            programs.append({"service_id": sid, "pmt_pid": pmt})
    return tsid, programs


def measure(base: str, channel_type: str, channel: str, timeout: int = 12):
    url = f"{base}/api/channels/{channel_type}/{channel}/stream?decode=0"
    try:
        with urllib.request.urlopen(url, timeout=timeout) as r:
            while True:
                chunk = r.read(188 * 200)
                if not chunk:
                    return {"error": "stream ended before PAT"}
                for i in range(0, len(chunk) - 187, 188):
                    pat = parse_pat(chunk[i : i + 188])
                    if pat:
                        return {"tsid": pat[0], "programs": pat[1]}
    except Exception as e:  # noqa: BLE001 - probe reports the cause verbatim
        return {"error": str(e)}


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("base", help="e.g. http://192.168.100.111:40772")
    ap.add_argument("--types", nargs="+", default=["BS", "CS"])
    ap.add_argument("--channels", default="", help="comma separated filter")
    args = ap.parse_args()
    base = args.base.rstrip("/")
    with urllib.request.urlopen(base + "/api/channels", timeout=10) as r:
        channels = json.load(r)
    wanted = {c.strip() for c in args.channels.split(",") if c.strip()}
    rows = []
    for ch in channels:
        if ch["type"] not in args.types:
            continue
        if wanted and ch["channel"] not in wanted:
            continue
        print(f"# {ch['type']}/{ch['channel']} {ch['name']}", file=sys.stderr, flush=True)
        result = measure(base, ch["type"], ch["channel"])
        row = {
            "type": ch["type"],
            "channel": ch["channel"],
            "name": ch["name"],
            **result,
        }
        rows.append(row)
        if "tsid" in row:
            print(f"{ch['type']}/{ch['channel']}\t{row['tsid']:#06x}\t{row['tsid']}\t{ch['name']}", flush=True)
        else:
            print(f"{ch['type']}/{ch['channel']}\tERR\t{row['error']}", flush=True)
    out = {
        "source": base,
        "note": "PAT-based TSID measurement only; no stream retained",
        "rows": rows,
    }
    print(json.dumps(out, ensure_ascii=False, indent=1))


if __name__ == "__main__":
    main()
