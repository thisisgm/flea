"""Private sample state and worker timing in the shared Hyprland session."""

import datetime
import hashlib
import json
import os
from pathlib import Path
import pwd
import re
import shlex
import signal
import statistics
import struct
import subprocess
import tempfile
import time

import versions as version_probe
import gpui
import cgroup_cpu

ANSI = re.compile(r"\x1b\[[0-9;]*m")
# Sample input: '2026-10-20T08:00:00.100Z browser worker completed operation=Image queue_ms=80 elapsed_ms=20'.
EVENT = re.compile(r"(?P<at>\d{4}-\d\d-\d\dT[\d:.]+Z).*browser worker completed.*operation=(?P<kind>Image|PreviewImage)\b.*queue_ms=(?P<queue>\d+).*elapsed_ms=(?P<elapsed>\d+)")


# The freedesktop thumbnail sizes; a rival writes the one its own view asks for.
SIZES = {128, 256, 512, 1024}
FLAVORS = ("normal", "large", "x-large", "xx-large")
# Sample input: 'flea: trace row=0 depth=2 queued=0.5 setup=1.0 child=20.0 after=0.5 total=22.0'.
TRACE = re.compile(r"flea: trace row=(?P<row>\d+) depth=\d+ queued=(?P<queued>[\d.]+) setup=[\d.]+ child=[\d.]+ after=[\d.]+ total=(?P<total>[\d.]+)")
# Entrants whose own quit chord is a documented binding; every other entrant gets the compositor close request, the same as its close button.
QUIT_CHORD = {"krusader": "ctrl+q", "dolphin": "ctrl+q", "nautilus": "ctrl+q"}
QUIT_WAIT = 5
DECLINE_WAIT = 8


def utc_seconds(value):
    parsed = datetime.datetime.fromisoformat(value.replace("Z", "+00:00"))
    if parsed.tzinfo is None:
        raise ValueError("timestamp needs a timezone")
    return parsed.timestamp()


def worker_events(text):
    events = []
    for line in ANSI.sub("", text).splitlines():
        match = EVENT.search(line)
        if match:
            event = match.groupdict()
            end = utc_seconds(event["at"]) * 1000
            elapsed, queue = int(event["elapsed"]), int(event["queue"])
            events.append(dict(kind=event["kind"], end_ms=end, start_ms=end - elapsed - queue, elapsed_ms=elapsed))
    return events


def worker_metrics(events, verified):
    thumbnails = [event for event in events if event["kind"] == "Image"]
    previews = [event for event in events if event["kind"] == "PreviewImage"]
    output = dict(all_thumbnails_ms=None, per_thumbnail_ms=None, preview_800_ms=None)
    if len(thumbnails) == verified == 16:
        output["all_thumbnails_ms"] = max(event["end_ms"] for event in thumbnails) - min(event["start_ms"] for event in thumbnails)
        output["per_thumbnail_ms"] = statistics.median(event["elapsed_ms"] for event in thumbnails)
    if len(previews) == 16:
        output["preview_worker_ms_unverified"] = statistics.median(event["elapsed_ms"] for event in previews)
    return output


# Sample input: '2810.000,msec,task-clock:u,1,100.00,,'.
def perf_user(text):
    for line in text.splitlines():
        fields = line.split(",")
        if len(fields) >= 3 and fields[2].strip() == "task-clock:u":
            try:
                value = float(fields[0])
            except ValueError:
                return None
            return value / 1000 if value > 0 and fields[1].strip() == "msec" else None
    return None


class TraceTail:
    """Reads the backend trace file as it grows and stamps each line with the time this poll saw it."""

    def __init__(self, path):
        self.path, self.offset, self.partial, self.jobs = path, 0, b"", []

    def poll(self):
        try:
            with self.path.open("rb") as stream:
                stream.seek(self.offset)
                data = stream.read()
        except FileNotFoundError:
            return
        seen = time.monotonic() * 1000
        self.offset += len(data)
        *lines, self.partial = (self.partial + data).split(b"\n")
        for line in lines:
            match = TRACE.search(line.decode(errors="replace"))
            if match:
                self.jobs.append(dict(row=int(match["row"]), queued=float(match["queued"]), total=float(match["total"]), seen_ms=seen))


