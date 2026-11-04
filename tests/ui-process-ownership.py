#!/usr/bin/env python3
"""Run the native harness's real teardown functions against a private fake /proc."""
import os
import json
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import tempfile
import time
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "tools"))
from flea_ipc import Client
Client("qs").export()


source = Path(__file__).with_name("ui.sh").read_text()
if os.geteuid() == Path("/").stat().st_uid:
    raise SystemExit("Run as the desktop user to exercise the foreign-uid boundary")


def function(name):
    start = source.index("\n" + name + "() {") + 1
    end = source.index("\n}", start) + 2
    return source[start:end]


scratch = Path(os.environ.get("TMPDIR", Path(__file__).resolve().parent.parent / ".superpowers/tmp"))
scratch.mkdir(parents=True, exist_ok=True)
root = Path(tempfile.mkdtemp(prefix="flea-process-ownership-", dir=scratch)).resolve()
(root / ".flea-test-sandbox").write_text("private process guard fixtures\n")


def guard(path):
    assert str(path) and path.is_absolute() and path.is_relative_to(root), path
    assert (root / ".flea-test-sandbox").is_file(), root


def process(pid, command, own=True, readable=True, state=None):
    path = root / "proc" / str(pid)
    guard(path)
    path.mkdir(parents=True)
    (path / "cmdline").write_bytes(command.encode() + b"\0")
    if state:
        # Sample /proc/PID/stat: "347 (gio) Z 1 347 ...", so the state is the field after the ")".
        (path / "stat").write_text(f"{pid} (gio) {state} 1 {pid} {pid} 0 -1 4194560\n")
    if readable:
        tag = str(root) if own else str(root / "foreign")
        (path / "environ").write_bytes(
            f"FLEA_TEST_RUN_ROOT={tag}\0FLEA_BIN=/bin/flea-check\0FLEA_PATH={root}/fixture/listing\0".encode()
        )


process(123, "qs -p /candidate/ui")
process(124, "qs -p /candidate/ui", own=False)
process(126, "qs -p /candidate/ui", readable=False)
process(127, f"qs -p {root}/fixture/xwsettings/ui-b/boot")
process(234, "/bin/flea-check --backend", own=False)
process(235, "/bin/flea-check --backend")
process(236, "/bin/flea-check --backend", readable=False)
process(345, "gio monitor trash:///", own=False)
process(346, "gio monitor trash:///")
process(347, "gio monitor trash:///", readable=False)
process(348, "gio monitor trash:///", state="Z")
process(349, "gio monitor trash:///", state="S")
process(350, "foot --config=fixture", state="S")
process(351, "gio monitor trash:///", state="Z")
process(352, "gio monitor trash:///", state="S")
process(353, "gio monitor trash:///", own=False, state="S")
(root / "proc/351/environ").write_bytes(b"")
(root / "proc/352/environ").write_bytes(b"")
helpers = "\n".join(function(name) for name in (
    "flea_pids", "flea_pid", "flea_process_owned", "backend_pids", "owned_trash_monitors",
    "kill_flea", "cleanup", "window_box", "click_row"
))
# Sample input: glasspick_stop() { through the next top-level } in the real capture library.
glass_source = Path(__file__).with_name("ui-captures-glass.sh").read_text()
helpers += "\n" + re.search(r"(?ms)^glasspick_stop\(\) \{\n.*?^\}", glass_source).group()
if "\nxwsettings_route_snapshot() {" in source:
    helpers += "\n" + function("xwsettings_route_snapshot")
