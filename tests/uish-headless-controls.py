#!/usr/bin/env python3
"""Focused mounted-window controls, including deliberately failing twins."""
import argparse
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
from flea_transport import TransportError, endpoint, exchange
from flea_transport_lifecycle import identity
from flea_uish_headless import Host

checks = 0


def check(condition, message):
    global checks
    if not condition:
        raise AssertionError(message)
    checks += 1


def refused(action, expected, message):
    try:
        action()
    except expected:
        check(True, message)
    else:
        raise AssertionError(message)


def fixture(host, name, count=180):
    root = host.root / name
    root.mkdir(mode=0o700)
    for i in range(count):
        (root / f"f{i:03}.txt").write_text(f"row {i}\n")
    return root


def ready(window, count):
    window.wait("state", "ready")
    window.wait("listInFlight", "false")
    window.wait("total", str(count))
    window.wait("drawnCount", str(count))


def until(read, predicate, description):
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        value = read()
        if predicate(value):
            return value
        time.sleep(0.005)
    raise AssertionError(f"{description}: observed {value!r}")


def pane_state(window):
    replies = {name: window.call(name) for name in ("listInFlight", "listRequests", "keyDeliveryState")}
    return " ".join(f"{name}={reply.stdout.decode().strip() or reply.stderr.decode().strip()}"
                    for name, reply in replies.items())


def cursor_moves(window, deliver):
    before, requests = int(window.value("cursor")), window.value("listRequests")
    deliver()
    after = int(window.value("cursor"))
    moved = after == before + 1
    # A key that meets a same-folder re-read is queued until it lands, so a miss names the pane's state.
    check(moved, "" if moved else f"Window event did not move the IPC cursor: before={before} after={after} "
          f"listRequests before={requests} {pane_state(window)}")


def real_window(driver):
    with Host(driver) as host:
        window = host.open(fixture(host, "rows"))
        ready(window, 180)
        descriptor = window.input("describe")
        check(descriptor["draw_calls"] > 0, "Root never drew")
        check(descriptor["gpui_window"] >= 0, "missing GPUI window identity")
        # The same assertion must fail when a constant reader never observes delivered input.
        refused(lambda: cursor_moves(window, lambda: None), AssertionError, "constant Surface could pass cursor control")
        cursor_moves(window, lambda: window.input("key", key="down"))
        check(window.value("selectionCount") == "0", "plain movement changed the selection")
        window.input("key", key="v")
        check(window.value("selectedIndices") == "1", "selection key never reached the production Shell")
        window.input("key", key="v")
        check(window.value("selectedIndices") == "", "second selection key did not clear the mark")
        x, y = map(float, window.value("rowCentre", 2).split())
        window.input("mouse", phase="move", x=x, y=y)
        window.input("mouse", phase="down", x=x, y=y)
        window.input("mouse", phase="up", x=x, y=y)
        check(window.value("cursor") == "2" and window.value("selectedIndices") == "2",
              "Window mouse delivery did not select the painted row")
        before = float(window.value("viewContentY"))
        window.input("wheel", x=x, y=y, dx=0, dy=-310)
        until(lambda: float(window.value("viewContentY")), lambda value: value > before, "wheel did not scroll")
        check(True, "wheel moved the real viewport")
        check(not window.input("focus", active=False)["active"], "Window did not receive focus loss")
        check(window.input("focus", active=True)["active"], "Window did not receive focus activation")
        window.input("key", key="tab")
        check(window.value("focusView") == "rail", "Tab did not transfer focus to the rail")
        window.input("key", key="escape")
        check(window.value("focusView") == "list", "Escape did not return listing focus")
        observation = window.observe("cursor")
        check(observation["received_ns"] >= observation["started_ns"] > 0, "missing monotonic timestamps")
        check(observation["row_generation"]["list_requests"] >= 1, "missing production row generation")
        check(observation["window"] == window.id and len(observation["source"]["driver_sha256"]) == 64,
              "observation lost its window or source identity")
        capabilities = window.input("capabilities")
        check(capabilities["presented_frame_timing"] == "unsupported" and
              capabilities["hyprland_client"] == "unsupported" and capabilities["native_input"] == "unsupported",
              "headless host granted native observations")
        raster = capabilities["raster"]
        if raster["status"] == "unsupported":
            check(bool(raster["reason"]), "unavailable raster has no reason")
        else:
            check(Path(raster["path"]).stat().st_size == raster["width"] * raster["height"] * 4,
                  "raster does not contain real RGBA pixels")
        print("UISH_CAPABILITIES " + json.dumps(capabilities), flush=True)
        query = fixture(host, "query", 0)
        (query / "ré-notes.txt").write_text("notes\n")
        (query / "other.txt").write_text("other\n")
        search = host.open(query)
        ready(search, 2)
        search.input("key", key="/")
        filter_state = json.loads(search.value("keyDeliveryState"))
        if "filterQuery" not in filter_state:
            print("UISH_DEPENDENCY querypaste filterQuery unavailable: " + json.dumps(filter_state), flush=True)
        search.input("key", key="escape")
        search.input("key", key="f")
        search.input("text", text="ré-")
        search.input("paste", text="notes\nsecond line")
        query_state = json.loads(search.value("keyDeliveryState"))
        check(query_state["searchQuery"] == "ré-notes", f"text or clipboard paste bypassed the Window query path: {query_state!r}")
        until(lambda: search.value("total"), lambda value: value == "1", "pasted search did not reach the backend")
        search.input("key", key="ctrl-a")
        search.input("key", key="escape")
        search.close()
        window.close()
        replay = host.root / "replay.jsonl"
        commands = [dict(op="open", path="."), dict(op="wait", function="state", expected="ready"),
                    dict(op="read", function="cursor"), dict(op="close")]
        replay.write_text("".join(json.dumps(command) + "\n" for command in commands))
        result = subprocess.run([sys.executable, "-B", str(Path(__file__).parent / "lib/flea_uish_headless.py"),
                                 "--driver", str(driver), "--replay", str(replay)],
                                stdin=subprocess.DEVNULL, capture_output=True, timeout=20)
        check(result.returncode == 0, f"self-contained replay failed: {result.stderr.decode()}")
        values = [json.loads(line) for line in result.stdout.splitlines()]
        check(len(values) == 4 and values[-1] == {"closed": True}, "replay did not terminate after its own close")
        print(f"UISH_REAL_WINDOW PASS checks={checks} root={host.root} observation=" + json.dumps(observation), flush=True)


