"""Executable transport stubs, used only inside the marked offline test root."""

import base64
import datetime
import hashlib
import importlib.machinery
import importlib.util
import json
import os
from pathlib import Path
import re
import struct
import subprocess
import sys
import tempfile
import time
import zlib

ROOT = next(parent for parent in Path(sys.argv[0]).absolute().parents if (parent / ".b7-offline.json").is_file())
META = json.loads((ROOT / ".b7-offline.json").read_text())
NAME = Path(sys.argv[0]).name
ARGS = sys.argv[1:]
STATE = ROOT / "window.json"


def log(kind, **data):
    with (ROOT / "stub-commands.jsonl").open("a") as stream:
        stream.write(json.dumps(dict(kind=kind, argv=ARGS, **data)) + "\n")


def window():
    if not STATE.exists():
        return {}
    result = json.loads(STATE.read_text())
    return result if Path(f"/proc/{result['pid']}").exists() else {}


def publish_window(value):
    with tempfile.NamedTemporaryFile(mode="w", prefix="window-", dir=ROOT, delete=False) as stream:
        json.dump(value, stream)
        name = stream.name
    os.replace(name, STATE)


def png(width=256, height=192, uri=None):
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
    text = chunk(b"tEXt", b"Thumb::URI\0" + uri.encode()) if uri else b""
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)) + text + chunk(b"IDAT", zlib.compress((b"\0" + b"\x11\x22\x33" * width) * height)) + chunk(b"IEND", b"")


# Entrants whose own ctrl+q quits; the rest have no such binding and close only on the compositor's close request.
CHORD_QUITS = {"krusader", "dolphin", "nautilus"}
# KIO shows a preview but saves it only when its thumbnail root already exists (kio filepreviewjob.cpp canBeCached).
KIO_ONLY_WITH_ROOT = {"krusader", "dolphin"}
# Nemo declines an image over its thumbnail-limit default, 1048576 bytes.
NEMO_LIMIT = 1048576
# Rivals that write the freedesktop "normal" 128 px flavor; Strata, Flea and Nemo write "large" 256 px (Nemo as the FIX4 run showed).
NORMAL_WRITERS = {"krusader", "dolphin", "thunar", "pcmanfm"}


def event(kind):
    at = datetime.datetime.now(datetime.timezone.utc).isoformat().replace("+00:00", "Z")
    print(f"{at} DEBUG browser worker completed operation={kind} queue_ms=0 elapsed_ms=12", flush=True)


log(NAME)
if NAME == "flea-field-bench":
    assert ARGS[0] == "--thumbs"
    command = ARGS[1]
    loader = importlib.machinery.SourceFileLoader("offline_driver", META["driver"])
    spec = importlib.util.spec_from_loader(loader.name, loader)
    driver = importlib.util.module_from_spec(spec)
    loader.exec_module(driver)
    if command in ("fixtures", "verify"):
        print(json.dumps(dict(simulated=True, phase=command)))
        raise SystemExit(0)
    driver.wayland.NATIVE_RUNTIME = Path(os.environ["XDG_RUNTIME_DIR"])
    if command == "run":
        driver.verify_fixtures = lambda root: dict(simulated=True)
        driver.measure.run_one = lambda *args: dict(all_thumbnails_ms=None, per_thumbnail_ms=None,
            preview_800_ms=None, user_cpu_s=None, simulated=True)
    sys.argv = [META["driver"], *ARGS[1:]]
    raise SystemExit(driver.main())
elif NAME == "bwrap":
    env = dict(os.environ)
    while ARGS and ARGS[0].startswith("--"):
        flag = ARGS.pop(0)
        if flag in ("--ro-bind", "--bind", "--dev-bind"):
            source, target = ARGS[:2]
            del ARGS[:2]
            if target == "/run/flea-b7":
                env["XDG_RUNTIME_DIR"] = source
        elif flag in ("--tmpfs", "--proc", "--dev"):
            ARGS.pop(0)
        elif flag not in ("--die-with-parent", "--unshare-pid", "--unshare-net"):
            raise SystemExit(f"unhandled bubblewrap flag: {flag}")
    os.execvpe(ARGS[0], ARGS, env)
