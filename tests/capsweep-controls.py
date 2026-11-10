"""Drive the real capture branches with bad and good state, without native effects."""
import ast
import contextlib
import ctypes
import io
import json
import os
from pathlib import Path
import re
import select
import signal
import subprocess
import sys
import tempfile
import time
import tomllib
from types import SimpleNamespace

CONTROL_TIMEOUT_SECONDS = 10
PROCESS_WAIT_SECONDS = 5
PROCESS_POLL_SECONDS = 0.1
# timeout(1) exits with this status when it kills the command.
PICKER_TIMEOUT_STATUS = 124
PR_SET_CHILD_SUBREAPER = 36
PR_GET_CHILD_SUBREAPER = 37
# tests/ui.sh makes the suite's run root here whatever TMPDIR says, so the runtime controls use the same short parent.
RUN_ROOT_PARENT = "/tmp"

temporary_scratch = None
if len(sys.argv) == 1:
    temporary_scratch = tempfile.TemporaryDirectory(prefix="capsweep-controls-")
    scratch = Path(temporary_scratch.name)
    repo = Path(__file__).resolve().parent.parent
else:
    scratch, repo = map(Path, sys.argv[1:3])
groups = sys.argv[3:] or ["G1", "G2", "G3", "G4", "G7", "F10", "F11", "F12", "F13", "F14", "F15", "F16"]
picker_file = repo / "tests/ui-captures-sweep-picker.py"
# Sample input: def wait(label, predicate): in the picker capture script.
picker_tree = ast.parse(picker_file.read_text())
picker_try = next(node for node in picker_tree.body if isinstance(node, ast.Try))


def compiled(nodes):
    return compile(ast.Module(body=nodes, type_ignores=[]), str(picker_file), "exec")


def picker_case(label, mode, patch):
    evidence = scratch / label
    evidence.mkdir()
    world = {"view": "list", "marks": ["alpha.txt"], "marksBusy": False, "body": 14,
             "themeLoaded": True, "themeForeground": palette["foreground"].lower()}
    clock = SimpleNamespace(now=0)

    def sleep(seconds):
        clock.now += seconds

    def press(*keys):
        world["view"] = "grid" if "3" in keys else "list"
        if world["view"] == mode:
            world.update(patch)

    def run(arguments):
        if arguments[0] == "omarchy-drive":
            Path(arguments[2]).write_bytes(b"synthetic PNG")
            return ""
        assert arguments[0] == "magick", arguments
        return "1040x760"

    namespace = {"state": lambda: dict(world), "press": press, "run": run,
                 "evidence": evidence, "title": "Owned test picker", "json": json, "re": re,
                 "Path": Path, "tomllib": tomllib, "theme_home": theme_home,
                 "expected_foreground": palette["foreground"].lower(),
                 "time": SimpleNamespace(monotonic=lambda: clock.now, sleep=sleep),
                 "GLib": SimpleNamespace(MainContext=SimpleNamespace(default=lambda: SimpleNamespace(pending=lambda: False)))}
    helpers = [node for node in picker_tree.body if isinstance(node, ast.FunctionDef)
               and node.name in ("wait", "capture_state", "fixture_foreground")]
    constants = [node for node in picker_tree.body if isinstance(node, ast.Assign)
                 and isinstance(node.value, ast.Constant)
                 and all(isinstance(target, ast.Name) and target.id.isupper() for target in node.targets)]
    exec(compiled(constants + helpers), namespace)
    if "fixture_foreground" in namespace:
        assert namespace["fixture_foreground"](theme_home) == palette["foreground"].lower()
    loop = next(node for node in picker_try.body if isinstance(node, ast.For))
    error = None
    try:
        with contextlib.redirect_stdout(io.StringIO()):
            exec(compiled([loop]), namespace)
    except AssertionError as refused:
        error = str(refused)
    manifest = evidence / "manifest.tsv"
    entries = manifest.read_text().splitlines() if manifest.exists() else []
    target = f"sweep-picker-{mode}-selected"
    if patch:
        assert error, f"{label}: bad state accepted into manifest: {entries}"
        assert not any(entry.startswith(target) for entry in entries), (label, entries)
        assert not (evidence / f"{target}.png").exists(), label
        if "marks" in patch or "marksBusy" in patch:
            assert "marksBusy" in error and "marks" in error, error
        return
    assert error is None, error
    assert len(entries) == 2, entries
    for view in ("list", "grid"):
        # Sample input: {"marksBusy":false,"marks":["alpha.txt"],"themeLoaded":true}.
        observed = json.loads((evidence / f"sweep-picker-{view}-selected.json").read_text())
        assert not observed["marksBusy"] and len(observed["marks"]) == 1, observed
        assert observed["themeLoaded"] and observed["themeForeground"] == palette["foreground"].lower(), observed