prelude = f"""
set -u -o pipefail
. "$FLEA_IPC_HELPER"
run_root={shlex.quote(str(root))}
fixture_root="$run_root/fixture"
thumb_fixture="$run_root/thumb"
hash_fixture="$run_root/hash"
stale_fixture="$run_root/stale"
flea_ui=/candidate/ui
flea_bin=/bin/flea-check
flea_window_class=com.thisisgm.flea
foreign_pids=""
qs_pids=""
backend_ids=234
monitor_ids=345
drain_wait_s=1
stuck=false
client_payload='[]'
flea_process_dir() {{ printf '%s/proc/%s\\n' "$run_root" "$1"; }}
pgrep() {{
    case "$2" in qs) value="$qs_pids" ;; flea) value="$backend_ids" ;; gio) value="$monitor_ids" ;; *) return 2 ;; esac
    [[ -n "$value" ]] || return 1
    printf '%s\\n' "$value"
}}
kill() {{
    printf 'SIMULATED_SIGNAL %s\\n' "$1"
    "$stuck" || qs_pids=""
    if [[ "$1" == 127 ]]; then
        backend_ids=""
        monitor_ids=""
    fi
    return 0
}}
sleep() {{ :; }}
fail() {{ printf 'FAIL %s\\n' "$*" >&2; exit 1; }}
hyprctl() {{ printf '%s\\n' "$client_payload"; }}
ipc() {{ case "$1" in listAreaRect) printf '0 0 880 620\\n' ;; *) printf '10 20\\n' ;; esac; }}
omarchy-drive() {{ printf 'SIMULATED_CLICK %s\\n' "$*" >&2; }}
sandbox_remove() {{ printf 'SIMULATED_DELETE %s\\n' "$1"; }}
cache_restore() {{ :; }}
"""


def after_ownership(action):
    """Wrap flea_process_owned so a fixture changes right after the ownership read, the window a reaped or exiting monitor opens."""
    return f"""
real_owned=$(declare -f flea_process_owned)
eval "${{real_owned/flea_process_owned/inspect_owned}}"
flea_process_owned() {{
    inspect_owned "$1" || return "$?"
    {action}
}}
"""


