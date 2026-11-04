#!/usr/bin/env python3
"""Check runner completion, failure and timeout handling with a deterministic qs stand-in."""

from pathlib import Path
import os
import subprocess
import tempfile


def main():
    worktree = Path(__file__).resolve().parents[2]
    root = Path(tempfile.mkdtemp(prefix="runner-check.", dir=worktree / ".superpowers/g05"))
    (root / ".g05-test-root").write_text("g05 runner protocol checks\n")

    def write(name, text, executable=False):
        path = root / name
        assert path.is_absolute() and path.is_relative_to(root)
        assert (root / ".g05-test-root").is_file()
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
        if executable:
            path.chmod(0o700)

    write("commons/Commons/Color.qml", "// Runner path check only.\n")
    write("oracle/ThemeColors.qml", "// Runner path check only.\n")
    write("palettes/test/colors.toml", 'background = "#000000"\nforeground = "#ffffff"\n')
    write("palettes/test/shell.toml", "")
    write("bin/qs", '''#!/bin/bash
mode=$FLEA_QS_STUB_MODE
if [[ "$mode" = deadline ]]; then sleep 10; exit 0; fi
if [[ "$mode" = incomplete ]]; then exit 0; fi
printf 'THEME_GOLDEN\ttest\t12\tbaseSize\t12\nTHEME_GOLDEN\ttest\t14\tbaseSize\t14\n'
if [[ "$mode" = fail ]]; then printf 'THEME_GOLDEN_FAIL: injected\n'; fi
if [[ "$mode" = error ]]; then printf 'TypeError: injected engine warning\n'; fi
if [[ "$mode" = no-done ]]; then exit 143; fi
printf 'THEME_GOLDEN_DONE\n'
case "$mode" in done-*) exit "${mode#done-}" ;; esac
exit 0
''', True)
    write("bin/timeout", '''#!/bin/bash
if [[ "$FLEA_QS_STUB_MODE" = deadline ]]; then
    shift 2
    exec /usr/bin/timeout 0.1s "$@"
fi
exec /usr/bin/timeout "$@"
''', True)
    cases = [(f"done-{rc}", True) for rc in (0, 1, 42, 137, 143)]
    cases += [(name, False) for name in ("done-124", "deadline", "incomplete", "no-done", "fail", "error")]
    failures = []
    for mode, success in cases:
        env = dict(os.environ, PATH=str(root / "bin") + os.pathsep + os.environ["PATH"], FLEA_QS_STUB_MODE=mode)
        result = subprocess.run(["bash", str(worktree / "tools/theme-golden/run.sh"),
                                 str(root / "commons"), str(root / "oracle"), str(root / "palettes")],
                                env=env, capture_output=True, text=True, timeout=10)
        write(mode + ".stdout", result.stdout)
        write(mode + ".stderr", result.stderr)
        passed = (result.returncode == 0) == success
        if success:
            passed &= result.stdout.startswith("# QML golden")
            passed &= all(f"test\t{stop}\tbaseSize\t{stop}\n" in result.stdout for stop in (12, 14))
        else:
            passed &= not result.stdout
        if mode in ("deadline", "done-124"):
            passed &= result.returncode == 124 and "timed out" in result.stderr.lower()
        if not passed:
            failures.append((mode, result.returncode, result.stderr))
        print(mode, "PASS" if passed else "FAIL", "exit", result.returncode)
    print("evidence:", root)
    if failures:
        raise SystemExit(repr(failures))
    print("PASS:", len(cases), "runner completion/failure/timeout cases")


if __name__ == "__main__":
    main()
