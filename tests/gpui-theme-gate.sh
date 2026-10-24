#!/usr/bin/env bash
# Stock contrast and real GPUI startup/theme subscription, always in a marked child HOME.
set -euo pipefail
repo=$(cd "$(dirname "$0")/.." && pwd)
cd "$repo"
# This suite owns only /tmp fixtures; build products stay in the caller's target directory.
fixture_parent=$(mktemp -d /tmp/flea-gpui-theme-gate.XXXXXXXX)
export FLEA_FIXTURE_ROOT="$fixture_parent"
. "$repo/tools/flea-sandbox-guard"
test_root=$(mktemp -d "$fixture_parent/run.XXXXXXXX")
sandbox_require "$test_root"
: > "$test_root/$SANDBOX_MARKER"
trap 'sandbox_remove "$test_root"; rmdir "$fixture_parent"' EXIT
for dir in home config cache state runtime; do mkdir -p "$test_root/$dir"; done
chmod 700 "$test_root/runtime"
: > "$test_root/home/$SANDBOX_MARKER"
: > "$test_root/runtime/$SANDBOX_MARKER"
stock=${FLEA_THEME_STOCK_DIR:-/usr/share/omarchy/themes}
[[ -d $stock ]] || { echo "FAIL gpui-theme-gate: missing stock inventory $stock" >&2; exit 1; }
# Same inventory as theme-contrast.sh: 22 stable or 23 dev themes, plus Cool Dawn and Sunset.
expected=24
[[ ! -f $stock/starship/colors.toml ]] || expected=25
count=0
for palette in "$stock"/*/; do
    if [[ -f ${palette}colors.toml ]]; then count=$((count + 1)); fi
done
[[ $((count + 2)) == "$expected" ]] || { echo "FAIL gpui-theme-gate: stock inventory changed ($count plus 2 fixtures, expected $expected)" >&2; exit 1; }
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$repo/target}
# Build while the operator HOME is still available to rustup; no app is started here.
cargo build --offline --locked -p flea --bin flea
cargo test --offline --locked -p flea-gui --lib theme_gate --no-run
# Keep cargo/rustup locations explicit across the HOME swap. No user config, cache or state leaks in.
cargo_home=${CARGO_HOME:-$HOME/.cargo}
rustup_home=${RUSTUP_HOME:-$HOME/.rustup}
env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
    HOME="$test_root/home" XDG_CONFIG_HOME="$test_root/config" XDG_CACHE_HOME="$test_root/cache" \
    XDG_STATE_HOME="$test_root/state" XDG_DATA_HOME="$test_root/home/.local/share" XDG_RUNTIME_DIR="$test_root/runtime" \
    CARGO_HOME="$cargo_home" RUSTUP_HOME="$rustup_home" FLEA_BIN="$CARGO_TARGET_DIR/debug/flea" \
    FLEA_THEME_GATE_ROOT="$test_root" FLEA_THEME_STOCK_DIR="$stock" \
    cargo test --offline --locked -p flea-gui --lib theme_gate -- --nocapture --test-threads=1
printf 'GPUI_THEME_GATE PASS stock=%s contrast and live atomic replacement\n' "$expected"