elif NAME == "perf":
    if "-o" in ARGS:
        Path(ARGS[ARGS.index("-o") + 1]).write_text("1234.000,msec,task-clock:u,1,100.0,,\n")
    command = ARGS[ARGS.index("--") + 1:]
    os.execvpe(command[0], command, os.environ)
elif NAME == "dbus-run-session":
    command = ARGS[ARGS.index("--") + 1:]
    env = dict(os.environ, DBUS_SESSION_BUS_ADDRESS="unix:path=" + os.environ["XDG_RUNTIME_DIR"] + "/stub-bus")
    os.execvpe(command[0], command, env)
elif NAME == "pacman":
    if ARGS == ["-Q", "kdegraphics-thumbnailers"]:
        print("kdegraphics-thumbnailers 26.08.2-1")
    elif ARGS[:2] == ["-Qoq", "--"]:
        print(META["binaries"][str(Path(ARGS[2]).resolve())]["id"])
    else:
        assert ARGS[:2] == ["-Q", "--"]
        row = next(row for row in META["binaries"].values() if row["id"] == ARGS[2])
        print(row["id"], row.get("package_version", "2.9.0-2"))
elif NAME == "hyprctl":
    # Models Hyprland 0.56 under the Lua config: legacy dispatcher names fail with exit 7, and a Lua miss answers a warning with exit 0.
    current = window()
    if ARGS == ["-j", "clients"]:
        stale = json.loads((ROOT / "stale-clients.json").read_text()) if (ROOT / "stale-clients.json").exists() else []
        print(json.dumps(stale + ([current] if current else [])))
    elif ARGS == ["-j", "activewindow"]:
        print(json.dumps(current if current and current.get("focused") else {}))
    elif ARGS == ["-j", "monitors"]:
        print(json.dumps([dict(width=2560, height=1440, scale=1, focused=True)]))
    elif ARGS[:1] == ["dispatch"] and len(ARGS) == 3:
        print(f"')' expected near '{ARGS[1]}'", file=sys.stderr)
        raise SystemExit(7)
    elif ARGS[:1] == ["dispatch"] and len(ARGS) == 2:
        match = re.fullmatch(r'hl\.dsp\.(focus|window\.close)\(\{ window = "address:(0x[0-9a-fA-F]+)" \}\)', ARGS[1])
        if not match:
            print(f"')' expected near '{ARGS[1].split()[0]}'", file=sys.stderr)
            raise SystemExit(7)
        if not current or current["address"] != match[2]:
            print(f"warning: =[C]:-1: hl.{match[1]}: window not found")
        elif match[1] == "focus":
            current["focused"] = True
            publish_window(current)
            print("ok")
        else:
            with (ROOT / "closes.jsonl").open("a") as stream:
                stream.write(json.dumps(dict(token=current["token"], address=match[2])) + "\n")
            if current["class"] not in META.get("close_ignored", []):
                Path(current["quit_file"]).touch()
            print("ok")
    else:
        raise SystemExit(f"unexpected Hyprland command: {ARGS}")
elif NAME == "wtype":
    current = window()
    modifiers, held, key, index = [], [], None, 0
    while index < len(ARGS):
        if ARGS[index] == "-M":
            modifiers.append(ARGS[index + 1])
            index += 2
        elif ARGS[index] == "-m":
            modifiers.remove(ARGS[index + 1])
            index += 2
        elif ARGS[index] == "-k":
            key, held = ARGS[index + 1], list(modifiers)
            index += 2
        else:
            raise SystemExit(f"unexpected wtype argument: {ARGS[index]}")
    if not current or not current.get("focused"):
        raise SystemExit("wtype: no focused window")
    with (ROOT / "keys.jsonl").open("a") as stream:
        stream.write(json.dumps(dict(token=current["token"], key=key, modifiers=held)) + "\n")
    if key == "q" and held == ["ctrl"] and current["class"] in CHORD_QUITS:
        Path(current["quit_file"]).touch()
