"""Produce identity-bound native control supplements from controller capture receipts.

Use --plan for the complete capture checklist. --observations is an explicit mapping
of terminal/control/check to retained native cases, expected capture configurations,
observations and artifact hashes. No observation or visual decision is inferred from
TERM, a fake adapter, a decoder replay or a passing board comparison.
"""
import argparse
import base64
import hashlib
import json
from itertools import product
import re
from pathlib import Path
import shutil
import stat
import sys

from . import png
from .compositor import TOOL, command_matches, keymaps
from .adapters import TERMINALS
from .case_identity import artifacts, verify
from .evidence import Blocked, absolute, digest, output_root, save
from .protocol import DA1, KEYBOARD, KITTY_QUERY, KITTY_REPLY, capabilities
from .receipts import REQUIRED
from .reconcile import shutdown_failure

HERE = Path(__file__).resolve().parent
PRESETS = ("default", "vim", "mac", "windows")
PAIRS = {"ctrl-enter/O": (("ctrl-enter", "CTRL", "Return"), ("O", "SHIFT", "O")),
         "ctrl-shift-z/ctrl-y": (("ctrl-shift-z", "CTRL SHIFT", "Z"), ("ctrl-y", "CTRL", "Y")),
         "ctrl-shift-n/N": (("ctrl-shift-n", "CTRL SHIFT", "N"), ("N", "SHIFT", "N")),
         "ctrl-tab-keeps-ctrl-pagedown": (("ctrl-tab", "CTRL", "Tab"), ("ctrl-pagedown", "CTRL", "PageDown"))}
IDENTITY = ("commit", "dirty_patch_sha256", "elf_sha256", "references", "untracked")
GATES = ("behavior", "cell_geometry", "color", "native_visual")
LIFECYCLE = ("launch-exit.json", "relay-exit.json", "launch-start.json", "relay-start.json")
# Each recipe remains a controller obligation, including controls whose product owner has not joined.
RECIPES = {
    "query-native": ("TUI-16", "Query DA1, Kitty graphics and keyboard flags; retain real positive/negative replies and native picture pixels."),
    "query-timeout": ("TUI-16", "Suppress positive replies, retain dropped-responses.json and bounded disabled-state observations; never invent a positive."),
    "query-fragmented": ("TUI-16", "Fragment real replies and interleave compositor keys during negotiation; retain both sides and forwarding deadlines."),
    "query-stale-id": ("TUI-16", "Replay stale/wrong IDs only as labelled decoder input; require a separately observed matching current native response."),
    "tmux-passthrough": ("TUI-16", "Private tmux under foot and kitty, passthrough off/on, RGB/256 and extended-keys off/on, all 16 combinations."),
    "keys-presets": ("TUI-15", "Exercise every pair in default/vim/mac/windows, legacy/extended modes; retain ctrl-enter / O, ctrl-shift-z / ctrl-y, ctrl-shift-n / N and Ctrl+PageDown."),
    "keys-emission": ("TUI-15", "Compositor-delivered CSI-u, legacy tmux, Cyrillic base-layout where supported, arrow/menu alternatives; decoder replay is separate evidence."),
    "cwd-editor": ("TUI-15", "Use private cwd-file shell wrapper; failed navigation and output refusal; E suspends and restores the same terminal and hands off cwd."),
    "detail-modes": ("TUI-12", "Cycle Size/Modified/Mode/Owner, retaining identity and proving metadata remains scoped to held rows."),
    "calendar-boundaries": ("TUI-02", "Retain all original calendar cases, local timezone/DST, same day/year boundary, directories and Trash."),
    "geometry-resize": ("TUI-16", "Live 130x31, 130x4, 11x6, back to 130x31; overlays open, edge menu, pointer hit-testing on the drawn row."),
    "geometry-corners": ("TUI-16", "Per-window rounding 0/8 with every default-button/focus combination; record readback without global changes."),
    "media-stale": ("TUI-13", "Rapid navigation, late decode, private missing/error/cancel tools; old file replies never replace current identity."),
    "media-cleanup": ("TUI-13", "Picture clears under overlays and returns after; quit reaps playback and decoder children, relay and private tmux."),
    "media-controls": ("TUI-04", "PDF controls, 3D plate/orbit/zoom bounds; no turntable at rest or redundant graphics sends."),
    "operations-files": ("TUI-16", "Private marked fixtures only: rename/copy and all four collisions, trash/undo; compare resulting bytes and source identity."),
    "operations-refusals": ("TUI-09", "Default/refusal/cancel with private CUPS sink/providers/clipboard; no printer, rejected CUPS, no remote send or physical print."),
    "operations-formats": ("TUI-10/TUI-11", "Convert/Combine formats, list order, output/collision validation on real private fixture bytes."),
    "color-modes": ("TUI-13", "Truecolor, tmux RGB/256, 16-color reverse cursor, every supplied palette contrast and live palette replacement."),
    "font-native": ("TUI-13", "Native Nerd marks, private fontconfig in foot/alacritty, kitty/ghostty builtins, labelled forced states and live font replacement."),
    "font-no-chafa": ("TUI-13", "Private tool resolver hides chafa; facts alone and no extra missing-tool diagnostics."),
    "language": ("TUI-12", "Japanese width, isolated Arabic runs, Russian decimals and saved-language fallback."),
    "shutdown-normal": ("TUI-16", "Normal key quit restores termios/cursor/alternate screen/keyboard/mouse/graphics; reap all children."),
    "shutdown-signal": ("TUI-16", "Signal quit restores all terminal state; retain signal dispatch and child census including relay/mpv/decoder/private tmux."),
    "shutdown-startup": ("TUI-16", "Failed startup restores all terminal state; retain deliberate failure and expected exit plus child census."),
    "shift-delete-legacy": ("TUI-15", "Compositor Shift+Delete must reach candidate as CSI 3;2~ in all five terminals; X remains Carry."),
    "convert-variants": ("TUI-10", "Every image/PDF/office format, page range, metadata, multiple-image order, collision and actual converted outputs."),
}
EXTRA = {"color-modes": ["live-palette"], "font-native": ["live-font"],
         "media-cleanup": ["relay-reaped", "private-tmux-reaped"]}
SOURCES = {"foot-private-fontconfig": "foot", "alacritty-private-fontconfig": "alacritty",
           "kitty-builtins": "kitty", "ghostty-builtins": "ghostty", "legacy-tmux": "tmux",
           "tmux-RGB": "tmux", "tmux-256": "tmux"}
ALL_PRESETS = (*PRESETS, "yazi")
LISTING = frozenset({"tui-list-text", "tui-list-folder", "tui-list-image", "tui-list-symlink", "tui-list-code",
                     "tui-list-archive", "tui-list-blob", "tui-list-plain", "tui-list-pdf", "tui-list-video",
                     "tui-list-audio", "tui-tabs", "tui-marked", "tui-long", "tui-sort-size", "tui-detail",
                     "tui-no-preview", "tui-no-icons", "tui-small"})
