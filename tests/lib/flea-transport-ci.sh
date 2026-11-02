#!/bin/bash
# Signal and timing probes run without a tty, in a private PID namespace and explicit environment.
flea_transport_ci() {
    local repo root parent_ns wayland
    repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
    if [[ ${FLEA_TRANSPORT_CI:-} == 1 ]]; then
        [[ $(readlink /proc/self/ns/pid) != "$FLEA_TEST_PARENT_PID_NS" ]] || return 1
        [[ ! -t 0 && ! -t 1 && ! -t 2 ]] || return 1
        python3 -B - <<'PY_CHECK'
import os
with open('/dev/null', 'r+b'):
    pass
try:
    fd = os.open('/dev/tty', os.O_RDONLY)
except OSError:
    pass
else:
    os.close(fd)
    raise SystemExit('transport CI requires no controlling tty')
print('TRANSPORT_CI tty=none pidns=private env=explicit')
PY_CHECK
        return 0
    fi
    mkdir -p "$repo/.superpowers/g010-transport"
    root=$(mktemp -d "$repo/.superpowers/g010-transport/ci.XXXXXX")
    : > "$root/.flea-test-sandbox"
    mkdir -p "$root"/{home,config,state,data,cache,tmp,runtime}
    chmod 700 "$root/runtime"
    : > "$root/runtime/.flea-test-sandbox"
    wayland=${WAYLAND_DISPLAY:-}
    if [[ -n $wayland && $wayland != /* && -n ${XDG_RUNTIME_DIR:-} ]]; then wayland="$XDG_RUNTIME_DIR/$wayland"; fi
    parent_ns=$(readlink /proc/self/ns/pid)
    exec setsid -w bwrap --bind / / --dev /dev --unshare-pid --proc /proc --die-with-parent -- \
        env -i PATH="$PATH" HOME="$root/home" XDG_CONFIG_HOME="$root/config" \
        XDG_STATE_HOME="$root/state" XDG_DATA_HOME="$root/data" XDG_CACHE_HOME="$root/cache" \
        TMPDIR="$root/tmp" XDG_RUNTIME_DIR="$root/runtime" LANG=C.UTF-8 LC_ALL=C.UTF-8 \
        DISPLAY="${DISPLAY:-}" WAYLAND_DISPLAY="$wayland" XAUTHORITY="${XAUTHORITY:-}" \
        FLEA_TRANSPORT_CI=1 FLEA_TEST_PARENT_PID_NS="$parent_ns" \
        FLEA_TEST_FRONTEND="${FLEA_TEST_FRONTEND:-gpui}" \
        CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}" \
        RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}" \
        bash "$@" </dev/null
}
