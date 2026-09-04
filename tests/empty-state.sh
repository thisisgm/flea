#!/usr/bin/env bash
set -eu
cd "$(dirname "$0")/.." || exit 1

ui_dir=${FLEA_EMPTY_STATE_UI_DIR:-ui}
shell_file="$ui_dir/shell.qml"
column_file="$ui_dir/ColumnPane.qml"
unwanted='Press Ctrl+Shift+N for a new folder.'

fail() {
    printf 'empty-state: %s\n' "$1" >&2
    exit 1
}

[ -r "$shell_file" ] || fail "cannot read $shell_file"
[ -r "$column_file" ] || fail "cannot read $column_file"

if grep -Fq "$unwanted" "$shell_file" "$column_file"; then
    fail 'ordinary empty folder still shows new-folder hint'
fi

shell_hint=$(sed -n '/hint: pane.searchMode === "results"/,/^[[:space:]]*}/p' "$shell_file")
shell_hint=$(printf '%s' "$shell_hint" | tr -s '[:space:]' ' ')
expected_hint='hint: pane.searchMode === "results" ? "Press Escape to clear." : ""'
case "$shell_hint" in
    *"$expected_hint"*) ;;
    *) fail 'shell hint must show Escape only for search results' ;;
esac

column_empty=$(sed -n '/id: emptyTile/,/^[[:space:]]*}/p' "$column_file")
[ -n "$column_empty" ] || fail 'ColumnPane emptyTile is missing'
if printf '%s\n' "$column_empty" | grep -Eq '^[[:space:]]*hint:'; then
    fail 'ColumnPane emptyTile must not bind a hint'
fi

printf 'empty-state: search hint scoped; ordinary and column hints absent\n'