CONTEXT_STATES = {"listing": LISTING, "preview": frozenset({"tui-quicklook-text", "tui-quicklook-image"}),
                  "pdf": frozenset({"tui-list-pdf"}), "media": frozenset({"tui-list-video", "tui-list-audio"})}
LAUNCH_FLAGS = ("--cwd-file", "--chooser-file")
CSI_U = re.compile(rb"\x1b\[(\d+)(?::\d*)*;(\d+)(?::\d+)?u")
CSI_BASE = re.compile(rb"\x1b\[(\d+)(?::(\d*))?:(\d+)(?:;(\d+)(?::\d+)?)?u")
KEYBOARD_OP = re.compile(rb"\x1b\[([<>=])(\d*)(?:;(\d*))?u")
CAPABILITY = re.compile(b"|".join(r.pattern for r in (KITTY_REPLY, DA1, KEYBOARD)))
SIGNALS = ("TERM", "INT", "HUP")


def _path(control, context, text, keys, owner, contract=None, mode="legacy", scope="preset", flag=None, states=None):
    return dict(control=control, context=context, input=text, chords=tuple(k[:2] for k in keys), owner=owner,
                hex=tuple(k[2] for k in keys) if mode == "legacy" else None,
                csi=tuple(k[2] for k in keys) if mode == "extended" else None,
                contract=contract, mode=mode, scope=scope, flag=flag, states=states)


# Every TUI-03 required key path (FLEA-REQUIRED-KEY-PATHS.tsv) names the control check that must observe it natively.
KEY_PATH_AUDIT = {"source": "TUI-03 FLEA-REQUIRED-KEY-PATHS.tsv (id, preset, context, input)",
                  "sha256": "0d75ca4a72a8b40468a79c9f7bbe62a5b9d095904bbdff6cd38d43544f84091c",
                  "ids": ("open-tab-enhanced", "open-tab-plain", "redo-enhanced", "redo-plain", "mkdir-enhanced",
                          "mkdir-plain", "shift-delete-legacy", "E-editor", "I-detail", "print", "contents", "convert-F",
                          "X-reserved", "focus-plain-listing", "focus-plain-preview", "focus-plain-pdf",
                          "focus-plain-media", "focus-enhanced-listing", "focus-enhanced-preview", "focus-enhanced-pdf",
                          "focus-enhanced-media", "tab-next-legacy-listing", "tab-next-legacy-preview",
                          "tab-next-legacy-pdf", "tab-next-legacy-media", "tab-previous-legacy-listing",
                          "tab-previous-legacy-preview", "tab-previous-legacy-pdf", "tab-previous-legacy-media",
                          "tab-new", "tab-close", "tab-reorder-left", "tab-reorder-right", "tab-number",
                          "yazi-tab-new-sequence", "cwd-file", "chooser-file", "yazi-opt-in", "preview-3d",
                          "preview-office")}
KEYS = "keys-presets"
PAIR_TAB = "ctrl-tab-keeps-ctrl-pagedown"
KEY_PATHS = {
    "open-tab-enhanced": _path(KEYS, "listing", '"<C-Enter>"', [("CTRL", "Return", (13, 5))], "TUI-15", "ctrl-enter/O", "extended"),
    "open-tab-plain": _path(KEYS, "listing", '"O"', [("SHIFT", "O", "4f")], "TUI-15", "ctrl-enter/O"),
    "redo-enhanced": _path(KEYS, "listing", '"<C-S-z>"', [("CTRL SHIFT", "Z", (122, 6))], "TUI-15", "ctrl-shift-z/ctrl-y", "extended"),
    "redo-plain": _path(KEYS, "listing", '"<C-y>"', [("CTRL", "Y", "19")], "TUI-15", "ctrl-shift-z/ctrl-y"),
    "mkdir-enhanced": _path(KEYS, "listing", '"<C-S-n>"', [("CTRL SHIFT", "N", (110, 6))], "TUI-15", "ctrl-shift-n/N", "extended"),
    "mkdir-plain": _path(KEYS, "listing", '"N"', [("SHIFT", "N", "4e")], "TUI-15", "ctrl-shift-n/N"),
    "shift-delete-legacy": _path("shift-delete-legacy", "listing", '"<S-Delete>"', [("SHIFT", "Delete", "1b5b333b327e")],
                                 "TUI-15", "legacy-CSI-3;2~"),
    "E-editor": _path("cwd-editor", "listing", '"E"', [("SHIFT", "E", "45")], "TUI-15", "E-editor"),
    "I-detail": _path("detail-modes", "listing", '"I"', [("SHIFT", "I", "49")], "TUI-15;TUI-02"),
    "print": _path("operations-refusals", "listing", '"<C-p>"', [("CTRL", "P", "10")], "TUI-09;TUI-15"),
    "contents": _path(KEYS, "listing", '"<C-g>"', [("CTRL", "G", "07")], "TUI-08;TUI-15;I50/G1"),
    "convert-F": _path("convert-variants", "listing", '"F"', [("SHIFT", "F", "46")], "TUI-10;TUI-15;I50/G1"),
    "X-reserved": _path("shift-delete-legacy", "listing", '"X"', [("SHIFT", "X", "58")], "TUI-15", "no-X-delete"),
    **{f"focus-plain-{c}": _path(KEYS, c, '"<Tab>"', [("", "Tab", "09")], "TUI-15")
       for c in ("listing", "preview", "pdf", "media")},
    **{f"focus-enhanced-{c}": _path(KEYS, c, '"<C-Tab>"', [("CTRL", "Tab", (9, 5))], "TUI-15", PAIR_TAB, "extended")
       for c in ("listing", "preview", "pdf", "media")},
    **{f"tab-next-legacy-{c}": _path(KEYS, c, '"<C-PageDown>"', [("CTRL", "PageDown", "1b5b363b357e")], "TUI-15", PAIR_TAB)
       for c in ("listing", "preview", "pdf", "media")},
    **{f"tab-previous-legacy-{c}": _path(KEYS, c, '"<C-PageUp>"', [("CTRL", "PageUp", "1b5b353b357e")], "TUI-15")
       for c in ("listing", "preview", "pdf", "media")},
    "tab-new": _path(KEYS, "listing", '"t"', [("", "T", "74")], "TUI-15"),
    "tab-close": _path(KEYS, "listing", '"<C-w>"', [("CTRL", "W", "17")], "TUI-15"),
    "tab-reorder-left": _path(KEYS, "listing", '"{"', [("SHIFT", "bracketleft", "7b")], "TUI-15"),
    "tab-reorder-right": _path(KEYS, "listing", '"}"', [("SHIFT", "bracketright", "7d")], "TUI-15"),
    "tab-number": _path(KEYS, "listing", '"1"', [("", "1", "31")], "TUI-15"),
    "yazi-tab-new-sequence": _path(KEYS, "listing", '["t", "t"]', [("", "T", "74"), ("", "T", "74")], "TUI-15"),
    "cwd-file": _path("cwd-editor", "startup/exit", "--cwd-file", [], "TUI-15", "cwd-file-shell-wrapper", None, "all",
                      ("--cwd-file",)),
    "chooser-file": _path("cwd-editor", "startup/exit", "--chooser-file", [], "TUI-15", None, None, "all", ("--chooser-file",)),
    "yazi-opt-in": _path("cwd-editor", "startup/exit", "--tui-keymap yazi", [], "TUI-15", "yazi-opt-in", None, "all",
                         ("--tui-keymap", "yazi")),
    "preview-3d": _path("media-controls", "preview/Quick Look", "Tab focus; documented per-kind controls", [("", "Tab", "09")],
                        "TUI-04;TUI-15", scope="all", states=frozenset({"tui-3d-column", "tui-3d-chafa", "tui-3d-quicklook"})),
    "preview-office": _path("media-controls", "preview/Quick Look", "Tab focus; documented per-kind controls", [("", "Tab", "09")],
                            "TUI-07;TUI-15", scope="all", states=frozenset({"tui-office"})),
}


