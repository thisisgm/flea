#!/usr/bin/env python3
"""Validate native capture receipts and exercise each rejection independently."""
import base64
from collections import Counter
import json
from pathlib import Path
import shlex
import struct
import subprocess
import sys
import zlib

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "ci/visual"))
import blackgate

# A drawn window keeps at least this share of its pixels off its dominant colour.
MIN_CONTENT = 0.001


class Rejected(Exception):
    pass


def require(condition, reason):
    if not condition:
        raise Rejected(reason)


def client(clients, pid):
    owned = [item for item in clients if item.get("pid") == pid]
    require(len(owned) == 1, f"window_never_mapped pid={pid} matches={len(owned)}")
    item = owned[0]
    require(item.get("mapped") is True and item.get("hidden") is False,
            "client_not_visible")
    require(item.get("xwayland") is False, "client_is_not_wayland")
    for key in ("at", "size"):
        values = item.get(key, [])
        require(len(values) == 2 and all(type(value) is int for value in values),
                f"non_integer_client_{key}")
    require(all(value > 0 for value in item["size"]), "empty_client_rect")
    return item


def output(monitors, item):
    owned = [monitor for monitor in monitors if monitor.get("id") == item["monitor"]]
    require(len(owned) == 1, "missing_client_output")
    monitor = owned[0]
    require(monitor.get("name") == "FLEA", "wrong_output")
    require(monitor.get("scale") == 1, "output_scale_is_not_1")
    require((monitor.get("width"), monitor.get("height")) == (2560, 1440),
            "output_size_is_not_2560x1440")
    require(abs(monitor.get("refreshRate", 0) - 120) < 0.1, "output_refresh_is_not_120")
    x, y = item["at"]
    width, height = item["size"]
    require(monitor["x"] <= x and monitor["y"] <= y
            and x + width <= monitor["x"] + monitor["width"]
            and y + height <= monitor["y"] + monitor["height"], "client_outside_output")


def renderer(log, backend):
    lines = [line for line in log.splitlines() if line.startswith("GPUI_RENDERER ")]
    require(len(lines) == 1, f"renderer_report_count={len(lines)}")
    values = dict(word.split("=", 1) for word in shlex.split(lines[0])[1:])
    require(values.get("backend") == backend,
            f"wrong_backend expected={backend} actual={values.get('backend')}")
    require(values.get("software") == "true", "adapter_is_not_software")
    require("llvmpipe" in values.get("adapter", "").lower(), "adapter_is_not_llvmpipe")
    require(values.get("scale") == "1", "app_scale_is_not_1")
    require(values.get("text_size") == "14", "text_size_is_not_14")
    require(values.get("reduced_motion") == "true", "reduced_motion_is_not_set")
    return values["adapter"]


def frame(path, item):
    with Path(path).open("rb") as stream:
        header = stream.read(24)
    require(len(header) == 24 and header[:8] == b"\x89PNG\r\n\x1a\n", "capture_is_not_png")
    width, height = struct.unpack(">II", header[16:24])
    require((width, height) == tuple(item["size"]),
            f"size_mismatch png={width}x{height} "
            f"client={item['size'][0]}x{item['size'][1]}")
    image = blackgate.read_image(Path(path))
    rgb = image[2]
    require(rgb != rgb[:3] * (width * height), "uniform_frame")
    share = blackgate.black_share(image, (0, 0, width, height))
    require(share <= blackgate.BLACK_THRESHOLD,
            f"black_share={share:.6f} threshold={blackgate.BLACK_THRESHOLD}")
    dominant = Counter(rgb[i:i + 3] for i in range(0, len(rgb), 3)).most_common(1)[0][1]
    content = 1 - dominant / (width * height)
    require(content >= MIN_CONTENT, f"sparse_frame content={content:.6f} min={MIN_CONTENT}")
    return share, content


def png(path, width, height, pixel):
    rows = b"".join(b"\0" + b"".join(bytes(pixel(x, y)) for x in range(width)) for y in range(height))
    chunk = lambda kind, data: (struct.pack(">I", len(data)) + kind + data
                                + struct.pack(">I", zlib.crc32(kind + data)))
    path.write_bytes(b"\x89PNG\r\n\x1a\n"
                     + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
                     + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b""))
    return path