def trace_metrics(jobs, verified):
    """Per-job times from the installed 0.3.12 trace. The line carries no wall clock, so the batch starts at the earliest
    seen-time minus total and ends at the last seen-time; the error is one poll interval."""
    output = dict(all_thumbnails_ms=None, per_thumbnail_ms=None, preview_800_ms=None)
    if len(jobs) == verified == 16 and len({job["row"] for job in jobs}) == 16:
        output["all_thumbnails_ms"] = max(job["seen_ms"] for job in jobs) - min(job["seen_ms"] - job["total"] for job in jobs)
        output["per_thumbnail_ms"] = statistics.median(job["total"] - job["queued"] for job in jobs)
    return output


def expected_stems(fixture):
    return {hashlib.md5(path.as_uri().encode(), usedforsecurity=False).hexdigest(): path.as_uri() for path in fixture.iterdir()}


def thumb_uri(data):
    offset = 8
    while offset + 12 <= len(data):
        length, kind = struct.unpack(">I4s", data[offset:offset + 8])
        if kind == b"tEXt":
            key, _, value = data[offset + 8:offset + 8 + length].partition(b"\0")
            if key == b"Thumb::URI":
                return value.decode(errors="replace")
        offset += 12 + length
    return None


def complete_png(path):
    try:
        with path.open("rb") as stream:
            head = stream.read(8)
            stream.seek(-12, os.SEEK_END)
            return head == b"\x89PNG\r\n\x1a\n" and stream.read(12)[4:8] == b"IEND"
    except OSError:
        return False


def settled_count(cache, stems):
    """Cheap per-poll count of complete cache PNGs for the fixture; the full decode check runs once at the end."""
    found = set()
    for flavor in FLAVORS:
        try:
            for entry in os.scandir(cache / "thumbnails" / flavor):
                if entry.name.endswith(".png") and entry.name[:-4] in stems and not entry.is_symlink() and complete_png(Path(entry.path)):
                    found.add(entry.name[:-4])
        except FileNotFoundError:
            continue
    return len(found)


def cache_inventory(cache):
    """What an entrant wrote, by directory and edge, so a count of zero still explains itself."""
    rows = {}
    for path in sorted(cache.glob("thumbnails/*/*.png")):
        try:
            width, height = struct.unpack(">II", path.read_bytes()[16:24])
        except (OSError, struct.error):
            width = height = 0
        key = f"{path.parent.name}/{max(width, height)}"
        rows[key] = rows.get(key, 0) + 1
    return rows