unreadable_environ = 'chmod 000 "$(flea_process_dir "$1")/environ"'
cases = [
    ("owned window; foreign backend/monitor ignored", "qs_pids=123; kill_flea", 0, "SIMULATED_SIGNAL 123", "FAIL"),
    ("foreign window refused", "qs_pids=124; kill_flea", 1, "refusing to signal", "SIMULATED_SIGNAL"),
    ("vanished window harmless", "qs_pids=125; kill_flea", 0, "", "SIMULATED_SIGNAL"),
    ("vanished identity explicit", "flea_process_owned 125", 2, "", "SIMULATED_SIGNAL"),
    ("unreadable window refused", "qs_pids=126; kill_flea", 1, "refusing to signal", "SIMULATED_SIGNAL"),
    ("stuck window bounded; fixtures kept", "qs_pids=123; stuck=true; drain_wait_s=0; cleanup", 1, "active fixture roots kept", "SIMULATED_DELETE"),
    ("foreign refusal keeps fixtures", "qs_pids=124; cleanup", 1, "active fixture roots kept", "SIMULATED_DELETE"),
    ("owned backend must drain", "backend_ids=235; cleanup", 1, "backend or Trash monitor survived", "SIMULATED_DELETE"),
    ("unreadable backend keeps fixtures", "backend_ids=236; cleanup", 1, "cannot inspect backend ownership", "SIMULATED_DELETE"),
    ("owned monitor must drain", "monitor_ids=346; cleanup", 1, "backend or Trash monitor survived", "SIMULATED_DELETE"),
    ("unreadable monitor keeps fixtures", "monitor_ids=347; cleanup", 1, "cannot inspect Trash monitor ownership", "SIMULATED_DELETE"),
    ("enumeration failure keeps fixtures", "pgrep() { return 2; }; cleanup", 1, "cannot enumerate", "SIMULATED_DELETE"),
    ("successful drain permits fixture cleanup", "qs_pids=123; cleanup", 0, "SIMULATED_DELETE", "FAIL"),
    ("copied window is enumerated", "qs_pids=127\nflea_pids", 0, "127", "FAIL"),
    ("copied window and its children drain on success", "qs_pids=127\nbackend_ids=235\nmonitor_ids=346\nkill_flea", 0, "SIMULATED_SIGNAL 127", "FAIL"),
    ("copied window and its children drain on failure", "qs_pids=127\nbackend_ids=235\nmonitor_ids=346\ncleanup", 0, "SIMULATED_SIGNAL 127", "FAIL"),
    ("monitor vanishes after ownership read", after_ownership('rm -r "$(flea_process_dir "$1")"') + "monitor_ids=346\nowned_trash_monitors\n",
     0, "", "No such file or directory"),
    ("zombie monitor skipped after ownership read", after_ownership(unreadable_environ) + "monitor_ids=348\nowned_trash_monitors\n",
     0, "", "FAIL"),
    ("live monitor with an unreadable environ fails closed", after_ownership(unreadable_environ) + "monitor_ids=349\nowned_trash_monitors\n",
     3, "", "FAIL"),
    ("readable empty zombie is an exited identity", "flea_process_owned 351", 2, "", "SIMULATED_SIGNAL"),
    ("readable empty live process stays foreign", "flea_process_owned 352", 1, "", "SIMULATED_SIGNAL"),
    ("vanished after successful environ read is exited", 'tr() { command tr "$@"; mv "$(flea_process_dir 353)" "$run_root/vanished-after-read"; }; flea_process_owned 353',
     2, "", "SIMULATED_SIGNAL"),
    ("Glass stop accepts a readable zombie after TERM",
     'kill() { printf "350 (foot) Z 1 350 350 0 -1 4194560\\n" > "$run_root/proc/350/stat"; : > "$run_root/proc/350/environ"; }; glasspick_stop 350',
     0, "", "FAIL"),
]
# Sample source: the addressed j follows the active-address poll and precedes both cursor assertions.
key_start = source.index('    omarchy-drive key --window "$addrA" j')
poll_start = source.rfind("    # Focus once", 0, key_start)
routing_start = poll_start if poll_start >= 0 else key_start
routing_end = source.index('    omarchy-drive key --window "$addrA" k', key_start)
routing = source[routing_start:routing_end].replace('    local active=', '    active=')
route_driver = r'''
addrA=0xa
addrB=0xb
ipcA=(flea_ipc_drive -p /a flea)
ipcB=(flea_ipc_drive -p /b flea)
focus_required_reads=3
route_no_key=false
printf '0\n' > "$run_root/focus-reads"
printf '0\n' > "$run_root/a-cursor"
settle() { :; }
hyprctl() {
    local reads address
    reads=$(cat "$run_root/focus-reads")
    reads=$((reads + 1))
    printf '%s\n' "$reads" > "$run_root/focus-reads"
    address="$addrB"
    (( reads < focus_required_reads )) || address="$addrA"
    printf '{"class":"com.thisisgm.flea","address":"%s"}\n' "$address"
}
omarchy-drive() {
    case "$1" in
        focus) return 0 ;;
        key)
            if ! "$route_no_key" && (( $(cat "$run_root/focus-reads") >= focus_required_reads )); then
                printf '1\n' > "$run_root/a-cursor"
            fi
            ;;
        ipc)
            case "$5" in
                cursor)
                    if [[ "$3" == /a ]]; then
                        cat "$run_root/a-cursor"
                    else
                        printf '0\n'
                    fi
                    ;;
                path) printf '/fixture%s\n' "$3" ;;
                focusView) printf 'list\n' ;;
                keyDeliveryState) printf '{"activeFocusItem":"list-A"}\n' ;;
            esac
            ;;
    esac
}
'''
cases.append(("routing waits for A before typing", route_driver + routing, 0, "", "FAIL"))
cases.append(("stuck routing prints focus diagnostics", route_driver + "\nroute_no_key=true\n" + routing,
              1, 'activeFocusItem', "SIMULATED_SIGNAL"))
