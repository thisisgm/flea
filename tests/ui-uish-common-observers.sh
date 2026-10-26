#!/usr/bin/env bash
# Check the GPUI theme assertion and real window readers; native pixels and input remain controller work.
set -euo pipefail
cd "$(dirname "$0")/.."

if [[ "${1:-}" == --controls ]]; then
    root=${2:?marked root required}
    [[ "$root" == "$PWD/.superpowers/m9-uish-04/"?* && -f "$root/.flea-test-sandbox" ]] || exit 2
    fail() { printf '%s\n' "$*" >&2; exit 1; }
    assert_theme() { printf 'QML oracle\n'; }
    test_frontend=qml
    source tests/ui-uish/common-observers.sh
    [[ "$(assert_theme)" == 'QML oracle' ]]
    test_frontend=gpui
    source tests/ui-uish/common-observers.sh
    real_state_dir="$root/current"
    mkdir -p "$real_state_dir/theme"
    flea_ui="$root/unused"
    ipc_pid=42
    flea_ipc_wait() { [[ "$1 $2" == '--pid 42' ]]; }
    sleep() { :; }
    observed_loaded=true
    observed_foreground='#c1c497'
    ipc() {
        case "$1" in
            themeLoaded) printf '%s\n' "$observed_loaded" ;;
            themeForeground) printf '%s\n' "$observed_foreground" ;;
            *) return 2 ;;
        esac
    }
    printf 'foreground = "#c1c497"\naccent = "#d28a41"\n' > "$real_state_dir/theme/colors.toml"
    assert_theme
    printf 'foreground = "#FAFCFB"\naccent = "#123456"\n' > "$real_state_dir/theme/colors.toml"
    observed_foreground='#fafcfb'
    assert_theme
    [[ "$real_foreground" == '#FAFCFB' && "$real_accent" == '#123456' ]]
    observed_foreground='#c1c497'
    if (assert_theme) > "$root/stale.log" 2>&1; then fail 'stale foreground accepted'; fi
    grep -q "not the live theme's #FAFCFB" "$root/stale.log"
    observed_foreground='#fafcfb'
    observed_loaded=false
    if (assert_theme) > "$root/unloaded.log" 2>&1; then fail 'unloaded theme accepted'; fi
    grep -q "themeLoaded is 'false'" "$root/unloaded.log"
    printf 'foreground = "invalid"\naccent = "#123456"\n' > "$real_state_dir/theme/colors.toml"
    if (assert_theme) > "$root/invalid.log" 2>&1; then fail 'invalid palette accepted'; fi
    grep -q 'no foreground' "$root/invalid.log"
    printf 'UISH_04 HARNESS PASS 6 controls: QML, current, changed, stale, unloaded, invalid\n'
    exit 0
fi

scratch="$PWD/.superpowers/m9-uish-04"
mkdir -p "$scratch"
root=$(mktemp -d "$scratch/controls.XXXXXXXX")
: > "$root/.flea-test-sandbox"
trap 'case $root in "$scratch"/controls.?*) [ -f "$root/.flea-test-sandbox" ] && rm -rf -- "$root" ;; esac' EXIT
mkdir -p "$root"/{home,config,state,cache,runtime}
setsid -w env -i PATH="$PATH" HOME="$root/home" \
    XDG_CONFIG_HOME="$root/config" XDG_STATE_HOME="$root/state" \
    XDG_CACHE_HOME="$root/cache" XDG_RUNTIME_DIR="$root/runtime" \
    /bin/bash "$PWD/tests/ui-uish-common-observers.sh" --controls "$root" < /dev/null
for target in integration mounted; do
    args=(--test uish_04)
    [[ "$target" != mounted ]] || args=(--lib native_reader_tests)
    setsid -w env -i PATH="$PATH" HOME="$root/home" \
        XDG_CONFIG_HOME="$root/config" XDG_STATE_HOME="$root/state" \
        XDG_CACHE_HOME="$root/cache" XDG_RUNTIME_DIR="$root/runtime" \
        CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}" RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}" \
        CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target}" \
        CARGO_PROFILE_TEST_DEBUG=line-tables-only CARGO_PROFILE_DEV_DEBUG=line-tables-only \
        cargo test --locked -p flea-gui "${args[@]}" -- --nocapture < /dev/null
done
printf 'UISH_04 PASS live palette, 34 tokens, applied wheel gain and refusal controls\n'
