#!/usr/bin/env bash
# Offline TUI regressions and fake adapters, with private inputs and no display dependency.
set -euo pipefail
repo=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
mkdir -p "$repo/.superpowers/t12a"
scratch=$(mktemp -d "$repo/.superpowers/t12a/ci.XXXXXXXX")
printf '%s\n' 'owned offline TUI battery' > "$scratch/.flea-test-sandbox"
cleanup() {
    [[ -n "$scratch" && "$scratch" == /* && "$scratch" == "$repo/.superpowers/t12a/ci."* && -f "$scratch/.flea-test-sandbox" ]] && rm -rf -- "$scratch"
}
trap cleanup EXIT
mkdir -p "$scratch"/{home,config,cache,data,state,runtime,tmp,tests}
chmod 700 "$scratch/runtime"
env -i PATH=/usr/bin:/bin HOME="$scratch/home" LANG=C.UTF-8 LC_ALL=C.UTF-8 TZ=UTC \
    XDG_CONFIG_HOME="$scratch/config" XDG_CACHE_HOME="$scratch/cache" XDG_DATA_HOME="$scratch/data" \
    XDG_STATE_HOME="$scratch/state" XDG_RUNTIME_DIR="$scratch/runtime" TMPDIR="$scratch/tmp" \
    PYTHONPATH="$repo/tests" PYTHONDONTWRITEBYTECODE=1 FLEA_BATTERY_TEST_ROOT="$scratch/tests" \
    FLEA_BATTERY_BOARDS="$scratch/references" \
    /usr/bin/setsid -w /usr/bin/python3 -B -m tui_battery.offline "$scratch" < /dev/null
