#!/usr/bin/env bash
set -eu
cd "$(dirname "$0")/.." || exit 1

ui_dir=${FLEA_EMPTY_STATE_UI_DIR:-ui}
shell_file="$ui_dir/shell.qml"
column_file="$ui_dir/ColumnPane.qml"
tip='Press Ctrl+Shift+N for a new folder.'

fail() {
    printf 'empty-state: %s\n' "$1" >&2
    exit 1
}

[ -r "$shell_file" ] || fail "cannot read $shell_file"
[ -r "$column_file" ] || fail "cannot read $column_file"

# The tip is the Menus section's "Show keyboard hints" row, off by default, so an ordinary empty
# folder is as quiet as it was when the tip was removed outright. A copy of it anywhere that does
# not name that setting is a tip nothing can switch off.
if grep -Fq "$tip" "$column_file"; then
    fail 'the columns empty tile carries the new-folder tip'
fi
ungated=$(grep -F "$tip" "$shell_file" | grep -Fcv 'ViewState.keyHints' || true)
if [ "$ungated" != 0 ]; then
    fail 'the new-folder tip is drawn without the keyboard-hints setting'
fi

shell_hint=$(sed -n '/hint: pane.searchMode === "results"/,/^[[:space:]]*}/p' "$shell_file")
shell_hint=$(printf '%s' "$shell_hint" | tr -s '[:space:]' ' ')
expected_hint='hint: pane.searchMode === "results" ? "Press Escape to clear." : ViewState.keyHints ? "Press Ctrl+Shift+N for a new folder." : ""'
case "$shell_hint" in
    *"$expected_hint"*) ;;
    *) fail 'shell hint must show Escape for search results and the tip only behind the setting' ;;
esac

column_empty=$(sed -n '/id: emptyTile/,/^[[:space:]]*}/p' "$column_file")
[ -n "$column_empty" ] || fail 'ColumnPane emptyTile is missing'
if printf '%s\n' "$column_empty" | grep -Eq '^[[:space:]]*hint:'; then
    fail 'ColumnPane emptyTile must not bind a hint'
fi

printf 'empty-state: search hint always, new-folder tip gated, column hint absent\n'