elif NAME == "grim":
    assert ARGS[0] == "-g"
    Path(ARGS[-1]).write_bytes(png())
else:
    details = META["binaries"][str(Path(sys.argv[0]).resolve())]
    if ARGS == ["--version"]:
        log("version-call", id=details["id"])
        if details.get("version_failure"):
            print(details["version_failure"], file=sys.stderr)
            raise SystemExit(1)
        print(details["version_output"])
        raise SystemExit(0)
    ident = details["id"]
    if ARGS == ["--backend"]:
        # The Flea 0.3.12 backend: one trace line per reported job on stderr, as AGENTS.md "Thumbnail trace" describes.
        assert ident == "flea-0312" and os.environ.get("FLEA_THUMB_TRACE") == "1"
        for row in range(16):
            print(f"flea: trace row={row} depth=2 queued=0.5 setup=1.0 child=20.0 after=0.5 total={22.0 + row}", file=sys.stderr, flush=True)
            time.sleep(0.004)
        raise SystemExit(0)
    assert os.environ["XDG_SESSION_TYPE"] == "wayland"
    fixture = Path(ARGS[ARGS.index("--left") + 1] if ident == "krusader" else ARGS[-1])
    home = Path(os.environ["HOME"])
    sample = home.parent
    quit_file = sample / "quit"
    token = os.environ["FLEA_B7_BENCH"]
    current = dict(address="0xb700", pid=os.getpid(), token=token, quit_file=str(quit_file),
                   at=[10, 20], size=[1200, 900], title=fixture.name, xwayland=False, **{"class": ident})
    publish_window(current)
    log("app-environment", id=ident, environment={key: os.environ.get(key) for key in
        ("HOME", "XDG_CONFIG_HOME", "XDG_CACHE_HOME", "XDG_DATA_HOME", "XDG_STATE_HOME", "XDG_RUNTIME_DIR", "WAYLAND_DISPLAY", "XDG_SESSION_TYPE", "QT_QPA_PLATFORM", "DBUS_SESSION_BUS_ADDRESS")})
    normal = ident in NORMAL_WRITERS
    root = Path(os.environ["XDG_CACHE_HOME"]) / "thumbnails"
    saves = ident not in META.get("silent", []) and (ident not in KIO_ONLY_WITH_ROOT or root.is_dir())
    cache = root / ("normal" if normal else "large")
    if saves:
        cache.mkdir(parents=True, exist_ok=True)
    if ident == "flea-0312":
        seam = os.environ["FLEA_BIN"]
        assert Path(seam).name == "flea-bin" and os.environ["FLEA_THUMB_TRACE"] == "1", seam
        backend = subprocess.Popen([seam, "--backend"])
    for path in sorted(fixture.iterdir()):
        if not saves or (ident == "nemo" and path.stat().st_size > NEMO_LIMIT):
            continue
        key = hashlib.md5(path.as_uri().encode()).hexdigest()
        (cache / f"{key}.png").write_bytes(png(128, 96, path.as_uri()) if normal else png(256, 192, path.as_uri()))
        if ident.startswith("strata"):
            event("Image")
    seen = 0
    deadline = time.monotonic() + 60
    while time.monotonic() < deadline and not quit_file.exists():
        commands = (ROOT / "keys.jsonl").read_text().splitlines() if (ROOT / "keys.jsonl").exists() else []
        for item in commands[seen:]:
            action = json.loads(item)
            if action["token"] == token and action["key"] in ("space", "Right") and ident.startswith("strata"):
                event("PreviewImage")
        seen = len(commands)
        time.sleep(0.01)
    if not quit_file.exists():
        raise SystemExit("stub app was not closed by owned input")
    raise SystemExit(META.get("exit_status", {}).get(ident, 0))
