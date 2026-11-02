#!/usr/bin/env bash
. "$(dirname "${BASH_SOURCE[0]}")/../tools/flea-magick-font"
. "$(dirname "$0")/headless-commons.sh"
# The preview geometry gate (AGENTS.md "The preview swap"); tests/preview-geometry.qml holds the rule table.
set -u
cd "$(dirname "$0")/.." || exit 1

for tool in qs magick; do
    command -v "$tool" >/dev/null || { echo "preview-geometry.sh: $tool is not installed"; exit 1; }
done

. "$PWD/tools/flea-sandbox-guard"
sandbox_root_ok
test_root="$SANDBOX_ROOT/flea-preview-geometry-$$"
sandbox_make "$test_root"
cleanup() {
    local result=$?
    trap - EXIT
    if [ "$result" -ne 0 ]; then
        printf 'preview-geometry: keeping %s\n' "$test_root"
        exit "$result"
    fi
    sandbox_remove "$test_root"
    exit "$result"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

mkdir -p "$test_root/config" "$test_root/home" "$test_root/state" "$test_root/runtime" "$test_root/fixture" "$test_root/out" || exit 1
chmod 700 "$test_root/runtime" || exit 1
# The probe imports ui/ as Flea, and ui/'s qs.Commons resolves against this root, as it does from ui/boot.
ln -s "$PWD/ui" "$test_root/config/flea" || exit 1
headless_commons "$test_root" || exit 1
ln -s "$(readlink -f ui/boot/Ui)" "$test_root/config/Ui" || exit 1
cp tests/preview-geometry.qml "$test_root/config/shell.qml" || exit 1

fixture="$test_root/fixture"
solid() { magick -size "$1" "xc:$2" "$fixture/$3" || { echo "preview-geometry.sh: fixture $3 failed"; exit 1; }; }
# Quick Look and original-leg images, exact pixel sizes.
solid 64x48 '#7aa2f7' img-64x48.png
solid 120x68 '#e0af68' img-120x68.png
solid 1920x1080 '#7aa2f7' img-1920x1080.png
solid 1080x1920 '#e0af68' img-1080x1920.png
solid 6000x4000 '#7aa2f7' img-6000x4000.png
# Cache-leg thumbnails, 256 px on the long side of the clip's aspect.
solid 256x144 '#3a8a5f' thumb-256x144.png
solid 144x256 '#3a8a5f' thumb-144x256.png
solid 256x171 '#3a8a5f' thumb-256x171.png
solid 256x256 '#3a8a5f' thumb-256x256.png
solid 256x192 '#3a8a5f' thumb-256x192.png
# Office embedded pictures: below cache size draws own-size, at it fills the frame.
solid 120x68 '#bb9af7' office-120x68.png
solid 181x256 '#bb9af7' office-181x256.png
# PDF pages, portrait and landscape, made the way case_preview makes its manual.pdf.
magick \( -size 400x560 xc:white -fill black "${flea_magick_font_args[@]}" -pointsize 40 -annotate +40+80 'PAGEONE' \) \
    "$fixture/doc-400x560.pdf" || { echo "preview-geometry.sh: portrait PDF fixture failed"; exit 1; }
magick \( -size 560x400 xc:white -fill black "${flea_magick_font_args[@]}" -pointsize 40 -annotate +40+80 'PAGEONE' \) \
    "$fixture/doc-560x400.pdf" || { echo "preview-geometry.sh: landscape PDF fixture failed"; exit 1; }

output=$(env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
    FLEA_PREVIEW_GEOMETRY_DIR="$fixture" FLEA_PREVIEW_GEOMETRY_OUT="$test_root/out" \
    HOME="$test_root/home" XDG_STATE_HOME="$test_root/state" XDG_RUNTIME_DIR="$test_root/runtime" \
    QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_QUICK_BACKEND=software QT_FORCE_STDERR_LOGGING=1 \
    timeout 180 qs -p "$test_root/config" 2>&1)

# Sample input, one probe line: "GEOMETRY columns-wide image 64x48 frame=736x460 drawn=64x48 rule=image-own-size ok".
expected_cells=37 # one cell per entry of cases in tests/preview-geometry.qml
cells=$(printf '%s\n' "$output" | grep -c '^.*GEOMETRY [a-z-]* [a-z]* [0-9x]* frame=')
ok_count=$(printf '%s\n' "$output" | grep -c ' rule=[a-z-]* ok$')
fail_count=$(printf '%s\n' "$output" | grep -c 'GEOMETRY FAIL')
done_count=$(printf '%s\n' "$output" | grep -c 'GEOMETRY DONE')
if [ "$cells" -ne "$expected_cells" ] || [ "$ok_count" -ne "$expected_cells" ] || [ "$fail_count" -ne 0 ] || [ "$done_count" -ne 1 ]; then
    printf 'FAIL the preview drew a picture at the wrong size: cells=%s ok=%s fail=%s\n' "$cells" "$ok_count" "$fail_count"
    printf '%s\n' "$output" | grep -aE 'GEOMETRY|ERROR|error' | head -40
    exit 1
fi
printf '%s\n' "$output" | grep -a 'GEOMETRY [a-z-]* [a-z]* [0-9x]* frame='

# The contact sheet: every cell grab in index order, titled by its own file name, for a human to look at.
mapfile -t grabs < <(ls "$test_root"/out/geometry-*.png | sort -V)
[ "${#grabs[@]}" -eq "$expected_cells" ] || { echo "preview-geometry.sh: want $expected_cells cell grabs, got ${#grabs[@]}"; exit 1; }
montage -label '%f' "${grabs[@]}" -tile 4x -geometry 320x240+4+4 "$test_root/sheet.png" \
    || { echo "preview-geometry.sh: the contact sheet failed"; exit 1; }
# The contact sheet outlives the sandbox, which cleanup removes on exit 0.
evidence_root=$(mktemp -d /tmp/flea-preview-geometry.XXXXXXXX) || exit 1
mv "$test_root/sheet.png" "$evidence_root/sheet.png" || { echo "preview-geometry.sh: moving the contact sheet to $evidence_root failed"; exit 1; }
printf 'GEOMETRY cells=%s ok=%s fail=%s\n' "$cells" "$ok_count" "$fail_count"
printf 'GEOMETRY_SHEET %s\n' "$evidence_root/sheet.png"
