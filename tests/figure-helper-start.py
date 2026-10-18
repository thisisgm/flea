# A present but non-executable prlimit counts as missing, and an executable one that cannot start fails at exec.
import os
import pathlib
import subprocess
import sys
import tempfile

# A helper that never exits fails the check instead of hanging the suite.
HELPER_EXIT_BOUND_SECONDS = 5
# Print only the first bounded excerpt from each captured stream after failure.
HELPER_DIAGNOSTIC_LIMIT_CHARS = 4096
# An interpreter path that exists nowhere, so exec of a tool naming it fails with ENOENT and no shell fallback.
MISSING_INTERPRETER = "/nonexistent/flea-test-interpreter"


def diagnostics(result):
    for name, output in (("stdout", result.stdout), ("stderr", result.stderr)):
        if isinstance(output, bytes):
            output = output.decode(errors="replace")
        print(f"helper {name} (first {HELPER_DIAGNOSTIC_LIMIT_CHARS} characters):")
        print((output or "")[:HELPER_DIAGNOSTIC_LIMIT_CHARS])


def start(binary, executable):
    """One --figure-helper start against stand-in prlimit, bwrap and qjs files; None when it did not exit in time."""
    with tempfile.TemporaryDirectory(prefix="flea-figure-start-") as scratch:
        root = pathlib.Path(scratch)
        for name in ("prlimit", "bwrap", "qjs"):
            tool = root / name
            tool.write_text(f"#!{MISSING_INTERPRETER}\n" if executable else "present but not executable\n")
            tool.chmod(0o755 if executable else 0o644)
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
missing = start(binary, executable=False)
if missing is None:
    sys.exit(1)
checks += [
    (missing.returncode == 127, "a non-executable prlimit refuses with 127"),
    ("needs bwrap and prlimit, and one of them is missing" in missing.stderr, "the refusal names both sandbox tools"),
    ("could not start" not in missing.stderr, "the refusal comes before exec"),
]
failed_exec = start(binary, executable=True)
if failed_exec is None:
    sys.exit(1)
checks += [
    (failed_exec.returncode == 127, "exec failure refuses with 127"),
    ("could not start" in failed_exec.stderr and "prlimit" in failed_exec.stderr, "exec failure names argv[0] prlimit"),
    ("os error 2" in failed_exec.stderr, "exec failure names the OS error"),
]
for passed, label in checks:
    print(("PASS " if passed else "FAIL ") + label)
failures = sum(not passed for passed, _ in checks)
if failures:
    diagnostics(missing)
    diagnostics(failed_exec)
print(f"figure-helper-start: {len(checks)} check(s), {failures} failed")
sys.exit(bool(failures))