def picker_controls(group):
    patches = [{"marks": []}, {"marksBusy": True}, {"marks": ["alpha", "beta"]}] if group == "G1" else [
        {"themeLoaded": False}, {"themeForeground": "#ffffff"}]
    for mode in ("list", "grid"):
        for index, patch in enumerate(patches):
            picker_case(f"{group}-{mode}-bad-{index}", mode, patch)
    picker_case(f"{group}-good", "grid", {})
    print(f"CAPSWEEP_CONTROLS {group} refused={len(patches) * 2} accepted=1")


SHELL_PRELUDE = r'''set -euo pipefail
test_frontend=gpui
repo=$1
sweep_root=$2
good=$3
evidence_dir="$sweep_root/evidence"
run_root="$sweep_root/run"
run_log="$sweep_root/run.log"
real_foreground='#dfe8e0'
. "$repo/tests/ui-captures-sweep.sh"
fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }
sleep() { SECONDS=$((SECONDS + 1)); }
settle() { :; }
assert_theme() { :; }
token_of() { printf '14\n'; }
magick() { printf '1040x760'; }
shot() { printf 'synthetic PNG\n' > "$evidence_dir/$1.png"; }
omarchy-drive() { [[ "$1" == shot ]] || fail 'unexpected drive call'; printf 'synthetic PNG\n' > "$2"; }
'''


def shell_root(label):
    root = scratch / label
    for directory in ("views", "previews", "evidence", "run"):
        (root / directory).mkdir(parents=True, exist_ok=True)
    (root / "views/a.txt").write_text("source\n")
    (root / "previews/notes.md").write_text("# Release notes\n\nBody\n")
    (root / "run/flea-second.log").touch()
    return root


def shell_run(root, script, argument):
    script_file = root / "probe.sh"
    script_file.write_text(script)
    return subprocess.run(["bash", str(script_file), str(repo), str(root), str(argument).lower()],
                          capture_output=True, text=True, timeout=CONTROL_TIMEOUT_SECONDS)


WINDOWS_STUBS = r'''
sweep_launch() { :; }
flea_pid() { printf '101\n'; }
xwdrag_qsid() { printf '%s\n' "$1"; }
xwdrag_launch_second() { XW_SECOND_PID=202; XW_SECOND_ID=202; }
xwdrag_cleanup() { :; }
case_xwdrag_cleanup() { :; }
xwdrag_signal_cleanup() { :; }
xwdrag_kill_second() { :; }
kill_flea() { :; }
xwdrag_focus() { :; }
xwdrag_key() { :; }
xwdrag_addr() { printf '0x%s\n' "$1"; }
xwdrag_geometry() { if [[ "$1" == 101 ]]; then printf '0 0 400 600\n'; else printf '400 0 400 600\n'; fi; }
xwdrag_qs() {
    case "$2" in
        tokens) printf 'baseSize=14\nother=0\n' ;;
        bodyPx) printf '14\n' ;;
        themeLoaded) printf 'true\n' ;;
        themeForeground) printf '#dfe8e0\n' ;;
        selectionCount) printf '1\n' ;;
        listingDropActive) [[ "$1" == 202 ]] || fail 'drag read on A'; printf '%s\n' "$good" ;;
        *) fail "unexpected IPC: $2" ;;
    esac
}
xwdrag_row_point() { printf '100 100\n'; }
xwdrag_floor_point() { printf '500 500\n'; }
# The typed helpers are the only compositor writes; each stub records its arguments for the placement check.
hypr_window_resize() { printf 'resize %s\n' "$*" >> "$sweep_root/placement"; }
hypr_window_move() { printf 'move %s\n' "$*" >> "$sweep_root/placement"; }
hyprctl() {
    case "$1" in
        monitors) printf '[{"focused":true,"x":0,"y":0,"width":800,"height":600,"scale":1}]\n' ;;
        clients) printf '[{"pid":101,"address":"0x101","floating":true},{"pid":202,"address":"0x202","floating":true}]\n' ;;
        cursorpos) cat "$sweep_root/pointer" ;;
        *) fail "unexpected hyprctl: $1" ;;
    esac
}
ydotool() {
    if [[ "$1" == mousemove ]]; then
        # Sample input: 100 100, the simulated desktop pointer position.
        read -r cursor_x cursor_y < "$sweep_root/pointer"
        printf '%s %s\n' "$((cursor_x + $3))" "$((cursor_y + $5))" > "$sweep_root/pointer"
    elif [[ "$1" == click ]]; then
        printf '%s\n' "$2" >> "$sweep_root/buttons"
        [[ "$2" != 0x80 || "${release_fails:-false}" != true ]] || return 1
    else
        fail "unexpected ydotool: $1"
    fi
}
printf '0 0\n' > "$sweep_root/pointer"
'''


