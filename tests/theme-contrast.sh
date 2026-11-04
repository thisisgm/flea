#!/usr/bin/env bash
. "$(dirname "$0")/headless-commons.sh"
# Applies installed stock palettes and Cool Dawn and Sunset to the real Theme, holding text, frames and cursor wash to their floors.
set -u
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1

verdict=0
fail() { printf 'FAIL %s\n' "$1"; verdict=1; }

stock_dir=/usr/share/omarchy/themes
palettes_expected=24
if ! command -v qs >/dev/null; then
    echo "theme-contrast.sh: qs is not installed, cannot draw the roles"
    exit 1
fi
[ -d "$stock_dir" ] || { echo "FAIL theme-contrast.sh: $stock_dir is not there, so the 22 stock palettes cannot be read"; exit 1; }
# Omarchy dev adds Starship to the 22 stable stock palettes; every other inventory change still fails.
if [ -f "$stock_dir/starship/colors.toml" ]; then palettes_expected=25; fi

# A marked sandbox of its own under the fixture root, so cleanup deletes only what this run owns.
test_root=$(mktemp -d "$FIXTURE_ROOT/flea-theme-contrast-XXXXXX") || exit 1
# GNU mktemp -d honours a relative TMPDIR verbatim, so the path is checked absolute and two components deep.
case $test_root in
  /*/*) ;;
  *) echo "FAIL: mktemp -d gave '$test_root', which is not an absolute path two components deep"; exit 1 ;;
esac
sandbox_require "$test_root" || exit 1
: > "$test_root/$SANDBOX_MARKER" || exit 1
cleanup() { sandbox_remove "$test_root"; }
trap cleanup EXIT

mkdir -p "$test_root/home/.config" "$test_root/state" "$test_root/data" "$test_root/cache" "$test_root/runtime" || exit 1
chmod 700 "$test_root/runtime" || exit 1

# Sample input, one line per palette: "nord<TAB>/usr/share/omarchy/themes/nord/colors.toml".
list="$test_root/palettes.tsv"
: > "$list"
for dir in "$stock_dir"/*/; do
    [ -f "${dir}colors.toml" ] && printf '%s\t%scolors.toml\n' "$(basename "$dir")" "$dir" >> "$list"
done
printf 'cool-dawn\t%s\n' "$PWD/tests/fixtures/cool-dawn/colors.toml" >> "$list"
printf 'sunset\t%s\n' "$PWD/tests/fixtures/sunset/colors.toml" >> "$list"
palettes=$(wc -l < "$list")
[ "$palettes" -eq "$palettes_expected" ] || fail "the sweep reads $palettes palettes, not $palettes_expected: a stock theme was added or removed, update palettes_expected here and paletteCount in tests/theme-contrast.qml with it"

run_timeout_s=120
# Per palette: 18 role checks, 2 glass checks, 6 R3 lift checks and 19 picked-mode checks, then 4 one-off checks.
# The transparent section draws opaque inks (alpha 255), so its 7 role and 3 graphics contrast checks per focus
# became one opaque-inks check: bare Transparent renders the theme's own palette, which translucent floors lifted.
expected_checks=$((palettes * 45 + 4))
config="$test_root/config"
mkdir -p "$config" || exit 1
# The probe imports ui/ as Flea, and ui/'s qs.Commons resolves against this root, as it does from ui/boot.
ln -s "$PWD/ui" "$config/flea" || exit 1
headless_commons "$test_root" "$config/Commons" || exit 1
ln -s "$(readlink -f ui/boot/Ui)" "$config/Ui" || exit 1
cp tests/theme-contrast.qml "$config/shell.qml" || exit 1
cp tests/button-probe.js "$config/button-probe.js" || exit 1
cp tests/qml-probe.js "$config/qml-probe.js" || exit 1
# The palettes ride into the harness as one library, each colors.toml body whole.
python3 -I - "$list" "$config/palettes.js" <<'PY' || exit 1
import json, sys
# Sample input: "nord<TAB>/usr/share/omarchy/themes/nord/colors.toml", one TSV line per palette.
rows = [line.rstrip("\n").split("\t") for line in open(sys.argv[1])]
with open(sys.argv[2], "w") as out:
    out.write(".pragma library\n\nvar PALETTES = " + json.dumps([{"name": n, "body": open(p).read()} for n, p in rows]) + "\n")
PY

output=$( ( env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
    HOME="$test_root/home" XDG_STATE_HOME="$test_root/state" XDG_CACHE_HOME="$test_root/cache" \
    XDG_DATA_HOME="$test_root/data" XDG_CONFIG_HOME="$test_root/home/.config" \
    XDG_RUNTIME_DIR="$test_root/runtime" BUTTONSYS_ROOT="$test_root" BUTTONSYS_TIMEOUT_S="$run_timeout_s" THEMECONTRAST_PALETTES="$palettes_expected" \
    QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_QUICK_BACKEND=software QT_QPA_UPDATE_IDLE_TIME=1 QT_FORCE_STDERR_LOGGING=1 \
    timeout "$run_timeout_s" qs -p "$config" 2>&1 ) 2>/dev/null )
# Sample input: "  INFO qml: THEMECONTRAST ok nord muted text on surface 5.02 >= 4.5" once per check, and "THEMECONTRAST DONE checks=484 failed=0" once.
passed=$(printf '%s\n' "$output" | grep -c 'THEMECONTRAST ok ')
failed=$(printf '%s\n' "$output" | grep -c 'THEMECONTRAST FAIL')
printf '%s\n' "$output" | grep -a 'THEMECONTRAST DONE' || true
# The glass floors print on a red run too, since they are what a red glass check is read against.
printf '%s\n' "$output" | grep -a 'THEMECONTRAST floor '
if [ "$failed" -ne 0 ]; then
    fail "theme-contrast.qml: $failed roles are under their floor"
    printf '%s\n' "$output" | grep -a 'THEMECONTRAST FAIL'
fi
if ! printf '%s\n' "$output" | grep -q "THEMECONTRAST DONE checks=$expected_checks failed=0"; then
    fail "theme-contrast.qml: the run did not report $expected_checks checks and 0 failed"
    printf '%s\n' "$output" | grep -aE 'THEMECONTRAST DONE|ERROR|error' | head -10
fi
[ "$passed" -eq "$expected_checks" ] || fail "theme-contrast.qml: the run passed $passed checks by name, not $expected_checks"
# The offscreen platform itself says it cannot mask a FloatingWindow; that one line is the platform's, never the theme's.
platform_warning='This plugin does not support setting window masks'
warnings=$(printf '%s\n' "$output" | grep -aE 'TypeError|ReferenceError|WARN' | grep -vF "$platform_warning")
if [ -n "$warnings" ]; then
    fail 'the theme harness logged a warning'
    printf '%s\n' "$warnings" | head -10
fi
if [ "$verdict" -ne 0 ]; then
    exit 1
fi
printf '%s\n' "$output" | grep -aE 'THEMECONTRAST (ok|info) '
printf 'THEMECONTRAST PASS %s checks over %s palettes, text at 4.5, frames at 3, the cursor row a plain wash\n' "$passed" "$palettes"
