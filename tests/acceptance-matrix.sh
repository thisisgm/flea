#!/usr/bin/env bash
# Display-free derivation, guard and accounting check; GNU coreutils matches the native guard.
set -euo pipefail
REPO=$(cd "$(dirname "$0")/.." && pwd)
FLEA_FIXTURE_ROOT=$(mktemp -d /tmp/flea-acceptance-check.XXXXXXXX)
export FLEA_FIXTURE_ROOT
. "$REPO/tools/flea-sandbox-guard"
. "$REPO/tools/flea-acceptance-drive"
die() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }
SB="$FLEA_FIXTURE_ROOT/case"
printf 'acceptance derivation self-check\n' > "$FLEA_FIXTURE_ROOT/$SANDBOX_MARKER"
sandbox_make "$SB"
SB=$(realpath -e "$SB")
trap 'drive_guard "$SB/work"; sandbox_remove "$SB"' EXIT
DRIVE_FIXTURE="$SB/work"
DRIVE_EVIDENCE="$SB/evidence"
drive_guard "$DRIVE_FIXTURE"
mkdir "$DRIVE_FIXTURE" "$DRIVE_EVIDENCE"
python3 "$REPO/tools/flea-keymap-gen" "$SB/generated.js" "$REPO/keys.toml"
cmp "$SB/generated.js" "$REPO/ui/js/Keymap.js"
BINDINGS="$SB/bindings"
derive_bindings > "$BINDINGS"
python3 - "$BINDINGS" <<'PY'
import collections, pathlib, sys
rows = [line.split('|') for line in pathlib.Path(sys.argv[1]).read_text().splitlines()]
assert rows and all(len(row) == 7 for row in rows)
assert len(rows) == len({row[0] for row in rows})
assert {row[1] for row in rows} == {'default', 'vim', 'mac', 'windows'}
assert {row[2] for row in rows} == {'gui', 'tui'}
assert {row[3] for row in rows} == {'listing', 'rail', 'menu', 'panel', 'preview', 'pdf', 'media', 'editor'}
def action(preset, frontend, context, modifier, key):
    matches = [row[6] for row in rows if row[1:6] == [preset, frontend, context, modifier, key]]
    assert len(matches) == 1, (preset, frontend, context, modifier, key, matches)
    return matches[0]
assert action('mac', 'gui', 'listing', 'ctrl', 'X') == ''
assert action('windows', 'gui', 'listing', 'ctrl', 'X') == 'cut'
assert action('vim', 'gui', 'listing', 'text', 'y') == 'copyArm'
assert action('default', 'gui', 'menu', 'none', 'Return') == 'open'
assert action('mac', 'gui', 'listing', 'none', 'Return') == 'rename'
assert action('mac', 'gui', 'menu', 'none', 'Return') == 'open'
assert action('default', 'gui', 'listing', 'text', '1') == ''
assert action('default', 'tui', 'listing', 'text', '1') == 'tab1'
assert action('default', 'gui', 'listing', 'ctrl', 'Insert') == ''
assert action('default', 'tui', 'listing', 'ctrl', 'Insert') == 'copy'
assert action('default', 'gui', 'listing', 'ctrl', 'N') == 'windowNew'
assert action('default', 'tui', 'listing', 'ctrl', 'N') == ''
assert action('default', 'gui', 'pdf', 'shift', 'Tab') == 'focusPrevious'
assert action('default', 'gui', 'pdf', 'shift', 'Backtab') == 'focusPrevious'
for preset in ('default', 'vim', 'mac', 'windows'):
    for frontend in ('gui', 'tui'):
        for context in ('menu', 'panel', 'preview', 'pdf', 'media', 'editor'):
            assert action(preset, frontend, context, 'ctrl', 'X') == ''
            assert action(preset, frontend, context, 'none', 'Delete') == ''
print('ACCEPTANCE_MATRIX rows=' + str(len(rows)) + ' frontends=' + str(dict(collections.Counter(row[2] for row in rows))))
PY
if command -v node >/dev/null && command -v qml6 >/dev/null; then
  FLEA_MATRIX_ENGINE=node derive_bindings > "$SB/node-bindings"
  FLEA_MATRIX_ENGINE=qml6 derive_bindings > "$SB/qt-bindings"
  cmp "$SB/node-bindings" "$SB/qt-bindings"
  printf 'ACCEPTANCE_MATRIX engines=node,qml6 identical\n'
else
  printf 'ACCEPTANCE_MATRIX cross-engine comparison unavailable; installed engine derivation only\n'
