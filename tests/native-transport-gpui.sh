#!/bin/bash
# Run the stage native window through the same transport and report missing capabilities.
set -euo pipefail
repo=$(cd "$(dirname "$0")/.." && pwd)
cd "$repo"
export FLEA_TEST_FRONTEND=gpui
. "$repo/tests/lib/flea-transport-ci.sh"
flea_transport_ci "$0" "$@"
mkdir -p "$repo/.superpowers/g010t"
if [[ -z ${DISPLAY:-}${WAYLAND_DISPLAY:-} && -z ${FLEA_TEST_PRIVATE_X11:-} ]]; then
    for tool in Xvfb bwrap; do
        command -v "$tool" >/dev/null || { printf 'TRANSPORT_NATIVE BLOCKED: %s is required without a graphical session\n' "$tool" >&2; exit 1; }
    done
    root=$(mktemp -d "$repo/.superpowers/g010t/x11.XXXXXX")
    : > "$root/.flea-test-sandbox"
    exec bwrap --bind / / --bind "$root" /tmp --setenv FLEA_TEST_PRIVATE_X11 1 -- bash "$0"
fi
cargo build --locked -p flea-gui --bin flea-gui
if [[ -z ${DISPLAY:-}${WAYLAND_DISPLAY:-} ]]; then
    command -v Xvfb >/dev/null || { echo 'TRANSPORT_NATIVE BLOCKED: Xvfb is required without a graphical session' >&2; exit 1; }
    root=$(mktemp -d "$repo/.superpowers/g010t/xvfb.XXXXXX")
    : > "$root/.flea-test-sandbox"
    exec python3 -B "$repo/tests/lib/flea_transport_xvfb.py" "$root" bash "$repo/tests/native-transport.sh"
fi
bash "$repo/tests/native-transport.sh"