# ui.sh runs every case as an if condition, where errexit is ignored even after set -e.
RUN_AS_CASE = "if ( set -e; {name} ); then :; else exit 1; fi\n"


def windows_script(release_fails):
    driver = (repo / "tests/ui.sh").read_text()
    # Sample input: xwdrag_glide() { followed by xwdrag_drag() { in tests/ui.sh.
    glide = driver.split("xwdrag_glide() {", 1)[1].split("\nxwdrag_drag() {", 1)[0]
    return (SHELL_PRELUDE + WINDOWS_STUBS + f"release_fails={str(release_fails).lower()}\nxwdrag_glide() {{" + glide
            + "\n" + RUN_AS_CASE.format(name="sweep_windows"))


def shell_case(group, accepted):
    root = shell_root(f"{group}-{'good' if accepted else 'bad'}")
    if group == "G3":
        script = windows_script(False)
        name = "windows-drag-held"
    else:
        capture = (repo / "tests/ui-captures-sweep.sh").read_text()
        # Sample input: elif [[ "$tag" == markdown ]]; then, ending at its matching fi.
        branch = capture.split('elif [[ "$tag" == markdown ]]; then\n', 1)[1].split("\n        fi", 1)[0]
        stubs = r'''
overlay_mode=rendered
key() { [[ "$1" == r ]] || fail 'unexpected key'; if [[ "$good" == true ]]; then overlay_mode=source; fi; }
ipc() {
    case "$1" in
        bodyPx) printf '14\n' ;;
        previewState) printf 'ready\n' ;;
        previewMarkdownView) printf '%s\n' "$overlay_mode" ;;
        columnMarkdownText|previewText) cat "$sweep_root/previews/notes.md" ;;
        *) fail "unexpected reader: $1" ;;
    esac
}
'''
        script = SHELL_PRELUDE + stubs + branch + "\n"
        name = "quicklook-markdown-source"
    result = shell_run(root, script, accepted)
    manifest = root / "evidence/manifest.tsv"
    if accepted:
        assert result.returncode == 0, result.stdout + result.stderr
        assert manifest.read_text() == f"sweep-{name}\t1040x760\n"
    else:
        assert result.returncode != 0, f"{group}: bad state accepted: {result.stdout}"
        assert "capsweep:" in result.stderr, result.stderr
        assert not manifest.exists() and not (root / f"evidence/sweep-{name}.png").exists()
    if group == "G3":
        assert (root / "buttons").read_text().splitlines() == ["0x40", "0x80"], "held button not released"
        # Sample input: an 800x600 monitor at 0,0 gives each window 400x600, A at x 0 and B at x 400.
        assert (root / "placement").read_text().splitlines() == [
            "resize 0x101 400 600", "move 0x101 0 0", "resize 0x202 400 600", "move 0x202 400 0"], "half-screen placement drifted"


PICKER_STUBS = r"""
fixture_root="$sweep_root"
flea_bin=/nonexistent/flea
flea_ui=/nonexistent/ui
sandbox_make() { mkdir -p "$1"; }
fixture_home_make() { mkdir -p "$1"; }
sweep_views() { :; }
sweep_previews() { :; }
sweep_menus() { :; }
sweep_dialogs() { :; }
sweep_settings() { :; }
sweep_windows() { printf 'sweep-windows-drag-held\t1920x1080\n' >> "$evidence_dir/manifest.tsv"; }
timeout() {
    printf 'synthetic picker backend log\n' > "$evidence_dir/picker-backend.log"
    return "$picker_status"
}
"""


