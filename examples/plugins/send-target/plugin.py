#!/usr/bin/env python3
"""Protocol 1 send fixture. It acknowledges selections without copying files."""
import json
import os
import sys


for line in iter(lambda: sys.stdin.buffer.readline(65538), b""):
    if len(line) > 65536:
        break
    try:
        request = json.loads(line)
        operation = request.get("op")
        if operation == "hello":
            reply = {"op": "ready", "protocolVersion": 1}
        elif operation == "targets":
            reply = {"op": "targets", "targets": [
                {"id": "receiver", "label": "Local Receiver", "state": "ready"},
                {"id": "offline", "label": "Offline Receiver", "state": "offline"}
            ]}
        elif operation == "send":
            paths = request.get("paths", [])
            if request.get("target") != "receiver" or not paths or len(paths) > 512:
                raise ValueError("Choose the ready receiver and at least one file.")
            if any(not isinstance(path, str) or not os.path.isabs(path) for path in paths):
                raise ValueError("Expected absolute selected paths.")
            print(json.dumps({"op": "progress", "done": len(paths), "total": len(paths),
                              "message": "Receiving selection."}), flush=True)
            reply = {"op": "done", "message": "Selection received."}
        elif operation == "settings":
            reply = {"op": "done"}
        else:
            reply = {"op": "error", "message": "Unsupported action."}
    except (ValueError, TypeError) as error:
        reply = {"op": "error", "message": str(error)[:4096]}
    print(json.dumps(reply), flush=True)
