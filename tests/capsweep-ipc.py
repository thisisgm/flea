"""Measure whole-string IPC delivery using an isolated, bounded Quickshell process."""

import os
from pathlib import Path
import subprocess
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "tools"))
from flea_ipc import Client
IPC = Client("qs")
IPC.export()

PREVIEW_LOAD_SECONDS = 10
PREVIEW_POLL_SECONDS = 0.05

scratch, repo = map(Path, sys.argv[1:])
config = scratch / "ipc"
runtime = scratch / "runtime"
config.mkdir()
runtime.mkdir(mode=0o700)
(config / "shell.qml").symlink_to(repo / "tests/capsweep-ipc.qml")
(config / "Commons").symlink_to("/usr/share/omarchy/shell/Commons")
fixture = scratch / "large-readable.txt"
payload = "".join(f"Log row {index:05}: selected text loads only when requested.\n" for index in range(8000))
fixture.write_text(payload, encoding="utf-8")
markdown = scratch / "notes.md"
markdown.write_text("# Release notes\n", encoding="utf-8")
environment = os.environ | {
    "XDG_RUNTIME_DIR": str(runtime),
    "QT_QPA_PLATFORM": "offscreen",
    "QT_QUICK_BACKEND": "software",
    "QT_FORCE_STDERR_LOGGING": "1",
    "SWEEPIPC_UI": str(repo / "ui"),
    "SWEEPIPC_FILE": str(fixture),
    "SWEEPIPC_BYTES": str(fixture.stat().st_size),
    "SWEEPIPC_MARKDOWN": str(markdown),
}


def call(function, *arguments, target="sweepipc"):
    result = subprocess.run(
        ["timeout", "5", *IPC.command(), "-p", str(config), "call", target, function,
         *map(str, arguments)],
        env=environment, capture_output=True, timeout=6, check=True,
    )
    return result.stdout


def measure(size):
    scalar = call("length", size).strip()
    assert scalar == str(size).encode(), scalar
    delivered = call("whole", size).removesuffix(b"\n")
    if delivered and set(delivered) == {ord("x")}:
        count = len(delivered)
        status = "complete" if count == size else "truncated"
    else:
        assert b"QLocalSocket::PeerClosedError" in delivered, delivered[:200]
        count, status = 0, "peer-closed"
    assert call("ready").strip() == b"true", "server stopped after the large answer"
    measurements.append((size, status))
    print(f"CAPSWEEP_IPC requested={size} delivered={count} status={status} scalar={scalar.decode()}")
    return status == "complete"


measurements = []
with (scratch / "ipc.log").open("wb") as log:
    shell = subprocess.Popen(
        ["timeout", "45", "qs", "-p", str(config)],
        env=environment, stdout=log, stderr=subprocess.STDOUT,
    )
    try:
        deadline = time.monotonic() + PREVIEW_LOAD_SECONDS
        while True:
            try:
                if call("ready").strip() == b"true":
                    break
            except subprocess.CalledProcessError:
                pass
            assert shell.poll() is None, (scratch / "ipc.log").read_text()
            assert time.monotonic() < deadline, "offscreen IPC did not start"
            time.sleep(PREVIEW_POLL_SECONDS)
        complete, refused = 0, None
        for size in (1024, 65536, 131072, 262144, 456000, 1048576):
            if measure(size):
                if refused is None:
                    complete = size
            elif refused is None:
                refused = size
        if refused is not None:
            while refused - complete > 1:
                middle = (complete + refused) // 2
                if measure(middle):
                    complete = middle
                else:
                    refused = middle
            for _ in range(3):
                measure(complete)
                measure(refused)
                measure(456000)
            largest_complete = max(size for size, status in measurements if status == "complete")
            smallest_refused = min(size for size, status in measurements if status != "complete")
            cutoff = "variable" if largest_complete >= smallest_refused else "observed"
            print(f"CAPSWEEP_IPC cutoff={cutoff} largest-complete={largest_complete} smallest-refused={smallest_refused}")
        else:
            print("CAPSWEEP_IPC no-limit-observed-through=1048576")
        deadline = time.monotonic() + PREVIEW_LOAD_SECONDS
        while call("previewState", target="flea").strip() != b"ready":
            assert shell.poll() is None, (scratch / "ipc.log").read_text()
            assert time.monotonic() < deadline, (scratch / "ipc.log").read_text()
            time.sleep(PREVIEW_POLL_SECONDS)
        assert call("previewTextLength", target="flea").strip() == str(fixture.stat().st_size).encode()
        legacy = call("previewText", target="flea").removesuffix(b"\n")
        if legacy == payload.encode():
            status = "complete"
        elif legacy and payload.encode().startswith(legacy):
            status = "truncated"
        else:
            assert b"QLocalSocket::PeerClosedError" in legacy, legacy[:200]
            status = "peer-closed"
        print(f"CAPSWEEP_IPC legacy-preview-bytes={len(legacy) if status != 'peer-closed' else 0} status={status}")
        last_line = payload.splitlines(keepends=True)[-1]
        assert call("previewTextTail", len(last_line), target="flea").removesuffix(b"\n") == last_line.encode()
        checks = 2
        for size in (-1, 0, 1, 4096, len(payload), 2147483647):
            count = max(0, min(size, 4096))
            expected = payload[-count:] if count else ""
            assert call("previewTextTail", size, target="flea").removesuffix(b"\n") == expected.encode(), size
            checks += 1
        for function, expected in {
            "previewOpen": b"true", "previewKind": b"text", "previewPosition": b"0",
            "previewDuration": b"0", "previewPdfPage": b"-1", "previewPdfZoom": b"",
            "previewPdfFocus": b"-1", "previewExpanded": b"", "previewMediaLoaded": b"false",
            "previewArchiveNames": b"", "previewPictureRect": b"", "previewSliderCentre": b"",
            "previewMarkdownView": b"", "listingDropActive": b"false",
        }.items():
            assert call(function, target="flea").strip() == expected, function
            checks += 1
        for overlay in ("true", "false"):
            assert call("pdfState", overlay, target="flea").strip() == b"null", overlay
            checks += 1
        print(f"CAPSWEEP_IPC preview-checks={checks} failed=0 file-bytes={fixture.stat().st_size} tail-limit=4096")
        reader_checks = 0
        for active, enabled, correct_dest in ((False, True, True), (True, True, True),
                                               (True, False, True), (True, True, False)):
            assert call("floorState", str(active).lower(), str(enabled).lower(), str(correct_dest).lower()).strip() == b"true"
            expected = str(active and enabled and correct_dest).lower().encode()
            assert call("listingDropActive", target="flea").strip() == expected
            reader_checks += 1
        assert call("loadPreview").strip() == b"true"
        deadline = time.monotonic() + PREVIEW_LOAD_SECONDS
        while call("previewReady").strip() != b"true":
            assert shell.poll() is None, (scratch / "ipc.log").read_text()
            assert time.monotonic() < deadline, (scratch / "ipc.log").read_text()
            time.sleep(PREVIEW_POLL_SECONDS)
        for mode, active, is_markdown in (("rendered", True, True), ("source", True, True),
                                         ("source", False, True), ("source", True, False)):
            assert call("markdown", mode, str(active).lower(), str(is_markdown).lower()).strip() == b"true"
            expected = mode.encode() if active and is_markdown else b""
            assert call("previewMarkdownView", target="flea").strip() == expected
            reader_checks += 1
        print(f"CAPSWEEP_IPC capture-reader-checks={reader_checks} failed=0")
    except Exception:
        print((scratch / "ipc.log").read_text())
        raise
    finally:
        shell.terminate()
        shell.wait(timeout=6)
