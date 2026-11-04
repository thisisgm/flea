#!/usr/bin/env python3
"""Exercise two independent Flea backends against an isolated X11 display."""

import json
import os
import queue
import subprocess
import sys
import threading


def backend(binary):
    proc = subprocess.Popen(
        [binary, "--backend"], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
        stderr=subprocess.PIPE, text=True, bufsize=1,
    )
    lines = queue.Queue()
    threading.Thread(target=lambda: [lines.put(json.loads(line)) for line in proc.stdout], daemon=True).start()
    return proc, lines


def send(proc, **request):
    proc.stdin.write(json.dumps(request) + "\n")
    proc.stdin.flush()


def receive(lines, operation):
    while True:
        message = lines.get(timeout=5)
        if message.get("t") == "clip" and message.get("op") == operation:
            return message


def main():
    if not os.environ.get("DISPLAY") or os.environ.get("XDG_SESSION_TYPE") != "x11":
        raise SystemExit("run with DISPLAY set to an isolated X server and XDG_SESSION_TYPE=x11")
    binary = sys.argv[1] if len(sys.argv) > 1 else "./target/debug/flea"
    a, a_lines = backend(binary)
    b, b_lines = backend(binary)
    try:
        send(a, c="clipWatch")
        send(b, c="clipWatch")
        receive(a_lines, "changed")
        receive(b_lines, "changed")

        paths = ["/tmp/flea A", "/tmp/flea B"]
        send(a, c="clipSet", op="cut", paths=paths)
        first = receive(a_lines, "set")
        assert first["ok"] and first["token"].startswith("x11:"), first
        changed = receive(b_lines, "changed")
        assert (changed["clip"], changed["paths"], changed["token"]) == (
            "cut", paths, first["token"]), changed
        send(b, c="clipGet")
        got = receive(b_lines, "get")
        assert (got["clip"], got["paths"]) == ("cut", paths), got

        send(b, c="clipSet", op="copy", paths=["/tmp/newer"])
        second = receive(b_lines, "set")
        assert second["ok"], second
        newer_event = receive(a_lines, "changed")
        if newer_event["paths"] == paths:
            newer_event = receive(a_lines, "changed")
        assert (newer_event["clip"], newer_event["paths"]) == ("copy", ["/tmp/newer"]), newer_event
        send(a, c="clipClear", token=first["token"])
        stale = receive(a_lines, "clear")
        assert stale["ok"] and not stale["cleared"], stale
        send(a, c="clipGet")
        newer = receive(a_lines, "get")
        assert (newer["clip"], newer["paths"]) == ("copy", ["/tmp/newer"]), newer

        # A second publication by the same backend can reuse the X11 window ID.
        send(b, c="clipSet", op="copy", paths=["/tmp/newest"])
        third = receive(b_lines, "set")
        assert third["ok"] and third["token"] != second["token"], third
        latest_event = receive(a_lines, "changed")
        assert latest_event["paths"] == ["/tmp/newest"], latest_event
        send(a, c="clipClear", token=second["token"])
        stale_again = receive(a_lines, "clear")
        assert stale_again["ok"] and not stale_again["cleared"], stale_again
        send(a, c="clipClear", token=third["token"])
        cleared = receive(a_lines, "clear")
        assert cleared["ok"] and cleared["cleared"], cleared
        send(b, c="clipGet")
        empty = receive(b_lines, "get")
        assert empty["clip"] == "none" and not empty["paths"], empty
        print("x11 clipboard: cross-process cut, change, stale-clear guard, copy and clear passed")
    finally:
        for proc in (a, b):
            if proc.poll() is None:
                send(proc, c="quit")
                try:
                    proc.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait()


if __name__ == "__main__":
    main()