def rejects_stale_or_foreign(driver):
    with Host(driver) as host:
        first = host.open(fixture(host, "first", 5))
        second = host.open(fixture(host, "second", 7))
        ready(first, 5)
        ready(second, 7)
        check(first.id != second.id and first.descriptor["socket_identity"] != second.descriptor["socket_identity"],
              "two windows share an identity or IPC endpoint")
        first.input("key", key="down")
        check(first.value("cursor") == "1" and second.value("cursor") == "0", "input leaked into another window")
        refused(lambda: first.call("cursor", window=second.id), TransportError, "foreign read identity accepted")
        refused(lambda: first.input("key", key="down", window=second.id), TransportError, "foreign input identity accepted")
        refused(lambda: host.send(dict(op="key", window="foreign", key="down")), TransportError, "host accepted foreign window")
        refused(lambda: first.close(window=second.id), TransportError, "foreign teardown accepted")
        reply = first.call("path", target="fleapicker")
        check(reply.returncode == 3 and b"FLEA_IPC_NOT_PORTED fleapicker.path" in reply.stderr,
              "refused reader was replaced with a success")
        refused(lambda: first.call("quit"), TransportError, "production IPC mutation accepted")
        refused(lambda: first.input("invented"), TransportError, "malformed control operation accepted")
        refused(lambda: first.input("key", key="ctrl-invalid-extra"), TransportError, "malformed keystroke accepted")
        host.process.stdin.write(b"not-json\n")
        host.process.stdin.flush()
        malformed = host.receive(1)
        check(malformed["sequence"] is None and not malformed["result"]["ok"], "malformed JSON accepted")
        record = dict(host.record, **first.descriptor)
        record["root"] = str(host.root)
        with endpoint(record, timeout=1) as connection:
            connection.sendall(b"not-json\n")
            reply = json.loads(connection.recv(65536))
            check(reply["code"] == 2 and reply["stderr"], "production server accepted malformed IPC")
        started = time.monotonic()
        with endpoint(record, timeout=0.05) as connection:
            connection.sendall(b"{")
            refused(lambda: connection.recv(1), TimeoutError, "partial request had no bounded client timeout")
        elapsed = time.monotonic() - started
        check(0.02 <= elapsed < 0.5, f"client timeout outside its bound: {elapsed}")
        refused(lambda: first.call("cursor", timeout=float("inf")), TransportError, "unbounded timeout accepted")
        original = Path(first.descriptor["socket"])
        saved = original.with_name("saved.sock")
        original.rename(saved)
        replacement = socket.socket(socket.AF_UNIX)
        # Bind through the existing short runtime alias; the actual inode remains under the marked root.
        relative = original.relative_to(host.root / "runtime")
        replacement.bind(str(Path(first.descriptor["runtime"]) / relative))
        os.chmod(original, 0o600)
        try:
            refused(lambda: first.value("cursor"), TransportError, "replaced socket inode accepted")
        finally:
            replacement.close()
            original.rename(host.root / "kept-replacement.sock")
            saved.rename(original)
        first.close()
        refused(lambda: first.value("cursor"), TransportError, "closed window still readable")
        refused(lambda: exchange(record, ["call", "flea", "cursor"], 1), FileNotFoundError,
                "closed window socket still served production IPC")
        refused(lambda: host.send(dict(op="key", window=first.id, key="down")), TransportError, "closed host identity accepted")
        check(second.value("total") == "7", "closing one window closed its neighbour")
        second.close()
        check(not list((host.root / "runtime").rglob("*.sock")), "owned IPC socket survived teardown")
    refused(lambda: first.value("cursor"), TransportError, "exited host still readable")
    try:
        identity(host.process.pid)
    except FileNotFoundError:
        check(True, "host was reaped")
    else:
        raise AssertionError("owned host was not reaped")
    print(f"UISH_REJECTS_STALE_OR_FOREIGN PASS checks={checks} timeout_s={elapsed:.6f} root={host.root}", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--case", required=True, choices=("real-window", "rejects-stale-or-foreign"))
    parser.add_argument("--driver", required=True)
    args = parser.parse_args()
    {"real-window": real_window, "rejects-stale-or-foreign": rejects_stale_or_foreign}[args.case](args.driver)
