# A present but non-executable prlimit or bwrap counts as missing, and a prlimit that cannot start fails at exec.
import os
import pathlib
import re
import subprocess
import sys
import tempfile

# A helper that never exits fails the check instead of hanging the suite.
HELPER_EXIT_BOUND_SECONDS = 5
# Print only the first bounded excerpt from each captured stream after failure.
HELPER_DIAGNOSTIC_LIMIT_CHARS = 4096
# An interpreter path that exists nowhere, so exec of a tool naming it fails with ENOENT and no shell fallback.
MISSING_INTERPRETER = "/nonexistent/flea-test-interpreter"
# Stand-in file bodies: a tool that starts and exits 0, one whose exec fails with ENOENT, and one that is not executable.
STARTS = ("#!/bin/sh\nexit 0\n", 0o755)
CANNOT_START = (f"#!{MISSING_INTERPRETER}\n", 0o755)
NOT_EXECUTABLE = ("present but not executable\n", 0o644)
REFUSAL = "flea: the figure helper needs bwrap and prlimit, and one of them is missing"
# Sample line: "flea: the figure helper could not start prlimit: No such file or directory (os error 2)"; the text between is the locale's.
EXEC_FAILURE = re.compile(r"^flea: the figure helper could not start prlimit: .*\(os error 2\)$", re.M)


def diagnostics(result):
    for name, output in (("stdout", result.stdout), ("stderr", result.stderr)):
        if isinstance(output, bytes):
            output = output.decode(errors="replace")
        print(f"helper {name} (first {HELPER_DIAGNOSTIC_LIMIT_CHARS} characters):")
        print((output or "")[:HELPER_DIAGNOSTIC_LIMIT_CHARS])


def start(binary, tools):
    """One --figure-helper start against stand-in tool files named in tools; None when it did not exit in time."""
    with tempfile.TemporaryDirectory(prefix="flea-figure-start-") as scratch:
        root = pathlib.Path(scratch)
        for name, (body, mode) in tools.items():
            tool = root / name
            tool.write_text(body)
            tool.chmod(mode)
        env = dict(os.environ, PATH=str(root), FLEA_QJS=str(root / "qjs"))
        # No home and no cache dir, so the launcher starts no background build and writes nothing outside this scratch.
        env.pop("HOME", None)
        env.pop("XDG_CACHE_HOME", None)
        env["FLEA_UI"] = env.get("FLEA_UI") or str(pathlib.Path(__file__).resolve().parents[1] / "ui")
        try:
            return subprocess.run([binary, "--figure-helper"], env=env, capture_output=True, text=True, timeout=HELPER_EXIT_BOUND_SECONDS)
        except subprocess.TimeoutExpired as error:
            print(f"FAIL helper did not exit within {HELPER_EXIT_BOUND_SECONDS} seconds")
            diagnostics(error)
            return None


binary = pathlib.Path(sys.argv[1]).resolve()
checks = []
runs = []
# Each refusal case leaves only one tool unexecutable, so the refusal can come from that tool's own check alone.
for missing_tool in ("prlimit", "bwrap"):
    tools = {name: STARTS for name in ("prlimit", "bwrap", "qjs")}
    tools[missing_tool] = NOT_EXECUTABLE
    refused = start(binary, tools)
    if refused is None:
        sys.exit(1)
    runs.append(refused)
    checks += [
        (refused.returncode == 127, f"a non-executable {missing_tool} refuses with 127"),
        (REFUSAL in refused.stderr.splitlines(), f"a non-executable {missing_tool} gets the refusal naming both sandbox tools"),
        ("could not start" not in refused.stderr, f"a non-executable {missing_tool} is refused before exec"),
    ]
# Only prlimit cannot start, so an exec of any other tool would run its stand-in and exit 0.
failed_exec = start(binary, {"prlimit": CANNOT_START, "bwrap": STARTS, "qjs": STARTS})
if failed_exec is None:
    sys.exit(1)
runs.append(failed_exec)
checks += [
    (failed_exec.returncode == 127, "exec failure refuses with 127"),
    (EXEC_FAILURE.search(failed_exec.stderr) is not None, "exec failure names argv[0] prlimit and os error 2"),
]
for passed, label in checks:
    print(("PASS " if passed else "FAIL ") + label)
failures = sum(not passed for passed, _ in checks)
if failures:
    for run in runs:
        diagnostics(run)
print(f"figure-helper-start: {len(checks)} check(s), {failures} failed")
sys.exit(bool(failures))