def key_path_records():
    return [(name, preset, e["context"], e["input"]) for name, e in KEY_PATHS.items()
            for preset in (("all five",) if e["scope"] == "all" else ALL_PRESETS)]


def records_sha256(records):
    return hashlib.sha256("\n".join(sorted("\t".join(r) for r in records)).encode()).hexdigest()


def key_path_coverage():
    for name in KEY_PATH_AUDIT["ids"]:
        if name not in KEY_PATHS or KEY_PATHS[name]["control"] not in RECIPES:
            raise Blocked("required key path has no control assertion: " + name)
    if set(KEY_PATHS) != set(KEY_PATH_AUDIT["ids"]) or records_sha256(key_path_records()) != KEY_PATH_AUDIT["sha256"]:
        raise Blocked("key path table differs from the TUI-03 audit")


def launch_flags(argv):
    for size in (0, 2):
        k = len(argv) - 3 - size
        if k >= 0 and argv[k:k+2] == ["/opt/flea", "--tui"]:
            tail = argv[k+2:-1]
            if (not tail or (tail[0] in LAUNCH_FLAGS and tail[1].startswith("/"))
                    or tail == ["--tui-keymap", "yazi"]):
                return tail
    return None


def keyboard_flags(case, ns=None):
    """Kitty keyboard flags in force at ns (the transcript end when None), following the push/pop/set stack."""
    raw, received = capabilities(case / "pty.log"), capabilities(case / "pty.log", "candidate-input")
    if not raw["keyboard_flags"] or received["keyboard_flags"] != raw["keyboard_flags"]:
        return None
    stack = [raw["keyboard_flags"][0]]
    data, marks = stream(case, "output")
    for match in KEYBOARD_OP.finditer(data):
        when = marks[match.end() - 1][0]  # an op split across relay reads takes the ns of its final byte
        if ns is not None and when > ns:
            break
        kind, count, mode = match[1], match[2], match[3] or b""
        value = int(count or 0)
        if kind == b">":
            stack.append(value)
        elif kind == b"<":
            for _ in range(max(int(count or 1), 1)):
                if len(stack) > 1:
                    stack.pop()
        elif mode in (b"", b"1"):
            stack[-1] = value
        elif mode == b"2":
            stack[-1] |= value
        elif mode == b"3":
            stack[-1] &= ~value
    return stack[-1]


def keyboard_mode(case, ns=None):
    return "extended" if keyboard_flags(case, ns) else "legacy"


def key_time(case, proof):
    rows, indexes = events(case), proof.get("events") or []
    if not indexes or type(indexes[0]) is not int or not 0 <= indexes[0] < len(rows):
        raise Blocked("key needs its first terminal-input event")
    return rows[indexes[0]]["ns"]


def stream(case, direction):
    """Bytes of one direction in arrival order, with the (ns, event index) of each byte."""
    data, marks = bytearray(), []
    for index, row in enumerate(events(case)):
        if row["direction"] == direction and row["bytes"]:
            data.extend(row["bytes"])
            marks.extend([(row["ns"], index)] * len(row["bytes"]))
    return bytes(data), marks


def tokens(case, direction):
    data, marks = stream(case, direction)
    found = []
    for match in CAPABILITY.finditer(data):
        first, last = match.start(), match.end() - 1
        found.append(dict(token=match[0], start=match.start(), end=match.end(),
                          ns=[marks[i][0] for i in range(match.start(), match.end())],
                          start_ns=marks[first][0], end_ns=marks[last][0],
                          first_event=marks[first][1], last_event=marks[last][1]))
    return found


def verbatim(case):
    raw, received = tokens(case, "terminal-input"), tokens(case, "candidate-input")
    if not raw or [t["token"] for t in raw] != [t["token"] for t in received]:
        raise Blocked("capability replies were not forwarded verbatim from the native terminal")
    for native_token, forwarded in zip(raw, received):
        # Every forwarded byte arrives no earlier than the native byte it repeats.
        if any(sent < arrived for arrived, sent in zip(native_token["ns"], forwarded["ns"])):
            raise Blocked("capability replies were not forwarded verbatim from the native terminal")
    return raw


def key_between_candidate_replies(case, proof):
    dispatch = proof["dispatch"]
    command = load(case / "commands.json")[dispatch["command"]]
    data, marks = stream(case, "candidate-input")
    replies = tokens(case, "candidate-input")
    wanted, inside = bytes.fromhex(proof["hex"]), [range(t["start"], t["end"]) for t in replies]
    for at in range(len(data)):
        if (not data.startswith(wanted, at) or any(at in span for span in inside)
                or not command["start_ns"] <= marks[at][0] <= dispatch["deadline_ns"]):
            continue
        if any(t["end"] <= at for t in replies) and any(t["start"] >= at + len(wanted) for t in replies):
            return True
    return False


def negotiation(check, proof, case):
    raw = verbatim(case)
    if check == "fragmented":
        if not any(t["first_event"] != t["last_event"] for t in raw):
            raise Blocked("fragmented control needs a native reply split across terminal reads")
        if not any(t["first_event"] != t["last_event"] for t in tokens(case, "candidate-input")):
            raise Blocked("fragmented control needs the candidate to receive the reply in fragments")
        return
    key_emission(case, proof)
    rows = events(case)
    queries = [r["ns"] for r in rows if r["direction"] == "output"
               and (b"\x1b[c" in r["bytes"] or b"\x1b[?u" in r["bytes"] or KITTY_QUERY.search(r["bytes"]))]
    keys = [rows[index]["ns"] for index in proof["events"]]
    if not queries or not all(min(queries) < ns < max(t["end_ns"] for t in raw) for ns in keys):
        raise Blocked("key was not delivered during capability negotiation: " + check)
    if check == "interleaved" and not (any(t["end_ns"] < min(keys) for t in raw) and any(t["start_ns"] > max(keys) for t in raw)):
        raise Blocked("key did not arrive between native replies during negotiation")
    if check == "interleaved" and not key_between_candidate_replies(case, proof):
        raise Blocked("interleaved control needs the candidate to receive the key between two replies")


