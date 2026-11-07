#!/usr/bin/env bash
# Exercise real GPUI windows with isolated Mesa drivers on a headless Wayland compositor.
set -euo pipefail
cd "$(dirname "$0")/.."
for tool in python3 cc bwrap Hyprland hyprctl start-hyprland; do
    command -v "$tool" >/dev/null || { printf 'FAIL gui-render-fallback: missing prerequisite %s; no packages were changed\n' "$tool"; exit 1; }
done
printf 'gui-render-fallback: host=%s compositor=Hyprland launcher=start-hyprland output=headless\n' "$(hostname)"
cargo build --locked --workspace
mkdir -p "$PWD/.superpowers/u2"
test_root=$(mktemp -d "$PWD/.superpowers/u2/render.XXXXXXXX")
: > "$test_root/.flea-test-sandbox"
python3 tests/gui-render-fallback.py "$test_root"
printf 'gui-render-fallback: all five renderer cases passed\n'
printf 'gui-render-fallback: both Gen6 OpenGL cases passed\n'
