#!/usr/bin/env bash
# Runs every suite that needs nothing but a shell, and names the ones that do not.
#
# This repo had twelve suites and no runner: seven were invoked by no file at all, including
# js.sh, the largest. A suite nobody runs reads as coverage in a directory listing and provides
# none. It happened again in 0.1.4: picker.sh, capability-ownership.sh and network-live.sh were
# named by no file at all, so the release's largest new surface had no automated coverage. The
# audit at the bottom is what makes that a failure here rather than a review finding later.
#
# Each suite's OWN exit code is read, never a pipeline's. `./tests/js.sh | tail -1` hands you
# tail's status and reports success over a red suite, which is how a wrong green survived here
# for a whole session.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1

# Both profiles unconditionally, seven suites driving the debug binary and thumbs.sh the release one: an `[ ! -x <path> ]` guard is satisfied by a stale binary from an older commit, and measured on 2026-09-05 the debug and release hashes were unchanged across a whole run-all over edited source.
printf 'run-all: building target/debug/flea, seven suites need it\n'
cargo build -q || { printf 'run-all: cargo build failed, nothing else was run\n' >&2; exit 1; }
printf 'run-all: building target/release/flea, thumbs.sh needs it\n'
cargo build -q --release || { printf 'run-all: release build failed, nothing else was run\n' >&2; exit 1; }

headless="js keymap-gen charts budget empty-state sandbox capability-ownership gio-auth gvfs ops modes protocol portal archive thumbs network-open-share mount-listing uistate uiwriter media filemanager1 dragwire shellload acceptance-matrix"
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

# Named, not run: each needs something this script cannot assume it has. One list, read twice: it
# is printed here and it is what the audit below checks, so a suite cannot be quietly excluded.
not_run="
ui|needs the display, and refuses beside a Flea it did not start
drag|needs the display and a real pointer through uinput
cardsizes|needs the display, a real pointer through uinput, and Hyprland to resize the window
bench|is a separate headless benchmark-contract suite
package|needs a real makepkg archive in FLEA_PACKAGE_FILE
picker|needs the display, a session bus, and Flea activatable as the FileChooser backend
network-live|needs live share credentials and the approved runtime bundle, controller only
ui-tui|is a standalone native TUI proof that needs the display and owns the display lock
"

printf '\nNot run here, and why:\n'
named=""
while IFS='|' read -r name reason; do
    [ -n "$name" ] || continue
    named="$named $name"
    printf '  %-16s %s\n' "$name.sh" "$reason"
done <<EOF
$not_run
EOF

# Every suite in tests/ is in one of the two lists. A new one in neither is invoked by no file and
# mentioned by none, which is the state picker.sh shipped in, so it fails this runner rather than
# waiting for somebody to notice the directory listing is longer than the report.
orphans=""
for suite in tests/*.sh; do
    name=${suite#tests/}
    name=${name%.sh}
    [ "$name" = run-all ] && continue
    case " $headless $named " in
        *" $name "*) continue ;;
    esac
    # A ui-*.sh that tests/ui.sh sources is a case library, not a suite: it is run whenever ui.sh is,
    # and it has no entry point of its own. Read out of ui.sh rather than listed here, so a library
    # that stops being sourced becomes an orphan again instead of staying quietly excused.
    if grep -q "tests/$name\.sh" tests/ui.sh 2>/dev/null; then
        continue
    fi
    orphans="$orphans $name"
done
if [ -n "$orphans" ]; then
    printf '\nrun-all: FAIL suite(s) that no list runs and no line names:%s\n' "$orphans"
    failed=$((failed + 1))
fi

# Last, so it counts the audit above as well as the suites: a tally printed before the last check
# ran is the same wrong green this runner was written to stop.
printf '\nrun-all: %d suite(s) run, %d failed\n' "$ran" "$failed"

exit "$failed"