cases.append(("foreign uid refused", "flea_process_dir() { printf '/\\n'; }; flea_process_owned 123", 1, "", "SIMULATED_SIGNAL"))
owned_window = dict(pid=123, **{"class": "com.thisisgm.flea"}, at=[12, 42], size=[880, 620])
foreign_window = dict(owned_window, pid=124)
for name, windows, code in (
    ("owned pointer target", [owned_window], 0),
    ("ambiguous pointer targets refused", [owned_window, foreign_window], 1),
    ("foreign pointer target refused", [foreign_window], 1),
    ("vanished pointer target refused", [], 1),
    ("invalid pointer geometry refused", [dict(owned_window, size=[0, 620])], 1),
):
    body = "qs_pids=123; client_payload=" + shlex.quote(json.dumps(windows)) + "; click_row 0"
    cases.append((name, body, code, "SIMULATED_CLICK click 22 62" if code == 0 else "FAIL",
                  "FAIL" if code == 0 else "SIMULATED_CLICK"))
world_script = Path(__file__).with_name("ui-xwsettings-world.py").resolve()
# A hang guard for one whole run of any case, not an assertion: a case does little work, but a loaded box can stall it for seconds.
world_hang_guard_s = 120
# The gap between looks at a file another process writes, in a wait for a condition and never an assertion on time.
late_poll_s = 0.01
world_names = ("qs", "omarchy-drive", "hyprctl", "flea", "pgrep", "world")
# The shipped functions case_xwsettings reaches, read from ui.sh so a stub never stands in for one of them.
world_functions = "\n".join(function(name) for name in (
    "flea_pids", "flea_process_owned", "backend_pids", "owned_trash_monitors", "kill_flea", "assert_window", "ipc",
    "wait_listing", "settle", "xwsettings_route_snapshot", "case_xwsettings"
))
if "\nxwsettings_pid() {" in source:
    world_functions += "\n" + function("xwsettings_pid")
# The ipc bounds, the pid route and assert_window's title bound are plain assignments in ui.sh, so the route under test is the one the suite sets.
world_constants = "\n".join(re.findall(r"^(?:ipc_(?:call_\w+|pid)|async_wait_ms)=.*$", source, re.M))


def world_script_body(run):
    return f"""
set -u -o pipefail
. "$FLEA_IPC_HELPER"
run_root={shlex.quote(str(run))}
fixture_root="$run_root/fixture"
flea_ui="$run_root/candidate-ui"
flea_bin="$run_root/stub/flea"
flea_log="$run_root/flea.log"
run_log="$run_root/run.log"
flea_window_class=com.thisisgm.flea
test_frontend=qml
foreign_pids=""
drain_wait_s=1
settle_s=0
{world_constants}
flea_process_dir() {{ printf '%s/proc/%s\\n' "$run_root" "$1"; }}
kill() {{ "$run_root/stub/world" kill "$1"; }}
sleep() {{ :; }}
fail() {{ printf 'FAIL %s\\n' "$*" >&2; exit 1; }}
sandbox_scratch() {{ mkdir -p "$1"; }}
assert_theme() {{ :; }}
flea_test_exec() {{ setsid nohup "$@"; }}
{world_functions}
case_xwsettings
"""


