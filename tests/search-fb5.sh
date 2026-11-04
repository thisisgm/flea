#!/usr/bin/env bash
# Runs fb5's Rust gates and timing probes without a terminal or an inherited application environment.
set -euo pipefail
cd "$(dirname "$0")/.."
repo=$PWD
# Removes the marked root after a passing run only; a failing run keeps its logs and names the root once.
cleanup() {
    local status=$?
    case "${root:-}" in /?*) ;; *) return 0 ;; esac
    if [ "$status" -ne 0 ]; then
        printf 'search-fb5: logs kept in %s\n' "$root" >&2
        return 0
    fi
    if [ -f "$root/.flea-test-sandbox" ]; then rm -rf -- "$root"; fi
}
if [ "${1:-}" != --isolated ]; then
    mkdir -p "$repo/.superpowers/fb5"
    root=$(mktemp -d "$repo/.superpowers/fb5/ci.XXXXXX")
    trap cleanup EXIT
    : > "$root/.flea-test-sandbox"
    mkdir -p "$root/home" "$root/tmp" "$root/config" "$root/data" "$root/state" "$root/cache" "$root/runtime"
    chmod 700 "$root/runtime"
    setsid -w env -i PATH="$PATH" HOME="$root/home" USER=gm LOGNAME=gm LANG=C.UTF-8 \
        RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}" CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}" \
        CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$repo/target}" TMPDIR="$root/tmp" \
        XDG_CONFIG_HOME="$root/config" XDG_DATA_HOME="$root/data" XDG_STATE_HOME="$root/state" \
        XDG_CACHE_HOME="$root/cache" XDG_RUNTIME_DIR="$root/runtime" \
        /bin/bash "$repo/tests/search-fb5.sh" --isolated "$root" < /dev/null
    exit 0
fi
root=${2:?isolated invocation needs its marked root}
[ -f "$root/.flea-test-sandbox" ]
python3 -B - <<'PY'
import os
try:
    fd = os.open('/dev/tty', os.O_RDONLY)
except OSError:
    pass
else:
    os.close(fd)
    raise AssertionError('a controlling terminal is present')
print('CI conditions: no controlling tty; PID namespace ' + os.readlink('/proc/self/ns/pid'))
print('CI explicit environment: ' + ','.join(sorted(os.environ)))
PY
[ "$(readlink /proc/self/fd/0)" = /dev/null ]
failed=0
gate() {
    local name=$1 rc=0
    shift
    printf 'COMMAND %s:' "$name"
    printf ' %q' "$@"
    printf '\n'
    "$@" > "$root/$name.log" 2>&1 || rc=$?
    cat "$root/$name.log"
    printf 'GATE %s exit=%s\n' "$name" "$rc"
    [ "$rc" -eq 0 ] || failed=$((failed + 1))
}
gate clippy tools/flea-clippy tools/clippy-baseline.txt "$root"
gate workspace cargo build --locked --workspace
if grep -E '^warning(\[|:)' "$root/workspace.log"; then
    printf 'FAIL workspace emitted a warning\n'
    failed=$((failed + 1))
fi
gate release-build cargo build --release --locked --bin flea
# cargo exits 0 when a name filter matches nothing, so each filtered gate states how many tests it runs.
must_run() {
    local name=$1 want=$2 exact=${3:-} got
    # Sample input: running 3 tests
    got=$(sed -n 's/^running \([0-9]*\) tests\{0,1\}$/\1/p' "$root/$name.log" | head -1)
    got=${got:-0}
    if [ "$got" -lt "$want" ] || { [ "$exact" = exact ] && [ "$got" -ne "$want" ]; }; then
        printf 'FAIL %s ran %s test(s), expected %s\n' "$name" "$got" "$want"
        failed=$((failed + 1))
    fi
}
gate regressions cargo test --release --locked --bin flea -- search_fix2 --nocapture
must_run regressions 3 exact
gate stream cargo test --release --locked --bin flea -- search_streams_completed_blocks_before_the_last_directory_lands
must_run stream 1 exact
gate search cargo test --release --locked --bin flea -- search
must_run search 1
gate protocol cargo test --release --locked --test search-fb5 -- --nocapture
for run in 1 2 3; do
    gate "probe-$run" cargo test --release --locked --bin flea -- --exact backend::searchjobs::tests::search_fb5_probe_worker_scale_pressure --ignored --nocapture --test-threads=1
    must_run "probe-$run" 1 exact
    gate "rows-$run" cargo test --release --locked --bin flea -- --exact backend::searchrowcache::tests::search_fb5_probe_rich_rows_latency --ignored --nocapture --test-threads=1
    must_run "rows-$run" 1 exact
done
[ "$failed" -eq 0 ] || { printf 'search-fb5: FAIL %s gate(s)\n' "$failed"; exit 1; }
printf 'search-fb5: PASS focused gates, CI conditions, and three probe repetitions\n'
