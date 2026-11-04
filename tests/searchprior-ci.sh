#!/usr/bin/env bash
# Runs prior timing under the CI isolation contract and judges workspace warnings and clippy.
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p .superpowers/fb3
fb3_scratch=$(mktemp -d "$PWD/.superpowers/fb3/ci.XXXXXXXX")
: > "$fb3_scratch/.flea-test-sandbox"
trap 'case $fb3_scratch in /*/.superpowers/fb3/ci.?*) [ -n "$fb3_scratch" ] && [ -f "$fb3_scratch/.flea-test-sandbox" ] && rm -rf -- "$fb3_scratch" ;; esac' EXIT
if ! cargo build --workspace --locked > "$fb3_scratch/workspace.log" 2>&1; then
    cat "$fb3_scratch/workspace.log"
    exit 1
fi
cat "$fb3_scratch/workspace.log"
if grep -q '^warning:' "$fb3_scratch/workspace.log"; then
    echo 'FAIL searchprior-ci workspace warnings'
    exit 1
fi
echo 'PASS searchprior-ci workspace build: zero warnings'
fb3_clippy_rc=0
tools/flea-clippy tools/clippy-baseline.txt "$fb3_scratch" > "$fb3_scratch/clippy.log" 2>&1 || fb3_clippy_rc=$?
cat "$fb3_scratch/clippy.log"
fb3_outer_pid_ns=$(readlink /proc/self/ns/pid)
bwrap --dev-bind / / --unshare-pid --proc /proc --die-with-parent -- \
    setsid -w env -i PATH="$PATH" HOME="$HOME" USER=gm LOGNAME=gm SHELL=/bin/bash LANG=C.UTF-8 \
    TMPDIR=/tmp CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}" RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}" \
    CARGO_TARGET_DIR="$PWD/target" CARGO_NET_OFFLINE=true FB3_OUTER_PID_NS="$fb3_outer_pid_ns" \
    /bin/bash -euo pipefail -c '
        [ "$(readlink /proc/self/ns/pid)" != "$FB3_OUTER_PID_NS" ]
        [ "$(readlink /proc/self/fd/0)" = /dev/null ]
        [ "$(ps -o tty= -p $$ | tr -d "[:space:]")" = "?" ]
        echo "PASS searchprior-ci isolation: env -i, no controlling tty, null stdin, private PID namespace"
        cargo test --release --locked --bin flea -- searchprior --nocapture --test-threads=1
        cargo test --release --locked --test searchprior -- searchprior --ignored --nocapture --test-threads=1
    ' < /dev/null
echo 'PASS searchprior-ci regressions: prior unit/protocol tests under CI isolation'
if [ "$fb3_clippy_rc" -ne 0 ]; then
    echo "FAIL searchprior-ci clippy exit=$fb3_clippy_rc"
    exit "$fb3_clippy_rc"
fi
echo 'PASS searchprior-ci: prior regressions, isolated timing, workspace warnings and clippy'