def world_run(index, knobs):
    run = root / f"world-{index}"
    guard(run)
    stub = run / "stub"
    for directory in (stub, run / "fixture", run / "candidate-ui/boot", run / "state"):
        directory.mkdir(parents=True)
    for marker_dir in (run, run / "fixture"):
        (marker_dir / ".flea-test-sandbox").write_text("private xwsettings world\n")
    (run / "candidate-ui/boot/shell.qml").write_text("//@ pragma ShellId flea\n")
    for name in world_names:
        (stub / name).write_text(f'#!/bin/bash\nexec python3 {shlex.quote(str(world_script))} {name} "$@"\n')
        (stub / name).chmod(0o755)
    # Records the bounds the case hands timeout and runs the call without them: on a loaded box a world call can outlast the real 2 s bound and read as empty, and world_hang_guard_s bounds a stall. A call not shaped timeout --kill-after=X Y cmd fails loud.
    (stub / "timeout").write_text('#!/bin/bash\n[[ $1 == --kill-after=* && $# -ge 3 ]] || { echo "timeout stub: unexpected call timeout $*" >&2; exit 125; }\n'
                                  'WORLD_TIMEOUT="$1 $2" exec "${@:3}"\n')
    (stub / "timeout").chmod(0o755)
    env = {**os.environ, "PATH": f"{stub}:{os.environ['PATH']}", "FLEA_WORLD": str(run / "world"),
           "FLEA_TEST_RUN_ROOT": str(run), "XDG_STATE_HOME": str(run / "state"), **knobs}
    result = subprocess.run(["bash"], input=world_script_body(run), text=True, capture_output=True, timeout=world_hang_guard_s, env=env)
    # A world that failed before any window registered leaves no pids file; the caller reports the world's own output.
    calls_file, pids_file = run / "world/calls.jsonl", run / "world/pids"
    calls = [json.loads(line) for line in calls_file.read_text().splitlines()] if calls_file.exists() else []
    return result, calls, pids_file.read_text().split() if pids_file.exists() else []


def pid_routes(calls, window_pids):
    """Once B runs no call is path routed, and every qs call names A's or B's pid under the suite's ipc bounds."""
    launches = [i for i, call in enumerate(calls) if call["tool"] == "flea" and call["argv"][:1] == ["--gui"]]
    if len(launches) != 2 or len(window_pids) != 2:
        return f"expected two windows, saw {len(launches)} launches and pids {window_pids}"
    problems = []
    path_calls = [call for call in calls[launches[1]:] if call["tool"] == "omarchy-drive" and call["argv"][:1] == ["ipc"]]
    if path_calls:
        problems.append(f"path routed calls while two windows ran: {path_calls[:2]}")
    qs_calls = [call for call in calls if call["tool"] == "qs"]
    named = {call["argv"][2] for call in qs_calls if call["argv"][:2] == ["ipc", "--pid"]}
    if not qs_calls or any(call["argv"][:2] != ["ipc", "--pid"] for call in qs_calls) or named != set(window_pids):
        problems.append(f"qs calls named pids {named}, the windows are {window_pids}")
    bounds = {call["timeout"] for call in qs_calls}
    if bounds != {"--kill-after=1s 2s"}:
        problems.append(f"qs calls ran under {bounds}")
    return "; ".join(problems)


world_cases = (
    ("each window is addressed by its own pid", {}, 0, "XWSETTINGS pinned=ok", "FAIL", pid_routes),
    ("a route that ignores the pid is refused, naming the route", {"WORLD_ROUTE": "newest"}, 1, "pid routes", "XWSETTINGS route=ok", None),
    ("a missing first-rows stamp is refused", {"WORLD_STAMP": "unavailable"}, 1, "no first-rows stamp", "XWSETTINGS route=ok", None),
    ("a window reported under the other window's pid is refused", {"WORLD_SWAP_PIDS": "1"}, 1, "does not run", "XWSETTINGS route=ok", None),
    ("a window pid this run does not own is refused", {"WORLD_FOREIGN": "1"}, 1, "not owned", "XWSETTINGS route=ok", None),
    # No window means no pids file; the run must end in the world's own FAIL line, not a traceback of the harness.
    ("a launch whose window never appears is reported by the world's own output", {"WORLD_NO_WINDOW": "1"}, 1, "FAIL", "Traceback", None),
)


# The late-window case's name, shared by its verdict line and the pin that reads it.
LATE_WINDOW_CASE = "a wait begun before the window exists ends when it registers"