fi
derive_menu_actions > "$SB/menus"
python3 - "$SB/menus" <<'PY'
import pathlib, sys
rows = [line.split('|') for line in pathlib.Path(sys.argv[1]).read_text().splitlines()]
assert all(len(row) == 4 for row in rows)
assert len(rows) == len({(row[0], row[3]) for row in rows})
# 30, derived by running the suite's own derive_menu_actions and counting the distinct non-header
# ids in its output, not by counting declarations in Menu.js. It was 29 and drifted when 0.2.0 added
# Move to, Copy to, Properties, Permissions, New file, Delete permanently and Add to Favorites.
assert len({row[0] for row in rows if row[3] != 'header'}) == 30
assert ['delete', 'deletePermanently', 'Delete permanently', 'file'] in rows
assert ['moveto', 'moveTo', 'Move to', 'file'] in rows
assert ['openwith', 'openWith', 'Open with', 'file'] in rows
assert {row[3] for row in rows if row[0] == 'open'} == {'file', 'trash'}
assert {row[3] for row in rows if row[0] == 'paste'} == {'file', 'background'}
assert {row[0] for row in rows if row[3] == 'header'} == {'col:mode', 'col:size', 'col:date', 'col:kind'}
print('ACCEPTANCE_MENU ids=30 header=4 context_rows=' + str(len(rows)))
PY
mkdir -p "$SB/broken-menu/ui/js"
printf 'var INVENTORY = [["open","Open","folder","F","open"],["broken"]]\n' > "$SB/broken-menu/ui/js/Menu.js"
if (REPO="$SB/broken-menu"; derive_menu_actions) > "$SB/partial-menu" 2> "$SB/partial-menu-error"; then
  die "invalid menu inventory was accepted"
fi
[[ ! -s "$SB/partial-menu" ]] || die "invalid menu inventory left a partial checklist"

status=0
output=$(FLEA_BIN=/missing/acceptance-candidate bash "$REPO/tools/flea-acceptance" --drive typo 2>&1) || status=$?
[[ "$status" == 2 && "$output" == 'unknown --drive group: typo' ]] \
  || die "unknown group reached setup or returned a successful result: $status $output"
if (DRIVE_GROUPS=typo; drive_all) >/dev/null 2>&1; then die "direct driver accepted an unknown group"; fi
drive_validate_groups key menu omarchy pointer

for key in Backtab BracketLeft BracketRight End Equal F10 Home Insert Menu Minus PageDown PageUp Plus Underscore; do
  [[ -n "$(keysym_for "$key")" ]] || die "missing delivery mapping: $key"
done
for modifier in none ctrl shift ctrlshift alt super supershift superalt; do
  [[ -n "$(chord_args "$modifier" A)" ]] || die "missing modifier delivery: $modifier"
done
if keysym_for InventedKey; then die "unknown key was silently mapped"; fi
if chord_args invented A; then die "unknown modifier was silently mapped"; fi
DRIVE_PRESET=mac
[[ "$(first_chord open menu)" == $'-k\nReturn' ]] || die "Mac menu setup inherited listing Right"
DRIVE_PRESET=vim
[[ "$(first_chord cursorFirst)" == $'-k\nHome' ]] || die "Vim setup ignored the direct Home alternative"
all_bindings="$BINDINGS"
BINDINGS="$SB/pair-only"
printf 'pair|vim|gui|listing|text|g|cursorFirstArm\n' > "$BINDINGS"
[[ "$(first_chord cursorFirst)" == $'g\ng' ]] || die "sequence-only setup did not deliver the required pair"
BINDINGS="$all_bindings"

for path in '' relative "$SB" "$FLEA_FIXTURE_ROOT/foreign" "$SB/work/../../outside"; do
  if (drive_guard "$path") >/dev/null 2>&1; then die "guard accepted unsafe path: $path"; fi
done
ln -s "$FLEA_FIXTURE_ROOT/foreign" "$SB/escape"
if (drive_guard "$SB/escape/file") >/dev/null 2>&1; then die "guard followed an escaping symlink"; fi
drive_mutation_guard "$DRIVE_FIXTURE/child"
if drive_mutation_guard "$SB/escape/file"; then die "native operation escaped the listing"; fi

