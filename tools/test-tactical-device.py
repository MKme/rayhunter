"""Exercise the installed LCD through local APIs and framebuffer readback.

Requires the owner's maintenance connection. Creates a new recording after a
brief pause; never deletes captures or writes configuration. Evidence stays in
the supplied output directory. Uses only the Python standard library.
"""
import argparse
import base64
import importlib.util
import json
from pathlib import Path
import re
import struct
import time
import urllib.request

spec = importlib.util.spec_from_file_location("maintenance", Path(__file__).with_name("orbic-maintenance.py"))
maintenance = importlib.util.module_from_spec(spec)
spec.loader.exec_module(maintenance)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--host", default="192.168.1.1")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    report = {"checks": [], "screens": {}}
    font = {tuple(map(int, rows.split(","))): char for char, rows in re.findall(
        r"'(.)' => \[([0-9, ]+)\]", (Path(__file__).parent.parent / "daemon/src/display/bitmap_font.rs").read_text())}
    font[(0,) * 7] = " "

    def api(path, post=False):
        request = urllib.request.Request(f"http://{args.host}:8080/api/{path}", data=b"" if post else None)
        with opener.open(request, timeout=8) as response:
            data = response.read()
        try:
            return json.loads(data)
        except json.JSONDecodeError:
            return data.decode()

    def capture(name):
        raw = base64.b64decode(maintenance.run(args.host, "base64 /dev/fb0"))
        assert len(raw) == 32768, len(raw)
        (args.output / (name + ".rgb565")).write_bytes(raw)
        pixels = struct.unpack("<16384H", raw)
        return pixels

    def text(pixels, x, y, length, scale=1):
        background = pixels[127 * 128 + 127]
        result = ""
        for i in range(length):
            rows = tuple(sum((pixels[(y + row * scale) * 128 + x + i * 6 * scale + col * scale] != background)
                             << (4 - col) for col in range(5)) for row in range(7))
            result += font.get(rows, "?")
        return result.strip()

    def screen(pixels):
        return {"status": text(pixels, 4, 17, 10, 2),
                "scope": text(pixels, 4, 35, 8),
                "total": text(pixels, 76, 35, 4, 2),
                "high": text(pixels, 4, 56, 5),
                "medium": text(pixels, 46, 56, 5),
                "low": text(pixels, 88, 56, 5),
                "traffic": text(pixels, 4, 68, 20),
                "battery": text(pixels, 4, 80, 20),
                "config": text(pixels, 4, 94, 20),
                "address1": text(pixels, 4, 105, 20),
                "address2": text(pixels, 4, 117, 20)}

    def counts(pixels):
        data = screen(pixels)
        return tuple(data[k] for k in ("total", "high", "medium", "low"))

    original = api("qmdl-manifest")
    config_before = maintenance.run(args.host, "sha256sum /data/rayhunter/config.toml").decode().split()[0]
    assert api("config")["ui_level"] == 5
    assert original["current_entry"] is not None, "Pause/resume test requires an active recording"
    needs_resume = False
    try:
        before = capture("recording")
        report["screens"]["recording"] = screen(before)
        api("stop-recording", True)
        needs_resume = True
        for _ in range(30):
            if api("qmdl-manifest")["current_entry"] is None:
                break
            time.sleep(0.1)
        else:
            raise AssertionError("Recording did not close")
        time.sleep(0.4)
        paused = capture("paused")
        report["screens"]["paused"] = screen(paused)
        assert screen(paused)["status"] == "PAUSED"
        assert screen(paused)["scope"] == "LAST RUN"
        time.sleep(1.2)
        paused_later = capture("paused-later")
        assert counts(paused_later) == counts(paused)
        assert screen(paused_later)["traffic"].split("RUN")[1] == screen(paused)["traffic"].split("RUN")[1]
        report["checks"].append("PAUSED / LAST RUN, retained counts, frozen RUN duration")
        api("start-recording", True)
        needs_resume = False
        for _ in range(30):
            current = api("qmdl-manifest")["current_entry"]
            if current and current["name"] != original["current_entry"]["name"]:
                break
            time.sleep(0.1)
        else:
            raise AssertionError("New recording did not start")
        time.sleep(0.4)
        resumed = capture("resumed")
        report["screens"]["resumed"] = screen(resumed)
        assert screen(resumed)["scope"] == "SESSION"
        assert screen(resumed)["status"] in ("RECORDING", "WAIT DATA", "LOW ALERT", "MED ALERT", "HIGH ALERT")
        report["checks"].append("New recording returns to SESSION; paused recording retained")
        time.sleep(2)
        after = api("qmdl-manifest")
        names = {item["name"] for item in after["entries"]}
        assert all(item["name"] in names for item in original["entries"])
        assert original["current_entry"]["name"] in names
        assert after["current_entry"]["qmdl_size_bytes"] > 0
        config_after = maintenance.run(args.host, "sha256sum /data/rayhunter/config.toml").decode().split()[0]
        assert config_before == config_after
        report["checks"].append("All prior manifest entries retained, capture bytes positive, configuration hash unchanged")
        report["recording_active"] = True
        report["existing_entries_preserved"] = len(original["entries"]) + 1
        report["result"] = "PASS"
    finally:
        if needs_resume:
            api("start-recording", True)
        (args.output / "report.json").write_text(json.dumps(report, indent=2))
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
