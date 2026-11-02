"""Record and reject animated or active wallpapers before a native benchmark."""

import argparse
import json
from pathlib import Path
import struct
import sys
import time

VIDEO = {".mp4", ".webm", ".mkv", ".mov", ".gif"}


def background(home):
    target = (home / ".local/state/omarchy/current/background").resolve(strict=True)
    if not target.is_file():
        raise ValueError("background target is not a regular file")
    return target


def animated_webp(path):
    with path.open("rb") as stream:
        header = stream.read(12)
        if len(header) != 12 or header[:4] != b"RIFF" or header[8:] != b"WEBP":
            return False
        end = struct.unpack("<I", header[4:8])[0] + 8
        if end > path.stat().st_size or end < 12:
            raise ValueError("truncated WebP background")
        while stream.tell() < end:
            chunk = stream.read(8)
            if len(chunk) != 8:
                raise ValueError("truncated WebP chunk")
            size = struct.unpack("<I", chunk[4:])[0]
            following = stream.tell() + size + (size & 1)
            if following > end:
                raise ValueError("WebP chunk exceeds RIFF bounds")
            if chunk[:4] == b"ANIM":
                return True
            if chunk[:4] == b"VP8X" and size:
                if stream.read(1)[0] & 2:
                    return True
            stream.seek(following)
    return False


def processes(proc_root):
    reading = dict(at_monotonic_s=time.monotonic(), processes={})
    for directory in proc_root.iterdir():
        if not directory.name.isdigit():
            continue
        try:
            line = (directory / "stat").read_text()
        except (FileNotFoundError, ProcessLookupError):
            continue
        opening, closing = line.find("("), line.rfind(")")
        if line[opening + 1:closing] != "owe-render":
            continue
        fields = line[closing + 1:].split()
        user, system, started = int(fields[11]), int(fields[12]), int(fields[19])
        reading["processes"][directory.name] = dict(utime=user, stime=system, total_ticks=user + system, starttime=started)
    return reading


def probe(home, bg, proc_root=Path("/proc"), pause=time.sleep):
    result = dict(bg=bg, interval_s=5, tick_limit=2, failures=[])
    try:
        initial = background(home)
        result["background"] = str(initial)
        result["animated"] = initial.suffix.lower() in VIDEO or animated_webp(initial)
        result["first"] = processes(proc_root)
        pause(5)
        result["second"] = processes(proc_root)
        result["background_after"] = str(background(home))
        if initial.name != bg or result["background_after"] != str(initial):
            result["failures"].append("FAIL B7_BACKGROUND_CHANGED: background link changed during preflight")
        if result["animated"]:
            result["failures"].append(f"FAIL B7_BACKGROUND_ANIMATED: {bg}")
        before, after = result["first"]["processes"], result["second"]["processes"]
        result["deltas"] = {}
        if before.keys() != after.keys():
            result["failures"].append("FAIL B7_BACKGROUND_OWE_CHANGED: owe-render appeared or exited during preflight")
        for pid in before.keys() & after.keys():
            delta = after[pid]["total_ticks"] - before[pid]["total_ticks"]
            result["deltas"][pid] = delta
            if before[pid]["starttime"] != after[pid]["starttime"] or delta < 0:
                result["failures"].append(f"FAIL B7_BACKGROUND_OWE_CHANGED: owe-render pid {pid} changed identity")
            elif delta > 2:
                result["failures"].append(f"FAIL B7_BACKGROUND_OWE_BUSY: pid {pid} used {delta} CPU ticks over 5 s")
    except (OSError, ValueError, IndexError) as error:
        result["failures"].append(f"FAIL B7_BACKGROUND_PROBE: {error}")
    result["passed"] = not result["failures"]
    return result


def main(argv=None, proc_root=Path("/proc"), pause=time.sleep):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("evidence", type=Path)
    parser.add_argument("bg")
    args = parser.parse_args(argv)
    if not args.evidence.is_absolute() or args.evidence.resolve() != args.evidence or not (args.evidence / ".flea-test-sandbox").is_file():
        print("FAIL B7_BACKGROUND_ROOT: evidence needs a canonical marked directory", file=sys.stderr)
        return 2
    result = probe(Path.home(), args.bg, proc_root, pause)
    with (args.evidence / "background.json").open("x") as stream:
        json.dump(result, stream, indent=2, sort_keys=True)
        stream.write("\n")
    for failure in result["failures"]:
        print(failure, file=sys.stderr)
    print(f"B7 background={args.bg} evidence={args.evidence}")
    return 0 if result["passed"] else 2


if __name__ == "__main__":
    try:
        sys.exit(main())
    except OSError as error:
        print(f"FAIL B7_BACKGROUND_EVIDENCE: {error}", file=sys.stderr)
        sys.exit(2)
