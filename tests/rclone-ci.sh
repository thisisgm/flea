#!/bin/bash
# Re-run rclone under CI process isolation, and judge Rust warnings and clippy against the unchanged baseline.
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p .superpowers/b-rclone
rclone_ci_root=$(mktemp -d "$PWD/.superpowers/b-rclone/ci.XXXXXX")
: > "$rclone_ci_root/.flea-test-sandbox"
export CARGO_TARGET_DIR="$rclone_ci_root/target"
rclone_ci_status=0
if cargo build --locked --workspace --message-format=json > "$rclone_ci_root/build.json" 2> "$rclone_ci_root/build.log"; then
    echo 'WORKSPACE_BUILD_EXIT=0'
    if python3 -B - "$rclone_ci_root/build.json" <<'PY'
import json
import sys
# Sample input: {"reason":"compiler-message","message":{"level":"warning","rendered":"warning: unused variable"}} or {"reason":"build-finished","success":true}
messages = [json.loads(line) for line in open(sys.argv[1]) if line.startswith('{')]
warnings = [m['message'] for m in messages if m.get('reason') == 'compiler-message'
            and m['message']['level'] in ('warning', 'error')]
for message in warnings:
    print(message['rendered'])
assert not warnings, 'workspace build emitted warnings or errors'
assert any(m.get('reason') == 'build-finished' and m.get('success') for m in messages)
print('PASS rclone workspace build has zero warnings')
PY
    then
        echo 'WORKSPACE_WARNING_CHECK_EXIT=0'
    else
        echo "WORKSPACE_WARNING_CHECK_EXIT=$?"
        rclone_ci_status=1
    fi
else
    cat "$rclone_ci_root/build.log"
    rclone_ci_status=1
fi
printf 'COMMAND tools/flea-clippy tools/clippy-baseline.txt %s\n' "$rclone_ci_root"
if tools/flea-clippy tools/clippy-baseline.txt "$rclone_ci_root"; then
    echo 'CLIPPY_EXIT=0'
    echo 'PASS rclone flea-clippy has no rise and no denied findings'
else
    echo "CLIPPY_EXIT=$?"
    rclone_ci_status=1
fi
rclone_ci_parent_ns=$(readlink /proc/self/ns/pid)
# The parity run uses the binary this script just built, never a FLEA_BIN or target/ left from another build.
rclone_ci_bin=$CARGO_TARGET_DIR/debug/flea
if [ ! -x "$rclone_ci_bin" ]; then
    echo "FAIL rclone CI parity: $rclone_ci_bin was not built"
    rclone_ci_status=1
elif unshare --user --map-root-user --pid --fork --mount-proc \
    setsid -w env -i PATH="$PATH" LC_ALL=C.UTF-8 FLEA_BIN="$rclone_ci_bin" \
    FLEA_RCLONE_PARENT_NS="$rclone_ci_parent_ns" /usr/bin/python3 -B tests/rclone.py "$rclone_ci_bin" < /dev/null; then
    echo 'PASS rclone CI parity: clean environment, no controlling tty, private PID namespace'
else
    rclone_ci_status=1
fi
exit "$rclone_ci_status"
