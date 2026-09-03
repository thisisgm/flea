#!/usr/bin/env bash
# Runs every suite that needs nothing but a shell, and names the ones that do not.
#
# This repo had twelve suites and no runner: seven were invoked by no file at all, including
# js.sh, the largest. A suite nobody runs reads as coverage in a directory listing and provides
# none.
#
# Each suite's OWN exit code is read, never a pipeline's. `./tests/js.sh | tail -1` hands you
# tail's status and reports success over a red suite, which is how a wrong green survived here
# for a whole session.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1

# protocol.sh and thumbs.sh drive the real binary, so it has to exist before they are asked
# anything. They refuse by name rather than reporting failures against a binary that is absent.
# protocol.sh drives the debug binary and thumbs.sh drives the release one. Building only debug
# passes in a tree that happens to carry both and fails on a fresh clone.
if [ ! -x target/debug/flea ]; then
    printf 'run-all: building target/debug/flea, protocol.sh needs it\n'
    cargo build -q || { printf 'run-all: cargo build failed, nothing else was run\n' >&2; exit 1; }
fi
if [ ! -x target/release/flea ]; then
    printf 'run-all: building target/release/flea, thumbs.sh needs it\n'
    cargo build -q --release || { printf 'run-all: release build failed, nothing else was run\n' >&2; exit 1; }
fi

headless="js keymap-gen charts budget sandbox ops modes protocol archive thumbs process-output"
failed=0
ran=0

for name in $headless; do
    suite="tests/$name.sh"
    [ -x "$suite" ] || { printf '  %-14s SKIP   no executable at %s\n' "$name" "$suite"; continue; }
    out=$("./$suite" 2>&1)
    rc=$?
    ran=$((ran + 1))
    # The suites do not share a summary format, so the last non-empty line is quoted as-is
    # rather than parsed into a number this script would then have to keep true.
    last=$(printf '%s\n' "$out" | grep -v '^[[:space:]]*$' | tail -1)
    if [ "$rc" -eq 0 ]; then
        printf '  %-14s ok     %s\n' "$name" "$last"
    else
        printf '  %-14s FAIL   rc=%s  %s\n' "$name" "$rc" "$last"
        failed=$((failed + 1))
    fi
done

printf '\nrun-all: %d suite(s) run, %d failed\n' "$ran" "$failed"

# Named, not run: each needs something this script cannot assume it has.
printf '\nNot run here, and why:\n'
printf '  ui.sh     needs the display, and refuses beside a Flea it did not start\n'
printf '  drag.sh   needs the display and a real pointer through uinput\n'
printf '  bench.sh  needs an idle box and the sudo password on stdin for drop_caches\n'

exit "$failed"