def sealed(case, name):
    """Read a hook file only when the capture verdict seals its digest; the proof's own anchor is never enough."""
    hashes = load(case / "verdict.json").get("artifact_sha256", {})
    if name not in hashes or hashes[name] != digest(artifact(case, name)):
        raise Blocked("hook file is not sealed by the capture verdict: " + name)
    return load(artifact(case, name))


def bounded_deadline(case, anchors):
    if "deadline.json" not in anchors:
        raise Blocked("bounded-deadline needs the relay deadline.json receipt")
    deadline = sealed(case, "deadline.json")
    outputs = [r for r in events(case) if r["direction"] == "output"]
    query, disabled, limit = (deadline.get(k) for k in ("query_ns", "disabled_ns", "deadline_ns"))
    if (any(type(v) is not int for v in (query, disabled, limit))
            or not any(r["ns"] == query and (b"\x1b[c" in r["bytes"] or KITTY_QUERY.search(r["bytes"])) for r in outputs)
            or not any(r["ns"] == disabled for r in outputs)
            or not 0 < disabled - query <= limit <= 2_000_000_000):
        raise Blocked("bounded deadline was not observed in the native transcript")
    # Screen output proves nothing about the disabled feature; only the sealed TUI-16 hook observation counts.
    if "disabled-state.json" not in anchors:
        raise Blocked("bounded-deadline needs the TUI-16 disabled-state observation hook")
    hook = sealed(case, "disabled-state.json")
    if hook.get("hook") != "tui16-disabled-state" or hook.get("state") != "disabled" or hook.get("ns") != disabled:
        raise Blocked("disabled-state observation does not match the deadline disabled_ns")


def stale_reply(check, proof, case, anchors):
    verbatim(case)  # the current reply, like every other capability reply, must be the native bytes of this case
    name = proof.get("decoder_artifact")
    if not name or name not in anchors:
        raise Blocked(check + " needs labelled decoder replay evidence")
    replay = sealed(case, name)
    match = KITTY_REPLY.fullmatch(bytes.fromhex(replay.get("input_hex", "")))
    raw = capabilities(case / "pty.log")
    current = [r for r in capabilities(case / "pty.log", "candidate-input")["kitty_replies"] if r["queried"]]
    if (replay.get("synthetic") is not True or replay.get("status") != "PASS" or replay.get("scenario") != check
            or replay.get("accepted") is not False or not match or not current
            or not any(r["queried"] for r in raw["kitty_replies"])):
        raise Blocked(check + " lacks a rejected decoder replay or a matching current native reply")
    if check == "wrong-id" and int(match[1]) in raw["query_ids"]:
        raise Blocked("wrong-id replay reuses a current query id")
    if check == "stale-before-query" and (int(match[1]) not in raw["query_ids"] or replay.get("before_query") is not True):
        raise Blocked("stale-before-query replay does not precede a current query")


def cyrillic(case, proof):
    match = CSI_BASE.fullmatch(bytes.fromhex(proof.get("hex", "")))
    base = chr(int(match[3])) if match else ""
    key = key_time(case, proof)
    if not match or not 0x400 <= int(match[1]) <= 0x4ff or not (base.isascii() and base.isalpha()) or not (keyboard_flags(case, key) or 0) & 4:
        raise Blocked("Cyrillic control needs an extended Cyrillic key reported with its base-layout key")
    commands = load(case / "commands.json")  # sealed by native_case; stdout readbacks come from here
    key_start = commands[proof["dispatch"]["command"]]["start_ns"]
    switches = [c for c in commands if command_matches(c, "switchxkblayout") and c["exit_code"] == 0
                and c.get("stdout", "").strip() == "ok" and c["end_ns"] <= key_start]
    if not switches:
        raise Blocked(f"Cyrillic control needs a successful {TOOL} switchxkblayout with stdout ok before the key")
    switch = switches[-1]
    readbacks = [(c, keymaps(c)) for c in commands]
    before = [km for c, km in readbacks if km is not None and c["end_ns"] <= switch["start_ns"]]
    after = [km for c, km in readbacks if km is not None and switch["end_ns"] <= c["start_ns"] and c["end_ns"] <= key_start]
    if not before or not after or not any(name in after[0] and after[0][name] != keymap for name, keymap in before[-1].items()):
        raise Blocked(f"Cyrillic control needs {TOOL} -j devices readbacks before and after the switch with a changed keymap")


def signal_dispatch(case, anchors):
    if "signal.json" not in anchors:
        raise Blocked("signal shutdown needs a recorded signal dispatch")
    record, commands = sealed(case, "signal.json"), load(case / "commands.json")
    index, pid = record.get("command"), sealed(case, "relay-start.json").get("candidate_pid")
    command = commands[index] if type(index) is int and 0 <= index < len(commands) else None
    if record.get("signal") not in SIGNALS or type(pid) is not int or record.get("pid") != pid or not command:
        raise Blocked("signal shutdown dispatch does not target the candidate")
    signal = record["signal"]
    if (Path(command["argv"][0]).name != "kill" or command["exit_code"] != 0
            or command["argv"][1:] not in (["-" + signal, str(pid)], ["-s", signal, str(pid)])):
        raise Blocked("signal shutdown needs a kill(1) sender that exits 0 and targets the candidate")
    if "process-census.json" not in anchors:
        raise Blocked("signal shutdown needs a child census before and after the dispatch")
    census = sealed(case, "process-census.json")
    alive = any(p == {"pid": pid, "kind": "candidate"} for p in census.get("processes_before") or [])
    if (not alive or type(census.get("before_ns")) is not int or census["before_ns"] >= command["start_ns"]
            or type(census.get("after_close_ns")) is not int or census["after_close_ns"] < command["end_ns"]):
        raise Blocked("signal shutdown census does not show the candidate alive before the dispatch")
    # terminal.rs turns SIGTERM, SIGINT and SIGHUP into a clean STOP exit (status 0); a signal exit fails shutdown_failure.


def binding(identity, terminal, label, hashes):
    document = {"identity": identity, "terminal": terminal, "id": label, "hashes": hashes}
    return hashlib.sha256(json.dumps(document, sort_keys=True).encode()).hexdigest()


def load(path):
    return json.loads(Path(path).read_text())


def artifact(case, name):
    relative = Path(name)
    if relative.is_absolute() or ".." in relative.parts or str(relative) in ("", "."):
        raise Blocked("artifact escaped its native case")
    path = absolute(case / relative)
    if not stat.S_ISREG(path.stat().st_mode):
        raise Blocked("native artifact is not a regular file: " + name)
    return path


def controls():
    rows = load(HERE / "controls.json")["controls"]
    if {r["id"] for r in rows} != set(RECIPES) or len(rows) != len(RECIPES):
        raise Blocked("control contract changed; explicit recipes and owners must be reconciled")
    return rows