def picker_status_case():
    for status in (1, PICKER_TIMEOUT_STATUS, 0):
        root = shell_root(f"F10-{status}")
        script = SHELL_PRELUDE + PICKER_STUBS + f"picker_status={status}\n" + RUN_AS_CASE.format(name="case_capsweep")
        result = shell_run(root, script, True)
        if status == 0:
            assert result.returncode == 0, result.stdout + result.stderr
            assert "SWEEP_TOTAL current 1 shots" in result.stdout, result.stdout
        else:
            assert result.returncode != 0, f"picker status {status} accepted: {result.stdout}"
            assert f"capsweep: picker sweep failed, status {status}" in result.stderr, result.stderr
    print("CAPSWEEP_CONTROLS F10 refused=2 accepted=1")


def release_case():
    root = shell_root("F12-bad")
    result = shell_run(root, windows_script(True), True)
    assert result.returncode != 0, f"failed release accepted: {result.stdout}"
    assert "capsweep: pointer release failed" in result.stderr, result.stderr
    assert (root / "buttons").read_text().splitlines() == ["0x40", "0x80"], "the release was not attempted once"
    print("CAPSWEEP_CONTROLS F12 refused=1")


def second_toggle_case():
    capture = (repo / "tests/ui-captures-sweep.sh").read_text()
    # Sample input: sweep_wait previewOpen false, then the Markdown if that checks the column and reopens Quick Look.
    marker = '        sweep_wait previewOpen false\n        if [[ "$tag" == markdown ]]; then\n'
    assert capture.count(marker) == 1, "the second Markdown toggle moved"
    branch = capture.split(marker, 1)[1].split("\n        fi", 1)[0]
    stubs = r"""
overlay_open=false
overlay_mode=source
column_view=rendered
[[ "$fault" != column ]] || column_view=source
key() {
    case "$*" in
        '-k Space') overlay_open=true; [[ "$fault" == quicklook ]] || overlay_mode=rendered ;;
        '-k Escape')
            printf 'escape\n' >> "$sweep_root/escapes"
            overlay_open=false
            ;;
        *) fail "unexpected key: $*" ;;
    esac
}
ipc() {
    case "$1" in
        bodyPx) printf '14\n' ;;
        previewOpen) printf '%s\n' "$overlay_open" ;;
        previewMarkdownView) printf '%s\n' "$overlay_mode" ;;
        columnMarkdownView) printf '%s\n' "$column_view" ;;
        columnMarkdownText) printf 'Release notes\n' ;;
        *) fail "unexpected reader: $1" ;;
    esac
}
"""
    # The flip lived in the closed Quick Look: the column must render and the next Quick Look must open rendered.
    for fault, message in (("none", ""), ("quicklook", "capsweep: previewMarkdownView expected 'rendered', saw 'source'"),
                           ("column", "capsweep: columnMarkdownView expected 'rendered', saw 'source'")):
        root = shell_root(f"F13-{fault}")
        result = shell_run(root, SHELL_PRELUDE + f"fault={fault}\n" + stubs + branch + "\n", fault == "none")
        escapes = root / "escapes"
        if fault == "none":
            assert result.returncode == 0, result.stdout + result.stderr
            assert escapes.read_text() == "escape\n", "Escape did not close the Quick Look once"
            assert (root / "evidence/manifest.tsv").read_text() == "sweep-column-markdown-rendered-after-flip\t1040x760\n"
        else:
            assert result.returncode != 0, f"a {fault} that still shows source was accepted: {result.stdout}"
            assert message in result.stderr, result.stderr
            assert not escapes.exists(), "Escape was pressed before the view read rendered"
    print("CAPSWEEP_CONTROLS F13 refused=2 accepted=1")