# These wrong-state controls prove assertions reject equal counts, wrong members and reversed direction.
(
  ipc() { case "$1" in cursor) printf '%s' "$test_cursor" ;; selectedIndices) printf '%s' "$test_selection" ;; esac; }
  test_cursor=1 test_selection=0,2
  if expect_selection 1 0,1; then die "selection accepted the wrong members with the correct count"; fi
  test_cursor=2 test_selection=0,1
  if expect_selection 1 0,1; then die "selection accepted the wrong cursor"; fi
  test_cursor=1
  expect_selection 1 0,1
)
(
  reset_state() { test_tabs=1 test_tab=0; }
  drive_action() { test_tabs=$((test_tabs + 1)); test_tab=$((test_tabs - 1)); }
  settle() { :; }
  ipc() { case "$1" in tabIndex) printf '%s' "$test_tab" ;; tabCount) printf '%s' "$test_tabs" ;; esac; }
  k() { test_tab=$(((test_tab + $1 + test_tabs) % test_tabs)); }
  keycase_tabNext 1
  keycase_tabPrevious -1
  if keycase_tabNext -1; then die "next tab accepted the previous direction"; fi
  if keycase_tabPrevious 1; then die "previous tab accepted the next direction"; fi
)
(
  reset_state() { test_cursor=0; }
  drive_action() { :; }
  drive_expect() { :; }
  settle() { :; }
  ipc() {
    case "$1" in
      settingsCursor) printf '%s' "$test_cursor" ;;
      settingsModel) printf '[{"kind":"check"},{"kind":"hint"},{"kind":"choice"},{"kind":"ruler","on":false},{"kind":"action"}]' ;;
    esac
  }
  k() {
    case "$*" in '-k Down'|down) test_cursor=2 ;; up) test_cursor=0 ;; wrong) test_cursor=4 ;; esac
  }
  drive_panel_key cursorDown down
  drive_panel_key cursorUp up
  if drive_panel_key cursorDown wrong; then die "panel accepted a skipped eligible control"; fi
  if drive_panel_key cursorUp wrong; then die "panel accepted reversed direction"; fi
)
(
  reset_state() { test_phase=before; }
  omarchy_press() { test_phase=after; }
  drive_owned_pids() { :; }
  hyprctl() {
    [[ "$1" == clients ]] || die "foreign window received a compositor mutation"
    if [[ "$test_phase" == before ]]; then printf '[]\n'; else printf '[{"address":"0xforeign","pid":4242}]\n'; fi
  }
  status=0
  omarchycase_super_return || status=$?
  [[ "$status" == 2 ]] || die "unowned compositor terminal was accepted"
)
(
  drive_window_identity() { printf '{"address":"0xowned","pid":10}\n'; }
  drive_mutation_guard() { :; }
  ipc() { printf '%s' "$DRIVE_FIXTURE"; }
  hyprctl() { printf '{"address":"0xowned","pid":11}\n'; }
  DRIVE_WINDOW_ADDRESS=0xowned
  omarchy-drive() {
    [[ "$*" == 'focus 0xowned' ]] || die "global chord reached a foreign or class-only input target"
  }
  if omarchy_press super_return; then die "global chord accepted a changed focused PID"; fi
)
mkdir "$SB/matrix-bin"
printf '#!/bin/sh\nprintf "qml: ReferenceError: missing matrix\\n"\nexit 0\n' > "$SB/matrix-bin/qml6"
chmod +x "$SB/matrix-bin/qml6"
if (PATH="$SB/matrix-bin:$PATH" FLEA_MATRIX_ENGINE=qml6 derive_bindings) > "$SB/failed-qt.log" 2>&1; then
  die "Qt derivation accepted exit zero without its execution tally"
fi

if (
  drive_window_identity() { printf '{}\n'; }
  omarchy-drive() { return 0; }
  drive_shot no-output
) >/dev/null 2>&1; then die "screenshot helper accepted success without a new file"; fi
if (
  drive_window_identity() { printf '{}\n'; }
  omarchy-drive() { drive_guard "$2"; printf '\211PNG\r\n\032\n' > "$2"; return 1; }
  drive_shot failed-renderer
) >/dev/null 2>&1; then die "screenshot helper ignored renderer failure after output creation"; fi
printf 'old artifact\n' > "$DRIVE_EVIDENCE/$DRIVE_ROW_ID-1-stale.png"
if (
  drive_window_identity() { printf '{}\n'; }
  omarchy-drive() { return 0; }
  drive_shot stale
) >/dev/null 2>&1; then die "screenshot helper accepted a previous artifact"; fi

# These stubs exercise result accounting only; they never masquerade as native evidence.
PASSED=0 FAILED=0 UNDRIVEN=0
FLEA_SOURCE_SHA=self-check FLEA_BINARY_SHA256=self-check
pass() { PASSED=$((PASSED + 1)); }
bad() { FAILED=$((FAILED + 1)); }
skip() { UNDRIVEN=$((UNDRIVEN + 1)); }
keycase_matrixPass() { return 0; }
keycase_matrixFail() { WHY='intentional negative control'; return 1; }
keycase_matrixPending() { WHY='intentional missing observer'; return 2; }
drive_key_row 'pass|default|gui|listing|none|Return|matrixPass' || die 'result accounting failed'
drive_key_row 'fail|default|gui|listing|none|Return|matrixFail' || die 'result accounting failed'
drive_key_row 'pending|default|gui|listing|none|Return|matrixPending' || die 'result accounting failed'
drive_key_row 'terminal|default|tui|listing|none|Return|matrixPass' || die 'result accounting failed'
[[ "$PASSED/$FAILED/$UNDRIVEN" == 1/1/2 ]] || die "per-row accounting hid an incomplete binding"
jq -se 'length == 4 and (map(.status) == ["PASS","FAIL","UNDRIVEN","UNDRIVEN"])' "$DRIVE_EVIDENCE/keys.jsonl" >/dev/null
(
  PASSED=0 FAILED=0 UNDRIVEN=0
  hyprctl() { printf '[{"modmask":64,"key":"Return"}]\n'; }
  omarchycase_super_return() { WHY='accounting control'; return "$test_result"; }
  for test_result in 0 1 2; do omarchy_general_case super_return || die "Omarchy caller accounting failed"; done
  [[ "$PASSED/$FAILED/$UNDRIVEN" == 1/1/1 ]] || die "Omarchy caller misclassified a required undriven chord"
)
printf 'ACCEPTANCE_SELF_CHECK passed; no native coverage claimed.\n'