def plan(terminals=TERMINALS, include_yazi=False):
    key_path_coverage()
    rows = []
    for terminal in terminals:
        for control in controls():
            owner, recipe = RECIPES[control["id"]]
            checks = control["checks"] + EXTRA.get(control["id"], [])
            if include_yazi and control["id"] == "keys-presets":
                checks += ["yazi-opt-in"]
            checks += ["key-path:" + name for name, e in KEY_PATHS.items() if e["control"] == control["id"]]
            rows.append(dict(terminal=terminal, kind="control", id=control["id"], checks=checks,
                             owner=owner, recipe=recipe, status="BLOCKED"))
    return {"schema": 1, "mode": "native-controls", "records": rows, "presets": list(PRESETS),
            "yazi_opt_in": include_yazi, "tmux_matrix": {"outer": ["foot", "kitty"],
            "passthrough": [False, True], "color": ["RGB", "256"], "extended_keys": [False, True]},
            "review": {gate: "PENDING" for gate in GATES},
            "key_paths": {name: dict(control=e["control"], context=e["context"], input=e["input"], owner=e["owner"])
                          for name, e in KEY_PATHS.items()}, "key_path_audit_sha256": KEY_PATH_AUDIT["sha256"],
            "integration": "TUI-16 supplies negotiation transforms, signal/startup and child-census capture hooks; pending hooks block the checks."}


def same_identity(actual, expected):
    for key in IDENTITY:
        if key not in actual or key not in expected or actual[key] != expected[key]:
            raise Blocked("candidate identity mismatch: " + key)


def shutdown_error(terminal_exit, shutdown, control=None, configuration=None):
    if not isinstance(shutdown, dict):
        return "shutdown record is not an object"
    if control == "shutdown-startup":
        setup = (configuration or {}).get("state", {}).get("control", {})
        expected = setup.get("expected_candidate_exit")
        if (setup.get("id") != control or type(expected) is not int or not 1 <= expected <= 255
                or shutdown.get("candidate_exit") != expected):
            return "startup control lacks its exact deliberate failure exit"
        shutdown = dict(shutdown, candidate_exit=0)
    return shutdown_failure(terminal_exit, shutdown)


def native_case(proof, identity, terminal, control=None):
    root = absolute(proof["capture"])
    same_identity(load(root / "identity.json"), identity)
    manifest = load(root / "manifest.json")
    if manifest.get("mode") != "native":
        raise Blocked("synthetic capture cannot establish native controls")
    relative = Path(proof["case"])
    if relative.is_absolute() or ".." in relative.parts or len(relative.parts) != 3:
        raise Blocked("native case must identify terminal/state/variant")
    case = absolute(root / relative)
    requested = proof["configuration"]
    if requested.get("terminal") != terminal or relative.parts[0] != terminal:
        raise Blocked("native case identifies another terminal")
    verdict = load(case / "verdict.json")
    if any(verdict.get(k) != value for k, value in zip(("terminal", "state", "variant"), relative.parts)):
        raise Blocked("native verdict identifies another case")
    failure = shutdown_error(verdict.get("terminal_exit"), verdict.get("shutdown"), control, requested)
    if failure:
        raise Blocked(failure)
    shutdown = load(case / "relay-exit.json")
    if shutdown_error(load(case / "launch-exit.json"), shutdown, control, requested):
        raise Blocked("raw shutdown receipt disagrees with passing verdict")
    hashes = verdict.get("artifact_sha256", {})
    verify(case, hashes, requested)
    if set(proof.get("sha256", {})) != set(LIFECYCLE):
        raise Blocked("complete lifecycle artifact hashes are required")
    for key in LIFECYCLE:
        if proof["sha256"][key] != digest(artifact(case, key)):
            raise Blocked("missing or stale lifecycle artifact: " + key)
    records = [r for r in manifest.get("records", []) if isinstance(r, dict)
               and all(r.get(k) == v for k, v in zip(("terminal", "state", "variant"), relative.parts))]
    if len(records) != 1 or records[0] != verdict or manifest.get("preset") != requested.get("preset"):
        raise Blocked("native case disagrees with its source manifest record")
    if verdict.get("elf_sha256") != identity["elf_sha256"]:
        raise Blocked("native case was captured from another candidate ELF")
    if verdict.get("reason") not in (None, "controller native visual review required") or verdict.get("shutdown_reason"):
        raise Blocked("native case contains an unresolved failure")
    for check in (verdict.get("behavior"), (verdict.get("cells") or {}).get("geometry"),
                  (verdict.get("cells") or {}).get("color"), verdict.get("media"), verdict.get("edges")):
        if not check or check.get("status") != "PASS":
            raise Blocked("native case machine gates did not pass")
    cells = load(case / "cells.json")
    if not cells.get("grid") or cells.get("unsupported") or cells.get("pending"):
        raise Blocked("native cell grid is incomplete")
    for name in ("native.png", "grid-crop.png", "diff.png", "overlay.png"):
        png.read(artifact(case, name))
    commands = load(case / "commands.json")
    if not commands or any(type(c.get("exit_code")) is not int for c in commands):
        raise Blocked("real command exit codes are required")
    launch, relay_spec = load(case / "launch-spec.json"), load(case / "relay-spec.json")
    argv = relay_spec.get("argv", [])
    binding = ["--ro-bind", identity.get("binary"), "/opt/flea"]
    if (not launch.get("argv") or not argv or argv[0] != "bwrap" or "--unshare-all" not in argv
            or not any(argv[i:i+3] == binding for i in range(len(argv)-2))
            or len(argv) < 3 or launch_flags(argv) is None
            or digest(absolute(identity["binary"])) != identity["elf_sha256"]
            or load(case / "launch-start.json").get("argv") != launch["argv"]):
        raise Blocked("real candidate launch argv is missing")
    if any(row.get("fake") or row.get("synthetic") for row in (verdict, launch, relay_spec, manifest)):
        raise Blocked("synthetic-native substitution")
    caps = capabilities(case / "pty.log")
    if caps != {k: load(case / "capabilities.json").get(k) for k in caps}:
        raise Blocked("capability receipt disagrees with native transcript")
    if any(load(case / name).get(key) is not True for name, key in
           (("relay-start.json", "stdin_tty"), ("relay-start.json", "stdout_tty"))):
        raise Blocked("candidate was not launched on a real controlling TTY")
    return case, verdict


def events(case):
    rows = load_lines(case / "pty.log")
    if not rows:
        raise Blocked("empty native PTY transcript")
    for row in rows:
        if type(row.get("ns")) is not int:
            raise Blocked("PTY events require real timestamps")
        row["bytes"] = base64.b64decode(row["base64"], validate=True)
    return rows


def load_lines(path):
    return [json.loads(line) for line in path.read_text().splitlines()]