def controls(root):
    require((root / ".flea-test-sandbox").is_file(), "unmarked_control_root")
    item = {"pid": 42, "mapped": True, "hidden": False, "xwayland": False,
            "at": [10, 10], "size": [32, 24], "monitor": 0}
    light = (236, 236, 236)
    text = lambda x, y: (40, 40, 40) if y % 6 == 2 and x % 4 else light
    black = png(root / "black.png", 32, 24, lambda x, y: (0, 0, 0))
    near_black = png(root / "near-black.png", 32, 24,
                     lambda x, y: (255, 255, 255) if (x, y) == (5, 5) else (0, 0, 0))
    dark_band = png(root / "dark-band.png", 32, 24, lambda x, y: (0, 0, 0) if y < 7 else text(x, y))
    sparse = png(root / "sparse.png", 64, 48, lambda x, y: (40, 40, 40) if (x, y) == (5, 5) else light)
    wrong_size = png(root / "wrong-size.png", 31, 24, text)
    drawn = png(root / "drawn.png", 32, 24, text)
    report = ('GPUI_RENDERER backend=gl adapter="llvmpipe" driver="" '
              'software=true scale=1 text_size=14 reduced_motion=true')
    cases = [
        ("black-frame", "uniform_frame", lambda: frame(black, item)),
        ("near-black", "black_share", lambda: frame(near_black, item)),
        ("dark-band", "black_share", lambda: frame(dark_band, item)),
        ("sparse-frame", "sparse_frame", lambda: frame(sparse, dict(item, size=[64, 48]))),
        ("size-mismatch", "size_mismatch", lambda: frame(wrong_size, item)),
        ("wrong-backend", "wrong_backend", lambda: renderer(report, "vulkan")),
    ]
    for name, expected, check in cases:
        try:
            check()
        except Rejected as error:
            require(str(error).startswith(expected), f"wrong_control_error name={name}: {error}")
            print(f"GPUI_CAPTURE_CONTROL name={name} rejected=GPUI_CAPTURE_REJECT {error}")
        else:
            raise Rejected(f"control_was_accepted name={name}")
    share, content = frame(drawn, item)
    print(f"GPUI_CAPTURE_CONTROL_ACCEPT name=drawn black_share={share:.6f} content={content:.6f}")


def main():
    command, *args = sys.argv[1:]
    if command == "client":
        item = client(json.loads(Path(args[0]).read_text()), int(args[1]))
        print(json.dumps(item, sort_keys=True))
    elif command == "check":
        root, backend = Path(args[0]), args[1]
        item = json.loads((root / f"{backend}-client.json").read_text())
        output(json.loads((root / "monitors.json").read_text()), item)
        adapter = renderer((root / f"{backend}-app.log").read_text(), backend)
        share, content = frame(root / f"{backend}.png", item)
        print(f"GPUI_CAPTURE_BLACKGATE backend={backend} black_share={share:.6f} content={content:.6f}")
        print(f"GPUI_CAPTURE PASS backend={backend} adapter={adapter} "
              f"size={item['size'][0]}x{item['size'][1]}")
    elif command == "frame":
        share, content = frame(Path(args[0]), json.loads(Path(args[1]).read_text()))
        print(f"GPUI_CAPTURE_FRAME black_share={share:.6f} content={content:.6f}")
    elif command == "controls":
        controls(Path(args[0]))
    elif command == "artifacts":
        for path in sorted(Path(args[0]).iterdir()):
            if path.is_file() and not path.name.startswith(".") and path.stat().st_size <= 1048576:
                print(f"GPUI_CAPTURE_ARTIFACT name={path.name} "
                      f"data={base64.b64encode(path.read_bytes()).decode('ascii')}")
    else:
        raise Rejected(f"unknown_command={command}")


if __name__ == "__main__":
    try:
        main()
    except (Rejected, OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        print(f"GPUI_CAPTURE_REJECT {error}", file=sys.stderr)
        sys.exit(1)