def late_window_wait(run_name, env_extra=None):
    """The stub's wait call must block until a window another call registers exists, as the real one does."""
    run = root / run_name
    guard(run)
    (run / "state").mkdir(parents=True)
    (run / ".flea-test-sandbox").write_text("private late window\n")
    env = {**os.environ, "FLEA_WORLD": str(run / "world"), "FLEA_TEST_RUN_ROOT": str(run),
           "XDG_STATE_HOME": str(run / "state"), "FLEA_UI": str(run / "ui"), **(env_extra or {})}
    calls = run / "world/calls.jsonl"
    waiter = subprocess.Popen(["python3", str(world_script), "omarchy-drive", "wait", "window", "flea", "--timeout", str(world_hang_guard_s)], env=env)
    late_window_wait.last_waiter = waiter
    try:
        # The waiter's own call line proves it began before the window exists; its next step is the one under test.
        deadline = time.monotonic() + world_hang_guard_s
        while time.monotonic() < deadline and not (calls.exists() and '"wait"' in calls.read_text()):
            time.sleep(late_poll_s)
        subprocess.run(["python3", str(world_script), "flea", "--gui", str(run)], env=env, timeout=world_hang_guard_s, check=True)
        return waiter.wait(timeout=world_hang_guard_s)
    finally:
        # A failed register or wait must not leave the waiter behind for the run-dir sweep.
        try:
            waiter.kill()
        except ProcessLookupError:
            pass
        try:
            waiter.wait(timeout=world_hang_guard_s)
        except subprocess.TimeoutExpired:
            pass


def late_window_case(run_name, env_extra=None):
    """The case's verdict line: PASS when the wait ends on the registered window, else a FAIL naming why, never a traceback."""
    try:
        waited = late_window_wait(run_name, env_extra)
    except Exception as exc:
        return f"FAIL {LATE_WINDOW_CASE}: {exc}"
    if waited != 0:
        return f"FAIL {LATE_WINDOW_CASE}: exit={waited}"
    return f"PASS {LATE_WINDOW_CASE}"


def late_window_register_fail_pin():
    """A register that exits non-zero, in a run dir of its own, gives the case's FAIL verdict and leaves no live waiter."""
    line = late_window_case("late-window-refused", {"WORLD_REGISTER_FAIL": "1"})
    if not (line.startswith(f"FAIL {LATE_WINDOW_CASE}: ") and "returned non-zero exit status 1" in line):
        return False, f"a failed register gave the verdict {line[-200:]!r}"
    waiter = late_window_wait.last_waiter
    if waiter.poll() is None:
        return False, f"waiter pid {waiter.pid} still live after a failed register"
    return True, ""


failures = 0
try:
    for name, body, code, present, absent in cases:
        result = subprocess.run(["bash"], input=prelude + helpers + "\n" + body + "\n", text=True, capture_output=True, timeout=world_hang_guard_s)
        output = result.stdout + result.stderr
        if result.returncode != code or present not in output or absent in output:
            failures += 1
            print(f"FAIL {name}: exit={result.returncode}, output={output!r}")
        else:
            print("PASS " + name)
    for index, (name, knobs, code, present, absent, check) in enumerate(world_cases):
        result, calls, window_pids = world_run(index, knobs)
        output = result.stdout + result.stderr
        problem = check(calls, window_pids) if check else ""
        if result.returncode != code or present not in output or absent in output or problem:
            failures += 1
            print(f"FAIL {name}: exit={result.returncode}, problem={problem!r}, output={output[-600:]!r}")
        else:
            print("PASS " + name)
    verdict = late_window_case("late-window")
    if verdict.startswith("FAIL"):
        failures += 1
    print(verdict)
    pin_ok, pin_detail = late_window_register_fail_pin()
    if pin_ok:
        print("PASS a register that exits non-zero gives the case its failing verdict with no live waiter")
    else:
        failures += 1
        print(f"FAIL a register that exits non-zero gives the case its failing verdict with no live waiter: {pin_detail}")
    print(f"{len(cases) + len(world_cases) + 2} process ownership checks, {failures} failed; no real signals")
finally:
    for child in root.iterdir():
        if child.name == ".flea-test-sandbox":
            continue
        guard(child)
        shutil.rmtree(child) if child.is_dir() else child.unlink()
    guard(root)
    (root / ".flea-test-sandbox").unlink()
    root.rmdir()

raise SystemExit(1 if failures else 0)