def key_emission(case, proof, expected=None):
    rows = events(case)
    dispatch = proof["dispatch"]
    if dispatch.get("source") != "compositor" or dispatch.get("synthetic"):
        raise Blocked("native key requires compositor delivery")
    commands = load(case / "commands.json")
    command = commands[dispatch["command"]]
    argv = command["argv"]
    address = load(case / "profile.json")["calibration"]["client"]["address"]
    if (command["exit_code"] != 0 or "hypr_window_send_shortcut" not in argv
            or argv[-3:] != [address, dispatch["mods"], dispatch["key"]]):
        raise Blocked("key dispatch does not target the captured native window")
    received = b""
    if (not proof["events"] or sorted(set(proof["events"])) != proof["events"]
            or any(type(index) is not int or index < 0 for index in proof["events"])
            or not command["start_ns"] < dispatch["deadline_ns"] <= command["start_ns"] + 2_000_000_000):
        raise Blocked("native key requires ordered events and a bounded deadline")
    for index in proof["events"]:
        row = rows[index]
        if row["direction"] != "terminal-input" or not command["start_ns"] <= row["ns"] <= dispatch["deadline_ns"]:
            raise Blocked("synthetic or unrelated input cannot prove native key emission")
        received += row["bytes"]
    wanted = bytes.fromhex(expected or proof["hex"])
    if not wanted or received != wanted:
        raise Blocked("native emitted bytes do not match the required key")
    forwarded = b"".join(row["bytes"] for row in rows if row["direction"] == "candidate-input"
                         and command["start_ns"] <= row["ns"] <= dispatch["deadline_ns"])
    if wanted not in forwarded:
        raise Blocked("native key was not delivered to the candidate")


def key_path(check, proof, case):
    entry = KEY_PATHS.get(check[len("key-path:"):])
    if not entry:
        raise Blocked("unknown required key path: " + check)
    if proof.get("context") != entry["context"]:
        raise Blocked("key path observed in another context: " + check)
    states = entry["states"] or CONTEXT_STATES.get(entry["context"])
    if states and proof["configuration"]["state"].get("id") not in states:
        raise Blocked("key path state is outside its context: " + check)
    if entry["flag"]:
        tail = launch_flags(load(case / "relay-spec.json")["argv"])
        if not tail or tuple(tail[:len(entry["flag"])]) != entry["flag"]:
            raise Blocked("key path launch flag missing from the real candidate argv: " + check)
    sequence = proof.get("sequence") or ([proof] if entry["chords"] else [])
    if len(sequence) != len(entry["chords"]):
        raise Blocked("key path needs its exact key sequence: " + check)
    previous = -1
    for index, (sub, (mods, key)) in enumerate(zip(sequence, entry["chords"])):
        dispatch = sub.get("dispatch", {})
        if dispatch.get("mods") != mods or dispatch.get("key", "").lower() != key.lower():
            raise Blocked("key path requires its literal compositor chord: " + check)
        if type(dispatch.get("command")) is not int or dispatch["command"] <= previous:
            raise Blocked("key path sequence is out of order: " + check)
        previous = dispatch["command"]
        if entry["mode"] and keyboard_mode(case, key_time(case, sub)) != entry["mode"]:
            raise Blocked(f"key path needs the {entry['mode']} keyboard mode at its key: " + check)
        if entry["hex"] is None:
            match = CSI_U.fullmatch(bytes.fromhex(sub.get("hex", "")))
            if not match or (int(match[1]), int(match[2])) != entry["csi"][index]:
                raise Blocked("key path extended bytes do not match: " + check)
        key_emission(case, sub, entry["hex"][index] if entry["hex"] else None)


def check_proof(check, control, proof, case):
    if proof.get("observed") is not True or not proof.get("observation", "").strip():
        raise Blocked("missing explicit control observation: " + check)
    anchors = proof.get("artifacts", {})
    if not anchors:
        raise Blocked("control observation lacks artifact anchors: " + check)
    for name, sha in anchors.items():
        if digest(artifact(case, name)) != sha:
            raise Blocked("missing or stale assertion artifact: " + name)
    if control == "keys-presets" and check in PAIRS:
        chord = next((chord for chord in PAIRS[check] if chord[0] == proof.get("chord")), None)
        dispatch = proof.get("dispatch", {})
        if not chord or dispatch.get("mods") != chord[1] or dispatch.get("key", "").lower() != chord[2].lower():
            raise Blocked("key pair requires its literal compositor chord: " + check)
        key_emission(case, proof)
    if check in ("compositor-shift-delete", "legacy-CSI-3;2~"):
        if proof.get("dispatch", {}).get("key") != "Delete" or proof["dispatch"].get("mods") != "SHIFT":
            raise Blocked("Shift+Delete requires the literal compositor chord")
        key_emission(case, proof, "1b5b333b327e")
    elif check in ("compositor-emission", "CSI-u", "Cyrillic-base-layout"):
        if check == "Cyrillic-base-layout":
            cyrillic(case, proof)
        key_emission(case, proof)
        if check == "CSI-u" and not bytes.fromhex(proof["hex"]).endswith(b"u"):
            raise Blocked("CSI-u control did not observe extended key emission")
    elif check == "no-X-delete":
        dispatch = proof.get("dispatch", {})
        if dispatch.get("mods") != "SHIFT" or dispatch.get("key", "").lower() != "x":
            raise Blocked("reserved X control requires compositor-delivered X")
        key_emission(case, proof, "58")
    elif check in ("synthetic-decoder-separate", "synthetic-separated"):
        if not proof.get("decoder_artifact") or proof["decoder_artifact"] not in anchors:
            raise Blocked("separate synthetic decoder evidence is required")
        decoder = sealed(case, proof["decoder_artifact"])
        if decoder.get("synthetic") is not True or decoder.get("status") != "PASS":
            raise Blocked("decoder evidence must be explicitly synthetic and independently passing")
    if check.startswith("key-path:"):
        key_path(check, proof, case)
    if check in ("fragmented", "interleaved", "ordinary-input-retained"):
        negotiation(check, proof, case)
    if check == "bounded-deadline":
        bounded_deadline(case, anchors)
    if check in ("stale-before-query", "wrong-id"):
        stale_reply(check, proof, case, anchors)
    if control == "shutdown-signal":
        signal_dispatch(case, anchors)
    if control.startswith("query-"):
        rows = events(case)
        output = b"".join(r["bytes"] for r in rows if r["direction"] == "output")
        if b"\x1b[c" not in output or b"\x1b[?u" not in output or not KITTY_QUERY.search(output):
            raise Blocked("actual DA1/Kitty graphics/keyboard queries are required")
        caps = capabilities(case / "pty.log", "candidate-input")
        raw = capabilities(case / "pty.log")
        if check == "native-positive" and not ((caps["kitty_positive"] and raw["kitty_positive"])
                or (caps["sixel_positive"] and raw["sixel_positive"]) or (caps["keyboard_flags"] and raw["keyboard_flags"])):
            raise Blocked("no matching native positive capability reply")
        if check == "native-negative":
            incoming = b"".join(r["bytes"] for r in rows if r["direction"] == "terminal-input")
            negative = any(r["queried"] and r["reply"] != "OK" for r in caps["kitty_replies"])
            if not negative and not any(b"4" not in m[1].split(b";") for m in DA1.finditer(incoming)):
                raise Blocked("no actual native negative capability reply")
            if not (any(r["queried"] and r["reply"] != "OK" for r in raw["kitty_replies"])
                    or any(r["queried"] and not r["supported"] for r in raw["sixel_replies"])):
                raise Blocked("negative capability reply was not forwarded verbatim from the native terminal")
        if check in ("native-positive", "native-negative", "matching-current-id"):
            verbatim(case)
        if check == "timeout-disables-feature" and (caps["kitty_positive"] or caps["sixel_positive"]):
            raise Blocked("timeout control received a positive capability")
        if check == "matching-current-id" and not any(r["queried"] for r in caps["kitty_replies"]):
            raise Blocked("no matching current native Kitty response")
    if check in PRESETS and proof["configuration"]["preset"] != check:
        raise Blocked("preset assertion uses another captured preset")
    if check == "yazi-opt-in" and proof["configuration"]["preset"] != "yazi":
        raise Blocked("yazi must be an explicitly selected preset")
    if check in ("children-reaped", "playback-reaped", "decoder-reaped", "relay-reaped", "private-tmux-reaped"):
        if "process-census.json" not in anchors:
            raise Blocked("native child census is required")
        census = sealed(case, "process-census.json")
        if (census.get("remaining") != [] or set(census.get("checked", [])) != {"candidate", "relay", "mpv", "decoder", "tmux"}
                or type(census.get("after_close_ns")) is not int or not census.get("processes_before")):
            raise Blocked("native process census is incomplete or reports leaked children")


