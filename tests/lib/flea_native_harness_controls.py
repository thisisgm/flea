"""Execute the native window guards with passing controls and deliberate mismatches."""
import os
from pathlib import Path
import re
import subprocess


def check_harness(root):
    repo = Path(__file__).resolve().parents[2]
    source = (repo / "tests/ui.sh").read_text()
    functions = "\n".join(re.search(r"^" + name + r"\(\) \{\n.*?^\}", source, re.M | re.S)[0]
                          for name in ("assert_focus", "assert_window", "window_addr_of_pid"))
    setup = r'''
set -eu -o pipefail
fail() { printf '%s\n' "$*" >&2; exit 1; }
test_frontend=gpui
flea_window_class=com.thisisgm.flea
ipc_pid=123
async_wait_ms=10
focused_pid() { printf '%s\n' "${FOCUS_PID:-123}"; }
focused_class() { printf '%s\n' "${FOCUS_CLASS:-com.thisisgm.flea}"; }
hyprctl() { printf '%s\n' "${CLIENTS}"; }
assert_theme() { :; }
'''
    environment = dict(os.environ, CLIENTS='[{"class":"com.thisisgm.flea","pid":123,"address":"0xabc"}]')
    for name, updates, success in (
        ("owned", {}, True),
        ("wrong-pid", {"FOCUS_PID": "124"}, False),
        ("wrong-class", {"FOCUS_CLASS": "other"}, False),
        ("foreign-client", {"CLIENTS": '[{"class":"com.thisisgm.flea","pid":124}]'}, False),
        ("duplicate-client", {"CLIENTS": '[{"class":"com.thisisgm.flea","pid":123},{"class":"com.thisisgm.flea","pid":123}]'}, False),
        ("legacy-short-class", {"CLIENTS": '[{"class":"flea","pid":123}]'}, False),
    ):
        result = subprocess.run(["bash", "-c", setup + functions + '\nassert_window\nwindow_addr_of_pid "$ipc_pid"'],
                                env=dict(environment, **updates), capture_output=True, timeout=3)
        assert (result.returncode == 0) == success, (name, result)
        if success:
            assert result.stdout == b"0xabc\n", result
        (root / f"native-guard-{name}.log").write_bytes(result.stdout + result.stderr)
        print(f"TRANSPORT_HARNESS {name}=verified exit={result.returncode}")
