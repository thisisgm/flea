"""Native-boundary doubles for exercising the real orchestration gates."""
from contextlib import ExitStack
import base64
import json
import os
from pathlib import Path
import tempfile
from types import SimpleNamespace
from unittest.mock import patch

from . import driver, fixtures
from .evidence import MARKER, Owned
from .references import Board
from .protocol import capabilities


def media_transcript(case, graphics, kitty=True, sixel=False):
    rows = []
    for enabled, query, reply in ((kitty, b"\x1b_Gi=7,a=q;AAAA\x1b\\", b"\x1b_Gi=7;OK\x1b\\"),
                                   (sixel, b"\x1b[c", b"\x1b[?1;4c")):
        if enabled:
            rows += [(10, "output", query), (21, "candidate-input", reply)]
    rows += [(r["ns"], "output", r["data"].encode()) for r in graphics]
    rows.sort(key=lambda row: row[0])
    (case / "pty.log").write_text("\n".join(json.dumps({"ns": ns, "direction": direction,
        "base64": base64.b64encode(data).decode()}) for ns, direction, data in rows))
    (case / "graphics.log").write_text("\n".join(json.dumps(r) for r in graphics))
    return capabilities(case / "pty.log", "candidate-input")


def board_file(root, name="tui-list-text", text="A", decoration=""):
    boards = root / "boards"
    boards.mkdir(exist_ok=True)
    page = boards / (name + ".html")
    page.write_text('<style>.th{--fg:#c6b395;--bg:#1c120d}</style>'
                    f'<b style="left:4px;top:9px;{decoration}">{text}</b>')
    page.with_suffix(".png").write_bytes(b"reference identity")
    return Board(page)


def pair(root, preset="default", state_id="tui-list-text", exit_code=0, wrong_kind=False, references=None, frame=None, validate=None,
         launches=None, adapters=None, capture=None, seams=None):
    # Each terminal launch appends its arguments here, so a caller can tell whether the pair launched at all.
    launches = [] if launches is None else launches
    # Each Adapter construction appends its arguments here: terminal name, case directory, window identity.
    adapters = [] if adapters is None else adapters
    if references:
        (root / "boards").mkdir()
        for extension in ("html", "png"):
            name = state_id + "." + extension
            (root / "boards" / name).write_bytes((references / "boards" / name).read_bytes())
        board = Board(root / "boards" / (state_id + ".html"))
    else:
        board = board_file(root, state_id, "Plain text document" if state_id == "tui-properties" else "A")
    expected = board.cells()
    grid = [[dict(expected[(x, y)], dim=False, reverse=False, underline=False) for x in range(130)] for y in range(31)]
    if wrong_kind:
        for row in grid:
            start = "".join(cell["ch"] for cell in row).find("Plain text document")
            if start >= 0:
                for x, char in enumerate("File".ljust(len("Plain text document")), start):
                    row[x]["ch"] = char
                break
        else:
            raise AssertionError("Kind control could not find the MIME kind cells")
    snapshot = {"columns": 130, "rows": 31, "grid": grid, "pty": [31, 130, 1040, 682], "unsupported": []}
    case = root / "foot" / state_id / "primary"
    fx = case / "fx"
    # Sample frame: ((width, height, rgba), client size, grid rect); the default grid fills its client, so it has no padding.
    image, size, grid = frame or ((1040, 682, bytes([0, 0, 0, 255]) * (1040 * 682)), [1040, 682], [0, 0, 1040, 682])
    client = {"at": [0, 0], "size": size}

    def capture_double(path):
        path.write_bytes(b"native boundary double")
        return image

    def close():
        (case / "relay-exit.json").write_text(json.dumps({"candidate_exit": 0, "termios_restored": True,
                                                          "relay_termios_restored": True}))
        receipt = {"exit_code": exit_code}
        (case / "launch-exit.json").write_text(json.dumps(receipt))
        return receipt

    def configure():
        config = case / "terminal.conf"
        config.write_text("private terminal configuration\n")
        return {"path": str(config), "font": "JetBrainsMono Nerd Font", "font_size": 14}

    def build(*args):
        # The fixture lives on tmpfs, as the real one does, so only the driver's retain brings it into the case.
        root = Path(tempfile.mkdtemp(prefix="flea-battery-fx.", dir=fixtures.TMPFS))
        (root / MARKER).write_text(f"battery fixture {os.getpid()}\n")
        (root / "fx").mkdir()
        receipt = {"family": "work", "manifest_sha256": "approved-fixture"}
        (case / "fixtures.json").write_text(json.dumps(receipt))
        return root / "fx", receipt

    def launch(*args, **kwargs):
        launches.append(args)
        for name in ("relay-spec.json", "launch-spec.json"):
            (case / name).write_text(json.dumps({"argv": ["boundary-double"]}))
        (case / "launch-start.json").write_text(json.dumps({"pid": 2147483647, "start_ns": 1}))
        (case / "relay-start.json").write_text(json.dumps({"stdin_tty": True, "stdout_tty": True}))
        for name in ("pty.log", "graphics.log"):
            (case / name).write_text("")

    def pixels(*args):
        for name in ("grid-crop.png", "diff.png", "overlay.png"):
            (case / name).write_bytes(b"native boundary double")
        return {"status": "REVIEW"}

    terminal = SimpleNamespace(client=client, launched=True, launch=launch, close=close, snapshot=lambda: snapshot)
    adapter = SimpleNamespace(identity="owned", preflight=lambda *a: {}, configure=configure, environment=lambda: {},
                              validate=validate or (lambda *a: {"status": "PASS"}))
    display = SimpleNamespace(monitor={"x": 0, "y": 0}, original_rounding=0, capture=capture or capture_double, client=lambda *a: client)
    args = SimpleNamespace(preset=preset, fixtures=None, binary=Path("/candidate"), boards=root, timeout=1)
    state = {"id": state_id, "family": "work", "requires": [], "cwd": "/fx/v/work", "steps": [], "predicate": {}}
    variant = {"id": "primary", "columns": 130, "rows": 31, "origin": [4, 9]}
    doubles = {"Adapter": lambda *a: adapters.append(a) or adapter, "Terminal": lambda *a: terminal, "build": build,
               "font_identity": lambda *a: {}, "private_theme": lambda *a: "theme", "namespace": lambda *a, **k: [],
               "settled": lambda *a, **k: snapshot, "separator": lambda *a: grid,
               "steps": lambda *a: None, "dynamic_masks": lambda *a: ([], None), "capabilities": lambda *a: {},
               "inspect_media": lambda *a, **k: ({"status": "PASS"}, None, []),
               "visual_review": lambda *a: {"status": "PASS"}, "compare_pixels": pixels,
               "file_assertions": lambda *a: {"status": "PASS"}}
    (root / MARKER).write_text("orchestration probe\n")
    with ExitStack() as stack:
        for name, double in doubles.items():
            stack.enter_context(patch.object(driver, name, double))
        result = driver.execute_pair(args, Owned(root), display, "foot", state, variant, {},
                                     {"elf_sha256": "elf"}, {}, {}, {}, seams or {})
    # A state blocked before its fixture was built has no candidate settings.
    settings = fx / "state/flea/ui.json"
    return result, json.loads(settings.read_text()) if settings.is_file() else None