def tmux_context(case):
    profile = load(case / "profile.json")
    if profile.get("terminal") != "tmux" or digest(artifact(case, "tmux.conf")) != profile.get("tmux_config_sha256"):
        raise Blocked("tmux control requires the private tmux profile")
    argv = load(case / "launch-spec.json")["argv"]
    outer = Path(argv[0]).name
    if outer not in ("foot", "kitty") or "tmux" not in argv:
        raise Blocked("tmux control requires an observed foot or kitty outer terminal")
    socket = argv[argv.index("-S") + 1] if "-S" in argv else None
    options = {}
    for command in load(case / "commands.json"):
        args = command["argv"]
        if (len(args) >= 6 and Path(args[0]).name == "tmux" and args[1:3] == ["-S", socket]
                and args[3:5] == ["show-options", "-gv"] and command["exit_code"] == 0):
            options[args[5]] = command["stdout"].strip()
    if any(k not in options for k in ("allow-passthrough", "extended-keys", "terminal-features")):
        raise Blocked("tmux control lacks actual server option readbacks")
    if options["allow-passthrough"] not in ("on", "off") or options["extended-keys"] not in ("on", "off"):
        raise Blocked("tmux option readback is neither on nor off")
    return (outer, options["allow-passthrough"] == "on", "RGB" if "RGB" in options["terminal-features"] else "256",
            options["extended-keys"] == "on")


def complete_variants(label, check, proofs, include_yazi=False):
    if check.startswith("key-path:"):
        entry = KEY_PATHS[check[len("key-path:"):]]
        observed = set()
        for proof in proofs:
            case = absolute(Path(proof["capture"]) / proof["case"])
            observed.add((proof["configuration"]["preset"], keyboard_mode(case, key_time(case, proof)) if entry["mode"] else None))
        if not {(preset, entry["mode"]) for preset in (ALL_PRESETS if include_yazi else PRESETS)} <= observed:
            raise Blocked("key path needs every preset in its keyboard mode: " + check)
        return
    if label == "keys-presets":
        observed = set()
        for proof in proofs:
            case = absolute(Path(proof["capture"]) / proof["case"])
            mode = keyboard_mode(case, key_time(case, proof))
            observed.add((proof["configuration"]["preset"], mode, proof.get("chord")))
        if check in PAIRS:
            first, second = PAIRS[check]
            required = {(preset, mode, chord) for preset in PRESETS for mode, chord in
                        (("extended", first[0]), ("extended", second[0]), ("legacy", second[0]))}
        elif check in PRESETS:
            required = {(check, mode, None) for mode in ("legacy", "extended")}
        elif check == "yazi-opt-in":
            required = {("yazi", mode, None) for mode in ("legacy", "extended")}
        else:
            required = {(preset, mode, None) for preset in PRESETS for mode in ("legacy", "extended")}
        if not required <= observed:
            raise Blocked("key control needs every shipped preset and legacy/extended variant: " + check)
    if label == "tmux-passthrough":
        contexts = {tmux_context(absolute(Path(p["capture"]) / p["case"])) for p in proofs}
        required = set(product(("foot", "kitty"), (False, True), ("RGB", "256"), (False, True)))
        if not required <= contexts:
            raise Blocked("tmux control needs all 16 observed outer/passthrough/color/keyboard variants")