def verified_cache(cache, fixture, edge=256):
    expected = expected_stems(fixture)
    good = set()
    for path in cache.glob("thumbnails/*/*.png"):
        if path.is_symlink() or path.stem not in expected or path.parent.name == "fail":
            continue
        data = path.read_bytes()
        if data[:8] != b"\x89PNG\r\n\x1a\n":
            continue
        try:
            width, height = struct.unpack(">II", data[16:24])
            if max(width, height) not in ({edge} if edge else SIZES) or min(width, height) < 1:
                continue
            if thumb_uri(data) not in (None, expected[path.stem]):
                continue
            subprocess.run(["magick", "-regard-warnings", str(path), "null:"], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        except (struct.error, subprocess.SubprocessError):
            continue
        good.add(path.stem)
    return good


def stop(process):
    if process is None or process.poll() is not None:
        return
    for sig in (signal.SIGTERM, signal.SIGKILL):
        try:
            os.killpg(process.pid, sig)
        except ProcessLookupError:
            pass
        try:
            process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            continue
        break


def environment(root, display=None):
    value = dict(PATH=os.environ.get("PATH", "/usr/local/bin:/usr/bin:/bin"), HOME=str(root / "home"), LANG="C.UTF-8", LC_ALL="C.UTF-8", TZ="UTC",
                 XDG_CONFIG_HOME=str(root / "config"), XDG_CACHE_HOME=str(root / "cache"), XDG_DATA_HOME=str(root / "data"),
                 XDG_STATE_HOME=str(root / "state"), XDG_RUNTIME_DIR="/run/flea-b7" if display else str(root / "runtime"), TMPDIR=str(root / "tmp"),
                 USER=pwd.getpwuid(os.getuid()).pw_name, LOGNAME=pwd.getpwuid(os.getuid()).pw_name, SHELL="/bin/bash", XDG_DATA_DIRS="/usr/local/share:/usr/share",
                 GDK_BACKEND="wayland", GDK_SCALE="1", GDK_DPI_SCALE="1", QT_QPA_PLATFORM="wayland" if display else "offscreen",
                 XDG_CURRENT_DESKTOP="Hyprland", GIO_USE_VFS="local", GTK_USE_PORTAL="0", NO_AT_BRIDGE="1",
                 QT_SCALE_FACTOR="1", QT_AUTO_SCREEN_SCALE_FACTOR="0")
    if display:
        value.update(WAYLAND_DISPLAY="wayland-0", XDG_SESSION_TYPE="wayland")
    return value


def directories(root):
    for name in ("home", "config", "cache", "data", "state", "runtime", "tmp"):
        (root / name).mkdir(mode=0o700)
    (root / ".flea-test-sandbox").touch()


def sandbox(root, display=None):
    arguments = ["bwrap", "--die-with-parent", "--unshare-pid", "--unshare-net", "--ro-bind", "/", "/",
                 "--bind", str(root), str(root), "--proc", "/proc", "--dev", "/dev"]
    if display:
        arguments.extend(["--tmpfs", "/run", "--bind", str(root / "runtime"), "/run/flea-b7",
                          "--ro-bind", str(display.socket), "/run/flea-b7/wayland-0"])
        if Path("/dev/dri").is_dir():
            arguments.extend(["--dev-bind", "/dev/dri", "/dev/dri"])
    return arguments


# KIO needs an existing thumbnail root to permit cache writes; source references are in flea-bench-thumbs.md.
KIO_ENTRANTS = ("krusader", "dolphin")

# Nemo's default refuses images over 1 MiB unless a thumbnail already exists.
DEFAULT_DECLINES = {
    "nemo": dict(setting="org.nemo.preferences thumbnail-limit", limit_bytes=1048576,
                 source="libnemo-private/org.nemo.gschema.xml:267-268 (default 1048576), libnemo-private/nemo-file.c:4491-4493"),
}


def prepare_rival_state(root, ident):
    """The state a user's account already has when the rival first runs; no setting of the rival is written."""
    if ident in KIO_ENTRANTS:
        (root / "cache/thumbnails").mkdir(mode=0o700)


def declined_by_default(ident, fixture):
    """The rival's default decline when every fixture file is over its limit, else None."""
    policy = DEFAULT_DECLINES.get(ident)
    sizes = [path.stat().st_size for path in fixture.iterdir()]
    if policy and sizes and min(sizes) > policy["limit_bytes"]:
        return dict(policy, smallest_fixture_bytes=min(sizes))
    return None


def write_trace_seam(root, binary):
    """FLEA_BIN is the product's own dev seam for the backend path. The backend's stderr is discarded by Quickshell, so
    this exec passthrough sends it, and only it, to a file; every other child runs untouched."""
    seam = root / "flea-bin"
    executable = shlex.quote(str(binary))
    trace = shlex.quote(str(root / "trace.log"))
    seam.write_text(f'#!/bin/sh\ncase "$1" in\n  --backend) exec {executable} "$@" 2>>{trace} ;;\n  *) exec {executable} "$@" ;;\nesac\n')
    seam.chmod(0o700)
    return seam


def exited(process, seconds):
    try:
        status = process.wait(timeout=seconds)
        if status != 0:
            raise ValueError(f"entrant exited with status {status}; sample incomplete")
        return True
    except subprocess.TimeoutExpired:
        return False


def quit_entrant(display, window, token, process, ident):
    """Quit through the entrant's own action: its documented chord, else the compositor close request. If it is still
    running after QUIT_WAIT seconds the other action follows, then the sample is called incomplete."""
    chord = QUIT_CHORD.get(ident)
    first, second = ("chord", "close") if chord else ("close", "chord")
    methods = []
    for name in (first, second):
        try:
            if name == "chord":
                display.key(window, token, chord or "ctrl+q")
            else:
                display.close(window, token)
            methods.append(name)
        except ValueError:
            if exited(process, 0):
                break
            continue
        if exited(process, QUIT_WAIT):
            return "+".join(methods)
    if exited(process, 0):
        return "+".join(methods)
    raise ValueError(f"entrant did not quit within {QUIT_WAIT * 2} s ({'+'.join(methods) or 'no quit action reached it'}); CPU sample incomplete")


def run_one(repo, parent, display, fixture, entrant, round_number, timeout):
    ident = entrant["id"]
    root = Path(tempfile.mkdtemp(prefix=f"{fixture.name}-{ident}-{round_number}-", dir=parent))
    directories(root)
    prefix = sandbox(root, display)
    env = environment(root, display)
    token = root.name
    env["FLEA_B7_BENCH"] = token
    if ident == "flea" and os.environ.get("FLEA_B7_DELAY_MS"):
        env["FLEA_B7_DELAY_MS"] = os.environ["FLEA_B7_DELAY_MS"]
    binary = entrant["binary"]
    from inventory import digest
    if digest(binary) != entrant["sha256"]:
        raise ValueError(f"binary checksum differs from inventory: {ident}")
    if ident == "flea" and digest(Path(binary).with_name("flea-gui")) != entrant.get("gui_sha256"):
        raise ValueError("GPUI sibling checksum differs from candidate inventory")
    identity = version_probe.probe(ident, binary, prefix, env, root)
    version_probe.check(entrant, identity)
    version_output = identity["version_output"]
    prepare_rival_state(root, ident)
    if ident in KIO_ENTRANTS:
        # Log lines only, so the next sample explains a missing cache write; no rival behavior changes.
        env["QT_LOGGING_RULES"] = "kf.kio.gui.debug=true;kf.kio.gui.warning=true"
    if ident.startswith("strata"):
        config = root / "config/strata"
        config.mkdir()
        (config / "settings.toml").write_text('browser_mode = "icons"\nthumbnail_workers = 4\nsingle_click_previews = false\nicons_thumbnail_size = 64\ncheck_for_updates = false\nreduce_motion = true\n')
        (config / "portal-opt-in-v1").write_text("1\n")
        env.update(RUST_LOG="strata=info,strata::sandbox::browser=debug", STRATA_THUMBNAIL_WORKERS="4")
    elif ident == "krusader":
        (root / "right").mkdir()
        (root / "config/krusaderrc").write_text("First Time=false\nConfig Version=1\n\n[ViewerModule]\nFirstRun=false\n\n[Look&Feel]\nDefault Panel Type=0\nSingle Instance Mode=false\nShow splashscreen=false\nWarn On Exit=false\n\n[Startup]\nLeft Side Is Active=true\n\n[KrInterDetailedView]\nWith Icons=true\nShowPreviews=true\n\n[KrInterBriefView]\nWith Icons=true\nShowPreviews=true\n")
    elif ident.startswith("flea"):
        env["FLEA_BIN"] = binary
        env["FLEA_FRONTEND"] = "gpui" if ident == "flea" else "qml"
        if ident == "flea-0312":
            env["FLEA_THUMB_TRACE"] = "1"
            env["FLEA_BIN"] = str(write_trace_seam(root, binary))
            ui = Path(binary).parent.parent.parent / "ui"
            if (ui / "boot/shell.qml").is_file():
                env["FLEA_UI"] = str(ui)
    argv = [binary, "--gui", str(fixture)] if ident.startswith("flea") else [binary, str(fixture)]
    if ident == "krusader":
        argv = [binary, "--left", str(fixture), "--right", str(root / "right")]
    keymap = root / "fixture.keys"
    subprocess.run([str(repo / "tools/flea-bench-keys"), "map", str(fixture), str(keymap)], check=True)
    perf = ["perf", "stat", "-x,", "-o", str(root / "cpu.csv"), "-e", "task-clock:u", "--"]
    with (root / "cpu-probe.log").open("xb") as log:
        supported = ident != "flea" and subprocess.run(prefix + ["perf", "stat", "-e", "task-clock:u", "--", "true"], env=env, cwd=root, stdout=log, stderr=log, timeout=5).returncode == 0
    command = (perf if supported else []) + ["dbus-run-session", "--", *argv]
    scope = cgroup_cpu.Scope(root) if ident == "flea" else None
    reader = gpui.Receipts(root / "app.log", token, fixture) if scope else None
    process = None
    error = ""
    events = []
    settle = None
    quit_method = None
    stems = expected_stems(fixture)
    declined = declined_by_default(ident, fixture)
    tail = TraceTail(root / "trace.log") if ident == "flea-0312" else None
    try:
        with (root / "app.log").open("xb") as log:
            launch = scope.command(["dbus-run-session", "--", *argv], env, prefix) if scope else prefix + command
            process = subprocess.Popen(launch, env=None if scope else env, cwd=root, stdout=log, stderr=log, start_new_session=True)
        window = display.wait_window(token, process, timeout)
        mapped = time.monotonic()
        (root / "window.json").write_text(json.dumps(dict(token=token, launcher_pid=process.pid, **{k: window.get(k) for k in ("address", "pid", "class", "title", "at", "size")})) + "\n")
        display.focus(window, token)
        if ident != "krusader":
            key = "ctrl+3" if ident.startswith("flea") else "ctrl+2" if ident.startswith("strata") else "ctrl+1"
            display.key(window, token, key)
        # A rival that declines by its own default writes nothing, so it gets a short look rather than the full wait.
        deadline = time.monotonic() + (min(timeout, DECLINE_WAIT) if declined else timeout)
        while time.monotonic() < deadline and process.poll() is None:
            if reader:
                reader.poll()
            if tail:
                tail.poll()
            if ident.startswith("strata"):
                events = worker_events((root / "app.log").read_text(errors="replace"))
            if settle is None and settled_count(root / "cache", stems) >= 16:
                settle = time.monotonic() - mapped
            clock_done = reader.complete("thumbnail") if reader else sum(e["kind"] == "Image" for e in events) >= 16 if ident.startswith("strata") else len(tail.jobs) >= 16 if tail else True
            if clock_done and settle is not None:
                break
            time.sleep(0.01)
        if reader and not reader.complete("thumbnail"):
            raise ValueError("GPUI did not present all 16 decoded thumbnails")
        before = time.monotonic_ns()
        display.screenshot(root, window, token, "thumbnails")
        if reader:
            gpui.capture_receipt(root, "thumbnails", reader, before, time.monotonic_ns())
        if (reader or ident.startswith("strata")) and len(verified_cache(root / "cache", fixture)) == 16:
            display.key(window, token, "Home", "space")
            for sample in range(16):
                deadline = time.monotonic() + timeout
                while time.monotonic() < deadline and process.poll() is None:
                    if reader:
                        reader.poll()
                        if len(reader.outputs("preview")) >= sample + 1:
                            break
                    events = worker_events((root / "app.log").read_text(errors="replace"))
                    if sum(e["kind"] == "PreviewImage" for e in events) >= sample + 1:
                        break
                    time.sleep(0.05)
                else:
                    raise ValueError("preview worker did not finish")
                before = time.monotonic_ns()
                display.screenshot(root, window, token, f"preview-{sample:02d}")
                if reader:
                    gpui.capture_receipt(root, f"preview-{sample:02d}", reader, before, time.monotonic_ns())
                if sample < 15:
                    display.key(window, token, "Right")
        quit_method = quit_entrant(display, window, token, process, ident)
    except (ValueError, OSError, subprocess.SubprocessError) as failure:
        error = str(failure)
    finally:
        stop(process)
        if scope:
            scope.stop()
    events = worker_events((root / "app.log").read_text(errors="replace"))
    if tail:
        tail.poll()
    verified = len(verified_cache(root / "cache", fixture, 256))
    if reader:
        try:
            reader.poll()
        except ValueError as failure:
            error = error or str(failure)
        metrics = reader.metrics(verified)
        method = "GPUI decoded image paint and exact CLOCK_MONOTONIC compositor presentation; thumbed.ms worker duration"
    elif ident.startswith("strata"):
        metrics = worker_metrics(events, verified)
        method = "worker-clock (browser worker completion log)"
    elif tail:
        metrics = trace_metrics(tail.jobs, verified)
        method = "flea thumb trace (FLEA_THUMB_TRACE through the FLEA_BIN seam, seen-time polled every 10 ms)"
    else:
        metrics = dict(all_thumbnails_ms=None, per_thumbnail_ms=None, preview_800_ms=None)
        method = "cache-settle (no per-job completion log)"
    if error:
        metrics = {key: None for key in metrics}
    cpu = perf_user((root / "cpu.csv").read_text()) if (root / "cpu.csv").exists() else None
    reasons = [error] if error else []
    if not supported and not scope:
        reasons.append("perf user counter unavailable; app still ran without CPU collection")
    if tail and len(tail.jobs) != 16:
        reasons.append(f"{len(tail.jobs)} trace lines, expected 16; trace is per reported job")
    outcome = None
    if declined and verified == 0:
        # The rival's own default refused every file, which is its result at that setting, not a failed sample.
        outcome = dict(declined, rule="declined by the rival's default policy; no setting was changed")
        reasons.append(f"declined: {declined['setting']} default {declined['limit_bytes']} bytes, smallest fixture file "
                       f"{declined['smallest_fixture_bytes']} bytes ({declined['source']})")
    elif verified != 16:
        reasons.append(f"{verified}/16 valid fixture cache entries")
        log_text = (root / "app.log").read_text(errors="replace").splitlines()
        reasons.append("app.log tail: " + " | ".join(line.strip()[:160] for line in log_text[-6:]))
    if not reader:
        reasons.append("800px successful preview output needs verification; screenshots and raw worker timings retained")
    elif not reader.complete("preview"):
        reasons.append("GPUI did not present all 16 successful 800 px previews")
    cgroup_user = scope.result() if scope and not error and reader.complete("preview") else None
    if scope and cgroup_user is None:
        reasons.append("complete whole-cgroup user CPU receipt unavailable")
    settle_ms = round(settle * 1000, 1) if settle is not None and verified == 16 and not error else None
    return dict(**metrics, all_thumbnails_settle_ms=settle_ms, thumbnail_method=method, quit_method=quit_method, cache_written=cache_inventory(root / "cache"), mutant=bool(env.get("FLEA_B7_DELAY_MS")),
                user_cpu_s=cgroup_user, user_cpu_s_diagnostic=cpu, verified=verified, declined=outcome, version_output=version_output, version_identity=identity,
                evidence=str(root), reasons=reasons, argv=argv, cpu_method="cgroup v2 cpu.stat user_usec, launch to graceful exit and all helper exits" if scope else "inherited perf task-clock:u, including private session bus")