def keymap_order_case():
    capture = (repo / "tests/ui-captures-sweep.sh").read_text()
    # Sample input: sweep_wait keymapQuery copy, then the shot and the keymapSheetRows assertion.
    marker = "    sweep_wait keymapQuery copy\n"
    assert capture.count(marker) == 1, "the keymap query wait moved"
    body = capture.split(marker, 1)[1].split("    # The first Escape", 1)[0]
    stubs = r"""
ipc() {
    case "$1" in
        bodyPx) printf '14\n' ;;
        keymapSheetRows) if [[ "$good" == true ]]; then printf 'copy as\nmove to\n'; else printf 'delete\nmove to\n'; fi ;;
        *) fail "unexpected reader: $1" ;;
    esac
}
"""
    for accepted in (False, True):
        root = shell_root(f"F14-{'good' if accepted else 'bad'}")
        result = shell_run(root, SHELL_PRELUDE + stubs + body + "\n", accepted)
        manifest = root / "evidence/manifest.tsv"
        if accepted:
            assert result.returncode == 0, result.stdout + result.stderr
            assert manifest.read_text() == "sweep-keymap-query-copy\t1040x760\n"
        else:
            assert result.returncode != 0, f"a sheet with no copy row was accepted: {result.stdout}"
            assert "capsweep: keymap query lists no copy action" in result.stderr, result.stderr
            assert not manifest.exists(), "a shot of the wrong sheet reached the manifest"
            assert not (root / "evidence/sweep-keymap-query-copy.png").exists(), "a shot of the wrong sheet was taken"
    print("CAPSWEEP_CONTROLS F14 refused=1 accepted=1")


def scan_check(label, sweep_text):
    check = (repo / "tests/capsweep-check.sh").read_text()
    # Sample input: python3 - <<'PY', the scan source, then a line holding only PY.
    scan = check.split("python3 - <<'PY'\n", 1)[1].split("\nPY\n", 1)[0]
    # Sample input: . "$repo/tests/ui-captures-sweep.sh"
    sources = re.findall(r'^\. "\$repo/(tests/ui-[^"]+\.sh)"$', (repo / "tests/ui.sh").read_text(), re.M)
    tree = scratch / label
    shared = {"tests/ui.sh", "ui/Ipc.qml", "tests/fixtures/cool-dawn/colors.toml",
              "tests/ui-captures-sweep-picker.py", *sources} - {"tests/ui-captures-sweep.sh"}
    for relative in shared:
        (tree / relative).parent.mkdir(parents=True, exist_ok=True)
        (tree / relative).symlink_to(repo / relative)
    (tree / "tests/ui-captures-sweep.sh").write_text(sweep_text)
    return subprocess.run([sys.executable, "-B", "-"], input=scan, cwd=tree, capture_output=True,
                          text=True, timeout=CONTROL_TIMEOUT_SECONDS)


def reader_scan_case():
    capture = (repo / "tests/ui-captures-sweep.sh").read_text()
    # Sample input: xwdrag_qs "$(xwdrag_qsid "$pid")" themeForeground
    reader_form = 'xwdrag_qs "$(xwdrag_qsid "$pid")" themeForeground'
    assert capture.count(reader_form) == 1, "the window reader form moved"
    result = scan_check("F11-good", capture)
    assert result.returncode == 0, result.stdout + result.stderr
    result = scan_check("F11-bad", capture.replace(reader_form, reader_form.replace("themeForeground", "themeForegrund")))
    assert result.returncode != 0, f"a misspelt reader in the window form was accepted: {result.stdout}"
    assert "themeForegrund" in result.stderr, result.stderr
    print("CAPSWEEP_CONTROLS F11 refused=1 accepted=1")


def bare_bound_case():
    capture = (repo / "tests/ui-captures-sweep.sh").read_text()
    result = scan_check("F15-good", capture)
    assert result.returncode == 0, result.stdout + result.stderr
    # Sample input: a bare button code, a bare wall-clock bound and a bare timeout, each added to the sweep file.
    bare = (("ydotool click 0x40", "ydotool click 0x40"), ("end=$((SECONDS + 7))", "SECONDS + 7"),
            ("timeout 9 true", "timeout 9"))
    for index, (line, matched) in enumerate(bare):
        result = scan_check(f"F15-bad-{index}", capture + f"\nsweep_bare_probe() {{ {line}; }}\n")
        assert result.returncode != 0, f"a bare literal was accepted: {line}"
        assert matched in result.stderr, result.stderr
    print(f"CAPSWEEP_CONTROLS F15 refused={len(bare)} accepted=1")