def produce(document, identity, output, terminals=TERMINALS, reviews=None, include_yazi=False):
    same_identity(document["identity"], identity)
    root = output_root(output)
    save(root.child("identity.json"), identity)
    requested = plan(terminals, include_yazi)
    supplied = {}
    for row in document["records"]:
        key = (row["terminal"], row["id"])
        if key in supplied:
            raise Blocked("duplicate native control observation")
        supplied[key] = row
    if set(supplied) - {(r["terminal"], r["id"]) for r in requested["records"]}:
        raise Blocked("unknown control observation")
    manifest = dict(schema=1, mode="native-controls", identity=identity, records=[])
    for request in requested["records"]:
        terminal, label = request["terminal"], request["id"]
        directory = root.directory(f"{terminal}/{label}")
        result = dict(terminal=terminal, kind="control", id=label, directory=str(directory.relative_to(root.path)),
                      status="BLOCKED", owner=request["owner"])
        verdict = {gate: "PENDING" for gate in GATES}
        verdict.update(assertions={}, synthetic=False, yazi_opt_in=include_yazi, terminal=terminal, id=label)
        try:
            row = supplied.get((terminal, label), {})
            assertions = row.get("assertions", {})
            if set(assertions) != set(request["checks"]):
                raise Blocked("missing or unexpected control assertions: " + label)
            retained, bindings = {}, {}
            for check in request["checks"]:
                proofs = assertions[check]
                if not isinstance(proofs, list) or not proofs:
                    raise Blocked("control assertion lacks native evidence: " + check)
                for proof in proofs:
                    source_terminal = "tmux" if label == "tmux-passthrough" else SOURCES.get(check, terminal)
                    case, _ = native_case(proof, identity, source_terminal, label)
                    check_proof(check, label, proof, case)
                    if str(case) not in retained:
                        destination = directory / "captures" / str(len(retained))
                        destination.mkdir(parents=True)
                        retained[str(case)] = destination
                    destination = retained[str(case)]
                    names = set(artifacts(case)) | set(proof["sha256"]) | set(proof["artifacts"]) | {"verdict.json"}
                    for name in names:
                        source = artifact(case, name)
                        target = destination / name
                        target.parent.mkdir(parents=True, exist_ok=True)
                        if not target.exists():
                            shutil.copyfile(source, target)
                    bindings.setdefault(check, []).append({"capture": str(retained[str(case)].relative_to(directory)),
                        "configuration": proof["configuration"], "observation": proof["observation"],
                        "native_capture_sha256": digest(case / "native.png"), "proof": proof})
                complete_variants(label, check, proofs, include_yazi)
                verdict["assertions"][check] = True
            primary = next(iter(retained.values()))
            for name in REQUIRED:
                shutil.copyfile(primary / name, directory / name)
            save(directory / "checks.json", bindings)
            hashes = {str(p.relative_to(directory)): digest(p) for p in sorted(directory.rglob("*")) if p.is_file()}
            sealed = binding(identity, terminal, label, hashes)
            verdict.update(behavior="PASS", native_capture_sha256=hashes["native.png"], binding_sha256=sealed)
            decision = (reviews or {}).get(f"{terminal}/{label}", {})
            if decision:
                if decision.get("binding_sha256") != sealed or any(decision.get(gate) != "PASS" for gate in GATES):
                    raise Blocked("stale or incomplete controller review")
                verdict.update({gate: "PASS" for gate in GATES})
                result["status"] = "PASS"
            else:
                result["reason"] = "controller geometry/color/native review required"
            result["sha256"] = hashes
        except (Blocked, OSError, ValueError, KeyError, TypeError, IndexError, AttributeError) as error:
            result["reason"] = str(error)
        save(directory / "verdict.json", verdict)
        result["verdict_sha256"] = digest(directory / "verdict.json")
        manifest["records"].append(result)
    manifest["status"] = "PASS" if all(r["status"] == "PASS" for r in manifest["records"]) else "BLOCKED"
    save(root.child("manifest.json"), manifest)
    return manifest


def validate_record(bundle, row, identity):
    """TUI-16's receipt integration calls this before accepting a produced record."""
    try:
        return _validate_record(bundle, row, identity)
    except (OSError, ValueError, KeyError, TypeError, IndexError, AttributeError) as error:
        raise Blocked("invalid native control record: " + str(error)) from error


def _validate_record(bundle, row, identity):
    bundle = absolute(bundle)
    same_identity(load(bundle / "manifest.json")["identity"], identity)
    directory = bundle / row["directory"]
    relative = Path(row["directory"])
    if relative.is_absolute() or ".." in relative.parts or relative.parts != (row["terminal"], row["id"]):
        raise Blocked("control record directory escaped its bundle or names another terminal")
    directory = absolute(directory)
    verdict = load(artifact(directory, "verdict.json"))
    if digest(directory / "verdict.json") != row["verdict_sha256"]:
        raise Blocked("stale native control verdict")
    hashes = row["sha256"]
    files = {str(p.relative_to(directory)) for p in directory.rglob("*") if p.is_file()} - {"verdict.json"}
    if set(hashes) != files or not set((*REQUIRED, "checks.json")) <= files:
        raise Blocked("incomplete native control artifact inventory")
    for name, sha in hashes.items():
        if digest(artifact(directory, name)) != sha:
            raise Blocked("stale native control artifact: " + name)
    if (binding(identity, row["terminal"], row["id"], hashes) != verdict.get("binding_sha256")
            or (verdict.get("terminal"), verdict.get("id")) != (row["terminal"], row["id"])
            or any(verdict.get(g) != "PASS" for g in GATES)):
        raise Blocked("native control lacks current controller review")
    include_yazi = verdict.get("yazi_opt_in")
    if type(include_yazi) is not bool:
        raise Blocked("native control verdict lacks its yazi opt-in decision")
    request = next((r for r in plan([row["terminal"]], include_yazi)["records"]
                    if r["id"] == row["id"]), None)
    if (not request or set(verdict["assertions"]) != set(request["checks"])
            or any(v is not True for v in verdict["assertions"].values())):
        raise Blocked("native control omitted required assertions")
    bindings = load(directory / "checks.json")
    if set(bindings) != set(request["checks"]):
        raise Blocked("native control omitted assertion evidence")
    for check, records in bindings.items():
        if not records:
            raise Blocked("native control assertion has no captures")
        archived_proofs = []
        for record in records:
            captured = absolute(directory / record["capture"])
            if not captured.is_relative_to(directory / "captures"):
                raise Blocked("assertion capture escaped control record")
            proof = record["proof"]
            config = load(captured / "capture-config.json")
            source = "tmux" if row["id"] == "tmux-passthrough" else SOURCES.get(check, row["terminal"])
            if config.get("terminal") != source:
                raise Blocked("assertion capture comes from another terminal")
            if config != record["configuration"] or config != proof["configuration"]:
                raise Blocked("assertion capture configuration changed")
            if digest(captured / "native.png") != record["native_capture_sha256"]:
                raise Blocked("assertion native capture changed")
            if shutdown_error(load(captured / "launch-exit.json"), load(captured / "relay-exit.json"), row["id"], config):
                raise Blocked("assertion capture has bad shutdown")
            for name in LIFECYCLE:
                if digest(captured / name) != proof["sha256"][name]:
                    raise Blocked("assertion lifecycle artifact changed")
            check_proof(check, row["id"], proof, captured)
            archived_proofs.append(dict(proof, capture=str(directory), case=record["capture"]))
        complete_variants(row["id"], check, archived_proofs, include_yazi)
    return verdict


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plan", action="store_true")
    parser.add_argument("--observations", type=Path)
    parser.add_argument("--identity", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--reviews", type=Path)
    parser.add_argument("--terminals", default=",".join(TERMINALS))
    parser.add_argument("--include-yazi", action="store_true")
    args = parser.parse_args(argv)
    try:
        terminals = args.terminals.split(",")
        if not terminals or len(set(terminals)) != len(terminals) or set(terminals) - set(TERMINALS):
            raise Blocked("unknown or duplicate terminal")
        if args.plan:
            print(json.dumps(plan(terminals, args.include_yazi), indent=2))
            return 0
        if not all((args.observations, args.identity, args.output)):
            raise Blocked("--observations, --identity and fresh marked --output are required")
        manifest = produce(load(args.observations), load(args.identity), args.output, terminals,
                           load(args.reviews) if args.reviews else None, args.include_yazi)
        print(json.dumps({"status": manifest["status"], "output": str(args.output)}))
        return 0 if manifest["status"] == "PASS" else 2
    except (Blocked, OSError, ValueError, KeyError, TypeError, AttributeError) as error:
        print(json.dumps({"status": "BLOCKED", "reason": str(error)}), file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