def cleanup_controls():
    cleanup = next(node for node in picker_try.finalbody if isinstance(node, ast.For))
    namespace = {"signal": signal, "subprocess": subprocess, "time": time,
                 "PROCESS_WAIT_SECONDS": PROCESS_WAIT_SECONDS, "POLL_SECONDS": PROCESS_POLL_SECONDS}
    leader_script = '''
import os
import signal
import sys

ready_read, ready_write = os.pipe()
child = os.fork()
if child == 0:
    os.close(ready_read)
    signal.signal(signal.SIGTERM, signal.SIG_IGN)
    os.write(ready_write, b"ready")
    os.close(ready_write)
    while True:
        signal.pause()
os.close(ready_write)
# Sample input: b"ready", sent after the child installs its SIGTERM handler.
os.read(ready_read, len(b"ready"))
os.close(ready_read)
if sys.argv[1] == "timeout":
    signal.signal(signal.SIGTERM, signal.SIG_IGN)
print(child, flush=True)
if sys.argv[1] == "exited":
    sys.exit(0)
while True:
    signal.pause()
'''
    libc = ctypes.CDLL(None, use_errno=True)
    previous = ctypes.c_int()
    assert libc.prctl(PR_GET_CHILD_SUBREAPER, ctypes.byref(previous), 0, 0, 0) == 0
    assert libc.prctl(PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0) == 0
    try:
        for mode in ("exits", "exited", "timeout"):
            process = subprocess.Popen([sys.executable, "-c", leader_script, mode],
                                       stdout=subprocess.PIPE, text=True, start_new_session=True)
            child_pid = None
            child_fd = None
            calls = []
            try:
                assert select.select([process.stdout], [], [], CONTROL_TIMEOUT_SECONDS)[0], "leader did not start"
                # Sample input: "12345\n", the real SIGTERM-resistant child's PID.
                child_pid = int(process.stdout.readline())
                child_fd = os.pidfd_open(child_pid)
                if mode == "exited":
                    os.waitid(os.P_PID, process.pid, os.WEXITED | os.WNOWAIT)

                def killpg(pid, sig):
                    assert pid == process.pid, "cleanup signalled a foreign group"
                    if sig == signal.SIGKILL:
                        assert process.returncode is None, "leader was reaped before SIGKILL; PID may be recycled"
                        os.waitid(os.P_PID, pid, os.WEXITED | os.WNOHANG | os.WNOWAIT)
                    calls.append(sig)
                    os.killpg(pid, sig)

                namespace["processes"] = [process]
                namespace["os"] = SimpleNamespace(**{**vars(os), "killpg": killpg})
                exec(compiled([cleanup]), namespace)
                assert select.select([child_fd], [], [], PROCESS_WAIT_SECONDS)[0], "SIGTERM-resistant child survived leader exit"
                assert calls == [signal.SIGTERM, signal.SIGKILL], calls
                reaped_pid, status = os.waitpid(child_pid, 0)
                assert reaped_pid == child_pid and os.WIFSIGNALED(status) and os.WTERMSIG(status) == signal.SIGKILL
            finally:
                if child_fd is not None:
                    try:
                        signal.pidfd_send_signal(child_fd, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                    os.close(child_fd)
                process.kill()
                process.wait(timeout=CONTROL_TIMEOUT_SECONDS)
                process.stdout.close()
                if child_pid is not None:
                    try:
                        os.waitpid(child_pid, 0)
                    except ChildProcessError:
                        pass
    finally:
        assert libc.prctl(PR_SET_CHILD_SUBREAPER, previous.value, 0, 0, 0) == 0
    calls = []

    def gone_group(pid, sig):
        calls.append(sig)
        raise ProcessLookupError()

    def reaped(**kwargs):
        calls.append("reaped")

    namespace["processes"] = [SimpleNamespace(pid=101, wait=reaped)]
    namespace["os"] = SimpleNamespace(**{**vars(os), "killpg": gone_group, "waitid": lambda *args: object()})
    exec(compiled([cleanup]), namespace)
    assert calls == [signal.SIGTERM, signal.SIGKILL, "reaped"], calls
    print("CAPSWEEP_CONTROLS G4 exiting-leader=1 exited-leader=1 timeout=1 gone-group=1 reserved-pid=3")


theme_home = scratch / "theme-home"
theme_path = theme_home / ".local/state/omarchy/current/theme/colors.toml"
theme_path.parent.mkdir(parents=True)
def picker_functions(names, namespace):
    nodes = [node for node in picker_tree.body
             if (isinstance(node, ast.FunctionDef) and node.name in names)
             or (isinstance(node, ast.Assign) and isinstance(node.value, ast.Constant)
                 and all(isinstance(target, ast.Name) and target.id.isupper() for target in node.targets))]
    exec(compiled(nodes), namespace)
    return namespace


def socket_depth_case():
    namespace = picker_functions(("socket_fits", "private_runtime"), {"os": os, "Path": Path, "tempfile": tempfile})
    fits = namespace["socket_fits"]
    # Sample input: the runtime dir of native-sweep038r4n, where Quickshell failed to start its IPC server.
    deep = "/home/flea-sandbox/sweep038r4n.p23fAc3R/fixture/flea-ui-fixtures-2459770/capsweep-current/picker/run"
    assert not fits(deep), "a runtime dir too deep for an IPC socket was accepted"
    assert fits("/tmp/flea-ui-run.myJunPPO/picker-run-abcd1234"), "the suite's run root was refused"
    tail = f"/quickshell/by-id/{'x' * namespace['QS_INSTANCE_ID_CHARS']}/ipc.sock"
    room = namespace["SOCKET_PATH_MAX_BYTES"] - len(tail)
    assert fits("/" + "r" * (room - 1)), "a socket path of exactly the limit was refused"
    assert not fits("/" + "r" * room), "a socket path one byte over the limit was accepted"
    with tempfile.TemporaryDirectory(prefix="capsweep-rt-", dir=RUN_ROOT_PARENT) as parent:
        first = namespace["private_runtime"](parent)
        second = namespace["private_runtime"](parent)
        assert first != second and first.is_dir() and second.is_dir(), "two picker runs shared one runtime dir"
        assert (first.stat().st_mode & 0o777) == 0o700, "the runtime dir is not private"
        deep = Path(parent) / ("d" * room)
        deep.mkdir()
        try:
            namespace["private_runtime"](deep)
        except AssertionError as refusal:
            assert "too deep for an IPC socket" in str(refusal), refusal
        else:
            raise AssertionError("a runtime dir was made where its socket cannot bind")
    reply_owner_case()
    print("CAPSWEEP_CONTROLS F16 refused=4 accepted=4")


def reply_owner_case():
    # A child with a crafted environment stands in for the picker: owned_window reads /proc/<pid>/environ.
    with tempfile.TemporaryDirectory(prefix="capsweep-reply-") as base:
        runtime, home, binary = Path(base) / "runtime", Path(base) / "home", Path(base) / "flea"
        for directory in (runtime, home, Path(base) / "elsewhere"):
            directory.mkdir()
        for label, reply, refused in (("inside", runtime / "reply", False), ("outside", Path(base) / "elsewhere/reply", True)):
            environment = {"HOME": str(home), "FLEA_PICKER_REPLY": str(reply), "FLEA_BIN": str(binary)}
            child = subprocess.Popen(["sleep", str(CONTROL_TIMEOUT_SECONDS)], env=environment)
            try:
                namespace = picker_functions(("owned_window",), {
                    "os": os, "Path": Path, "title": "Owned test picker", "picker_pid": child.pid,
                    "runtime": runtime, "theme_home": home, "picker_env": {"FLEA_BIN": str(binary)},
                    "windows": lambda: [{"title": "Owned test picker", "pid": child.pid, "address": "0xabc"}]})
                try:
                    namespace["owned_window"]()
                    assert not refused, "a picker replying outside its runtime dir was accepted"
                except AssertionError as refusal:
                    assert refused and "foreign reply" in str(refusal), f"{label}: {refusal}"
            finally:
                child.kill()
                child.wait(timeout=PROCESS_WAIT_SECONDS)


theme_path.write_bytes((repo / "tests/fixtures/cool-dawn/colors.toml").read_bytes())
# Sample input: foreground = "#DFE8E0" in the installed colors.toml.
palette = tomllib.loads(theme_path.read_text())
failures = []
for group in groups:
    try:
        if group in ("G1", "G2"):
            picker_controls(group)
        elif group == "G4":
            cleanup_controls()
        elif group == "F10":
            picker_status_case()
        elif group == "F11":
            reader_scan_case()
        elif group == "F12":
            release_case()
        elif group == "F13":
            second_toggle_case()
        elif group == "F14":
            keymap_order_case()
        elif group == "F15":
            bare_bound_case()
        elif group == "F16":
            socket_depth_case()
        else:
            shell_case(group, False)
            shell_case(group, True)
            print(f"CAPSWEEP_CONTROLS {group} refused=1 accepted=1")
    except AssertionError as error:
        failures.append(group)
        print(f"FAIL: {group}: {error}")
print(f"CAPSWEEP_CONTROLS groups={len(groups)} failed={len(failures)}")
if temporary_scratch is not None:
    temporary_scratch.cleanup()
sys.exit(bool(failures))
