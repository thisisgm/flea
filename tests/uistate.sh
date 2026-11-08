#!/bin/bash
# Drives the real binary against ui.json: main() is the only place both front ends' one update path
# is reachable from, and the lock is only a lock across processes.
set -u
# Hard rule 9's guard, which owns FIXTURE_ROOT and every create and delete below.
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1

BIN=./target/debug/flea
# A clean git archive export carries no target/, and without this every case below runs against a
# missing binary and reports them as product failures.
if [ ! -x "$BIN" ]; then
    printf 'uistate.sh: no binary at %s\n' "$BIN" >&2
    printf 'uistate.sh: build it (cargo build); refusing to report on nothing\n' >&2
    exit 1
fi
SANDBOX=$FIXTURE_ROOT/uistate-$$
fail=0

check() {
  local label="$1" expected="$2" actual="$3"
  if [ "$expected" != "$actual" ]; then
    echo "FAIL $label"
    echo "  expected: $expected"
    echo "  actual:   $actual"
    fail=1
  else
    echo "ok   $label"
  fi
}

# Every invocation below writes inside the sandbox and never in the operator's own ~/.local/state.
fresh() {
  sandbox_scratch "$SANDBOX/run" || exit 1
  STATE=$SANDBOX/run/state
  CONFIG=$SANDBOX/run/config
  UI=$STATE/flea/ui.json
  mkdir -p "$STATE" "$CONFIG" || exit 1
}

flea_ui() {
  env XDG_STATE_HOME="$STATE" XDG_CONFIG_HOME="$CONFIG" $BIN --ui-state "$@" </dev/null
}

plugins_value() {
  python3 -c 'import json,sys; print(json.dumps(json.load(sys.stdin)["plugins"], sort_keys=True, separators=(",", ":")))'
}

sandbox_make "$SANDBOX" || exit 1
mkdir -p "$SANDBOX/bin" || exit 1
cp "$BIN" "$SANDBOX/bin/flea" || exit 1
BIN="$SANDBOX/bin/flea"
cat > "$SANDBOX/bin/flea-gui" <<'WINDOW_PY'
#!/usr/bin/python3
import os
from pathlib import Path

capture = os.environ.get("UI_STATE_TEST_CAPTURE")
if capture:
    state = Path(os.environ["XDG_STATE_HOME"]) / "flea/ui.json"
    Path(capture).write_bytes(state.read_bytes())
print("UI_STATE_TEST_WINDOW_STARTED")
WINDOW_PY
chmod +x "$SANDBOX/bin/flea-gui" || exit 1

# A read answers the whole shape and writes nothing: a first launch leaves no file behind.
fresh
out=$(flea_ui 2>&1); rc=$?
check "a read exits 0" "0" "$rc"
check "a read answers the shipped view" "1" "$(echo "$out" | grep -c '"view": "list"')"
check "a read answers the shipped menu.hidden" "1" "$(echo "$out" | grep -c '"copyAs"')"
check "a read ships Open folders while dragging off" "1" "$(echo "$out" | grep -c '"openFoldersWhileDragging": false')"
check "a read answers every top-level key" "44" "$(echo "$out" | grep -c '^  "')"
check "a fresh read answers empty plugin state" "{}" "$(printf '%s\n' "$out" | plugins_value)"
check "a read leaves no state file behind" "0" "$([ -e "$UI" ] && echo 1 || echo 0)"

# The window paints a first launch before any file exists, so the two fallbacks it holds have to be
# the shipped ones. A drift here is a fresh install whose menu and columns disagree with the state
# file its own next write produces, which is the two-authority defect this file is here to prevent.
# Both sides are read off the artefacts: the QML literals, and the document the binary just printed.

# Sample input: `    readonly property var defaultColumns: ["name", "size", "date"]`, and the same
# declaration wrapped over two lines. Reads from the declaration to the line that closes the array.
qml_list() {
  local key="$1" line on=0 held=""
  while IFS= read -r line; do
    case "$line" in *"readonly property var $key:"*) on=1 ;; esac
    [ "$on" = 1 ] || continue
    held="$held $line"
    case "$line" in *']'*) break ;; esac
  done < ui/ViewState.qml
  printf '%s\n' "$held" | grep -o '"[a-zA-Z]*"' | tr -d '"' | sort | tr '\n' ' '
}

# Sample input: the pretty document flea --ui-state prints, whose arrays open on `  "columns": [`
# and hold one quoted id per line until a line closing the bracket.
json_list() {
  local key="$1" line on=0 held=""
  while IFS= read -r line; do
    if [ "$on" = 0 ]; then
      case "$line" in *"\"$key\": ["*) on=1 ;; esac
      continue
    fi
    case "$line" in *']'*) break ;; esac
    held="$held $line"
  done <<JSONEOF
$out
JSONEOF
  printf '%s\n' "$held" | grep -o '"[a-zA-Z]*"' | tr -d '"' | sort | tr '\n' ' '
}
check "the window's own column fallback is the shipped one" "$(json_list columns)" "$(qml_list defaultColumns)"
check "and its menu.hidden fallback is too" "$(json_list hidden)" "$(qml_list defaultMenuHidden)"

# A patch is the write, and it prints what it stored so a caller needs no second read.
out=$(flea_ui '{"view":"grid","places":{"sidebarWidth":240}}' 2>&1); rc=$?
check "a patch exits 0" "0" "$rc"
check "a patch prints the stored view" "1" "$(echo "$out" | grep -c '"view": "grid"')"
check "a patch writes the file" "1" "$([ -f "$UI" ] && echo 1 || echo 0)"
# 240 is not a stop, so src/uistate.rs Rule::SidebarWidth snaps it down to 224: this asserted the
# raw number and had been failing since the snap was written, which is why it pins the snap now.
check "the stored file carries the patched width, snapped to a stop" "1" "$(grep -c '"sidebarWidth": 224' "$UI")"
check "the stored file keeps every other key" "44" "$(grep -c '^  "' "$UI")"
check "a view patch keeps empty plugin state" "{}" "$(plugins_value < "$UI")"
check "the state file is owner only" "600" "$(stat -c '%a' "$UI")"
check "the state directory is owner only" "700" "$(stat -c '%a' "$STATE/flea")"
# ls -A: ui.json and its lock, and no temp file left behind by the rename.
check "no temp file survives the write" "ui.json ui.json.lock" "$(ls -A "$STATE/flea" | sort | tr '\n' ' ' | sed 's/ $//')"

flea_ui '{"plugins":{"fixture.plugin":{"enabled":true,"settings":{"value":"kept"}}}}' >/dev/null 2>&1; rc=$?
check "a plugin state patch exits 0" "0" "$rc"

# A second patch merges rather than replacing, which is what "caller-key merge" has to mean.
flea_ui '{"hidden":true}' >/dev/null 2>&1
check "the earlier key survives the later patch" "1" "$(grep -c '"view": "grid"' "$UI")"
check "the later key landed" "1" "$(grep -c '"hidden": true' "$UI")"
check "a write of another key preserves plugin state" '{"fixture.plugin":{"enabled":true,"settings":{"value":"kept"}}}' "$(plugins_value < "$UI")"

# A key from a newer Flea is kept and rewritten as it was read, so an older Flea cannot eat it.
fresh
mkdir -p "$STATE/flea"
printf '{"fromANewerFlea":{"a":[1,"two"]},"view":"columns"}\n' > "$UI"
flea_ui '{"hidden":true}' >/dev/null 2>&1
check "an unknown key survives a write" "1" "$(grep -c '"fromANewerFlea"' "$UI")"
check "an unknown key keeps its own value" "1" "$(grep -c '"two"' "$UI")"
check "a known key beside it survives" "1" "$(grep -c '"view": "columns"' "$UI")"

# A file this cannot parse costs the whole file, not the process.
fresh
mkdir -p "$STATE/flea"
printf '{ this is not json\n' > "$UI"
out=$(flea_ui 2>&1); rc=$?
check "a malformed file still exits 0" "0" "$rc"
check "a malformed file reads as the defaults" "1" "$(echo "$out" | grep -c '"view": "list"')"

# An unknown value costs one key and every other key in the file stands.
fresh
mkdir -p "$STATE/flea"
printf '{"view":"miller","density":"comfortable","hidden":true}\n' > "$UI"
out=$(flea_ui 2>&1)
check "an unknown value falls back to its default" "1" "$(echo "$out" | grep -c '"view": "list"')"
check "the key beside it is untouched" "1" "$(echo "$out" | grep -c '"density": "comfortable"')"
check "the second key beside it is untouched" "1" "$(echo "$out" | grep -c '"hidden": true')"

# A caller that sends junk is told which key, and nothing is half applied.
fresh
out=$(flea_ui '{"view":"miller"}' 2>&1); rc=$?
check "a bad patch value exits 2" "2" "$rc"
check "a bad patch value names its key" "1" "$(echo "$out" | grep -c 'view')"
check "a refused patch writes no state file" "0" "$([ -e "$UI" ] && echo 1 || echo 0)"
# ls -A: update() makes the directory and takes the lock before patched() validates the patch, so a
# refused one leaves both behind and the state file itself is the only thing it never writes.
check "a refused patch leaves the directory and the lock it took" "ui.json.lock" "$(ls -A "$STATE/flea" | sort | tr '\n' ' ' | sed 's/ $//')"
out=$(flea_ui '{"openFoldersWhileDragging":"yes"}' 2>&1); rc=$?
check "a non-boolean Open folders while dragging exits 2 and names itself" "2 1" "$rc $(echo "$out" | grep -c 'openFoldersWhileDragging')"
out=$(flea_ui '{"notAKey":1}' 2>&1); rc=$?
check "an unknown patch key exits 2" "2" "$rc"
check "an unknown patch key names itself" "1" "$(echo "$out" | grep -c 'notAKey')"
out=$(flea_ui 'not json at all' 2>&1); rc=$?
check "a patch that is not JSON exits 2" "2" "$rc"
out=$(env XDG_STATE_HOME="$STATE" XDG_CONFIG_HOME="$CONFIG" $BIN --ui-state a b </dev/null 2>&1); rc=$?
check "two arguments is a usage error" "2" "$rc"

# The contract ui/ViewState.qml's Process reads, which is the status and never the child's output:
# the merged document on stdout at 0, one `flea: ` sentence on stderr at 2, and nothing on the other
# stream either way. Nothing pinned the split, so a refusal printed to stdout would have passed.
fresh
merged=$(flea_ui '{"view":"grid"}' 2>/dev/null); rc=$?
check "an accepted patch exits 0" "0" "$rc"
check "an accepted patch prints the document on stdout" "1" "$(echo "$merged" | grep -c '"view": "grid"')"
check "an accepted patch prints nothing on stderr" "" "$(flea_ui '{"view":"grid"}' 2>&1 >/dev/null)"
check "a refused patch prints its sentence on stderr" "1" "$(flea_ui '{"view":"miller"}' 2>&1 >/dev/null | grep -c '^flea: ')"
check "a refused patch prints nothing on stdout" "" "$(flea_ui '{"view":"miller"}' 2>/dev/null)"
check "a patch that is not JSON prints its sentence on stderr" "1" "$(flea_ui 'not json at all' 2>&1 >/dev/null | grep -c '^flea: ')"
check "a patch that is not JSON prints nothing on stdout" "" "$(flea_ui 'not json at all' 2>/dev/null)"
check "two arguments prints its sentence on stderr" "1" "$(flea_ui a b 2>&1 >/dev/null | grep -c '^flea: ')"
check "two arguments prints nothing on stdout" "" "$(flea_ui a b 2>/dev/null)"

# columns names what the list row SHOWS and src/uischema.rs says name is never optional, so an empty
# array, a subset without name and a duplicate are all refused. Measured through the real singleton:
# a stored ["name","size","size","date"] left one header-menu "Hide Size" click still drawing size.
for bad_columns in '{"columns":[]}' '{"columns":["size","date"]}' '{"columns":["name","size","size"]}'; do
  fresh
  out=$(flea_ui "$bad_columns" 2>&1); rc=$?
  check "a columns array that is not a set exits 2: $bad_columns" "2" "$rc"
  check "and names the key it refused: $bad_columns" "1" "$(echo "$out" | grep -c 'columns')"
  check "and writes no state file: $bad_columns" "0" "$([ -e "$UI" ] && echo 1 || echo 0)"
  # ls -A, because [ -e "$UI" ] alone cannot see the directory and the lock update() takes first.
  check "and leaves only the lock it took: $bad_columns" "ui.json.lock" "$(ls -A "$STATE/flea" | sort | tr '\n' ' ' | sed 's/ $//')"
done

# A hand edit is not a patch: it costs that one key its own default and the key beside it stands.
fresh
mkdir -p "$STATE/flea"
printf '{"columns":["name","size","size"],"density":"comfortable"}\n' > "$UI"
out=$(flea_ui 2>&1)
check "a duplicated column in the file falls back to the shipped set" "1" "$(echo "$out" | tr -d ' \n' | grep -c '"columns":\["name","size","date"\]')"
check "the key beside the refused columns array stands" "1" "$(echo "$out" | grep -c '"density": "comfortable"')"

# ColumnsWidth caps the count at 2..5 and ListColumns040 remembers a dragged edge per column at 48..480.
fresh
out=$(flea_ui '{"columnsLimit":3,"columnWidths":{"size":120}}' 2>&1); rc=$?
check "a column limit and width patch exits 0" "0" "$rc"
check "the limit landed" "1" "$(grep -c '"columnsLimit": 3' "$UI")"
check "the width landed" "1" "$(tr -d ' \n' < "$UI" | grep -c '"columnWidths":{"size":120}')"
for good_column_key in '{"columnsLimit":2}' '{"columnsLimit":5}' '{"columnWidths":{"size":48}}' '{"columnWidths":{"size":480}}'; do
  out=$(flea_ui "$good_column_key" 2>&1); rc=$?
  check "a column value on its rails exits 0: $good_column_key" "0" "$rc"
done
for bad_column_key in '{"columnsLimit":1}' '{"columnsLimit":6}' '{"columnWidths":{"size":47}}' '{"columnWidths":{"size":481}}' '{"columnWidths":{"name":100}}'; do
  before_bad=$(cat "$UI")
  out=$(flea_ui "$bad_column_key" 2>&1); rc=$?
  check "a column value outside its rails exits 2: $bad_column_key" "2" "$rc"
  case "$bad_column_key" in
    *columnsLimit*) want_key="columnsLimit" ;;
    *) want_key="columnWidths" ;;
  esac
  check "and names the key it refused: $bad_column_key" "1" "$(echo "$out" | grep -c "$want_key")"
  check "and leaves ui.json untouched: $bad_column_key" "$before_bad" "$(cat "$UI")"
done
out=$(flea_ui '{"density":"tight"}' 2>&1); rc=$?
check "a tight density patch exits 0" "0" "$rc"
check "the tight density landed" "1" "$(grep -c '"density": "tight"' "$UI")"
out=$(flea_ui '{"preview":{"thumbSize":"huge"}}' 2>&1); rc=$?
check "a huge thumbnail patch exits 0" "0" "$rc"
out=$(flea_ui '{"preview":{"thumbSize":"largest"}}' 2>&1); rc=$?
check "a largest thumbnail patch exits 0" "0" "$rc"
# Document views are not stored choices: a fresh read holds neither leaf and patches are refused.
fresh
out=$(flea_ui 2>&1)
for leaf in markdownView htmlView; do
  check "a fresh read holds no $leaf" "0" "$(echo "$out" | grep -c "$leaf")"
  for view in source rendered html; do
    reply=$(flea_ui "{\"preview\":{\"$leaf\":\"$view\"}}" 2>&1); rc=$?
    check "a $view $leaf patch exits 2" "2" "$rc"
    check "and names the key it refused: $leaf ($view)" "1" "$(echo "$reply" | grep -c "$leaf")"
    check "and writes no state file: $leaf ($view)" "0" "$([ -e "$UI" ] && echo 1 || echo 0)"
  done
done
# A state file that still carries the leaf loads, and its other keys stand: the key is an unknown one, kept as read.
mkdir -p "$STATE/flea"
printf '{"preview":{"markdownView":"source","htmlView":"source","thumbSize":"large"},"view":"columns"}\n' > "$UI"
out=$(flea_ui 2>&1); rc=$?
check "a state file holding the retired leaf reads exit 0" "0" "$rc"
check "and prints no error" "0" "$(echo "$out" | grep -ci 'error\|refus')"
check "and keeps its other preview key" "1" "$(echo "$out" | grep -c '"thumbSize": "large"')"
check "and keeps its view" "1" "$(echo "$out" | grep -c '"view": "columns"')"
out=$(flea_ui '{"density":"tight"}' 2>&1); rc=$?
check "a write over the retired leaf exits 0" "0" "$rc"
check "and lands" "1" "$(grep -c '"density": "tight"' "$UI")"
# The write fills the preview defaults in, so the unknown key and its sibling leaf are read back by value, not by shape.
# Sample input (post-write fields, other defaults omitted): {"density": "tight", "view": "columns", "preview": {"column": true, "thumbSize": "large", "markdownView": "source", "htmlView": "source"}}
kept=$(python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); p=d.get("preview", {}); print(p.get("markdownView"), p.get("htmlView"), p.get("thumbSize"), d.get("view"))' "$UI" 2>&1)
check "and the write keeps both retired leaves, their sibling leaf and the view as read" "source source large columns" "$kept"

# G1: two folder patches both survive, a null forgets one, and two widths behave per key.
fresh
out=$(flea_ui '{"folderSorts":{"/a":{"key":"size","reverse":true}}}' 2>&1); rc=$?
check "the first folder patch exits 0" "0" "$rc"
out=$(flea_ui '{"folderSorts":{"/b":{"key":"name","reverse":false}}}' 2>&1); rc=$?
check "the second folder patch exits 0" "0" "$rc"
flat=$(tr -d ' \n' < "$UI")
check "two folder patches both survive" "1|1" "$(echo "$flat" | grep -c '"/a":{"key":"size","reverse":true}')|$(echo "$flat" | grep -c '"/b":{"key":"name","reverse":false}')"
out=$(flea_ui '{"folderSorts":{"/a":null}}' 2>&1); rc=$?
check "a null forget exits 0" "0" "$rc"
flat=$(tr -d ' \n' < "$UI")
check "a null forgets only its folder" "0|1" "$(echo "$flat" | grep -c '"/a":')|$(echo "$flat" | grep -c '"/b":{"key":"name","reverse":false}')"
flea_ui '{"columnWidths":{"size":120}}' >/dev/null 2>&1
flea_ui '{"columnWidths":{"date":140}}' >/dev/null 2>&1
flat=$(tr -d ' \n' < "$UI")
check "two width patches both survive" "1|1" "$(echo "$flat" | grep -c '"size":120')|$(echo "$flat" | grep -c '"date":140')"

# The path is predictable, so a link planted at it is refused and what it points at is untouched.
fresh
mkdir -p "$STATE/flea"
printf 'planted\n' > "$SANDBOX/run/planted.json"
ln -s "$SANDBOX/run/planted.json" "$UI"
out=$(flea_ui '{"hidden":true}' 2>&1); rc=$?
check "a symlink at the target exits 2" "2" "$rc"
check "a symlink at the target says so" "1" "$(echo "$out" | grep -c 'symbolic link')"
check "what the link points at is untouched" "planted" "$(cat "$SANDBOX/run/planted.json")"
# The same link under --gui is the settle that FAILS, and the documented behaviour is that main()
# prints one line and opens the window anyway on a file it did not validate.
out=$(env WAYLAND_DISPLAY=flea-uistate-test-display PATH=/nonexistent-flea-test-path \
      XDG_STATE_HOME="$STATE" XDG_CONFIG_HOME="$CONFIG" $BIN --gui </dev/null 2>&1)
check "a failed settle says the view state was not settled" "1" "$(echo "$out" | grep -c 'the view state was not settled')"
check "and names the link as the reason" "1" "$(echo "$out" | grep -c 'symbolic link')"
check "and the launch goes on to the window" "1" "$(echo "$out" | grep -c '^UI_STATE_TEST_WINDOW_STARTED$')"
check "and the link still points at untouched bytes" "planted" "$(cat "$SANDBOX/run/planted.json")"

# The other settle failure, and the one nothing drove at all: a state directory this cannot write in.
fresh
mkdir -p "$CONFIG/flea"
printf '{"hiddenCols":["kind"]}\n' > "$CONFIG/flea/view.json"
chmod 500 "$STATE"
out=$(env WAYLAND_DISPLAY=flea-uistate-test-display PATH=/nonexistent-flea-test-path \
      XDG_STATE_HOME="$STATE" XDG_CONFIG_HOME="$CONFIG" $BIN --gui </dev/null 2>&1)
check "an unwritable state directory is a settle failure too" "1" "$(echo "$out" | grep -c 'the view state was not settled')"
check "and that launch also goes on to the window" "1" "$(echo "$out" | grep -c '^UI_STATE_TEST_WINDOW_STARTED$')"
out=$(env XDG_STATE_HOME="$STATE" XDG_CONFIG_HOME="$CONFIG" $BIN --ui-state '{"hidden":true}' </dev/null 2>&1); rc=$?
check "a patch into an unwritable state directory exits 2" "2" "$rc"
check "and no state file appears under it" "0" "$([ -e "$UI" ] && echo 1 || echo 0)"
check "and prints its sentence on stderr" "1" "$(flea_ui '{"hidden":true}' 2>&1 >/dev/null | grep -c '^flea: ')"
check "and prints nothing on stdout" "" "$(flea_ui '{"hidden":true}' 2>/dev/null)"
# Restored before anything else runs: a 0500 directory is one rm -rf can descend and a later mkdir cannot.
chmod 700 "$STATE"

# 0.1.3's view.json: hiddenCols named what was hidden, columns names what is shown, uiScale is dropped.
fresh
mkdir -p "$CONFIG/flea"
printf '{"hiddenCols":["kind","mode"],"uiScale":1.4}\n' > "$CONFIG/flea/view.json"
out=$(flea_ui 2>&1)
check "the migration carries hiddenCols across" "1" "$(echo "$out" | tr -d ' \n' | grep -c '"columns":\["name","size","date"\]')"
check "the migration drops uiScale" "0" "$(echo "$out" | grep -c 'uiScale')"
flea_ui '{"hidden":true}' >/dev/null 2>&1
check "the migrated columns are what gets written" "1" "$(tr -d ' \n' < "$UI" | grep -c '"columns":\["name","size","date"\]')"
printf '{"hiddenCols":["size","date","kind","mode"]}\n' > "$CONFIG/flea/view.json"
out=$(flea_ui 2>&1)
check "view.json is never read again once ui.json exists" "1" "$(echo "$out" | tr -d ' \n' | grep -c '"columns":\["name","size","date"\]')"

# 0.3.3 turns Show unmounted drives on once for a file written before it, and the stamp makes it once.
fresh
mkdir -p "$STATE/flea"
# The stamp comes from the source constant (src/uischema.rs DEFAULTS), never from the binary under test.
# Sample input, the last DEFAULTS line of src/uischema.rs: `  "stateVersion": 3`
shipped=$(sed -n 's/^  "stateVersion": \([0-9][0-9]*\).*/\1/p' src/uischema.rs)
check "the source names one migration stamp" "1" "$(printf '%s' "$shipped" | grep -c '^[0-9][0-9]*$')"
# Sample input, the --ui-state read: `  "stateVersion": 3`
check "a fresh read ships that stamp" "$shipped" "$(flea_ui 2>&1 | sed -n 's/^  "stateVersion": \([0-9][0-9]*\).*/\1/p')"
printf '{"view":"grid","places":{"showUnmounted":false,"driveSize":true}}\n' > "$UI"
old_sha=$(sha256sum "$UI" | cut -d' ' -f1)
out=$(flea_ui 2>&1)
check "a 0.3.2 file that stored the switch off reads it on" "1" "$(echo "$out" | grep -c '"showUnmounted": true')"
check "and keeps the file's own choices beside it" "1|1" "$(echo "$out" | grep -c '"view": "grid"')|$(echo "$out" | grep -c '"driveSize": true')"
check "and the read carries the migration stamp" "1" "$(echo "$out" | grep -c "\"stateVersion\": $shipped\\b")"
check "and a read alone writes nothing" "$old_sha" "$(sha256sum "$UI" | cut -d' ' -f1)"
flea_ui '{"places":{"showUnmounted":false}}' >/dev/null 2>&1
check "switched off after the migration, the file stores it off" "1" "$(grep -c '"showUnmounted": false' "$UI")"
check "beside the stamp that keeps the migration from running again" "1" "$(grep -c "\"stateVersion\": $shipped\\b" "$UI")"
check "and the write kept the file's own choices too" "1|1" "$(grep -c '"view": "grid"' "$UI")|$(grep -c '"driveSize": true' "$UI")"
check "so the next process still reads it off" "1" "$(flea_ui 2>&1 | grep -c '"showUnmounted": false')"
out=$(flea_ui '{"stateVersion":0}' 2>&1); rc=$?
check "a patch that names the stamp exits 2" "2" "$rc"
check "and names the stamp it refused" "1" "$(echo "$out" | grep -c 'stateVersion')"

# The real launcher settles the migration before executing the private window double's marker.
fresh
mkdir -p "$CONFIG/flea"
printf '{"hiddenCols":["kind"],"uiScale":1.4}\n' > "$CONFIG/flea/view.json"
capture="$SANDBOX/run/at-window.json"
out=$(env UI_STATE_TEST_CAPTURE="$capture" WAYLAND_DISPLAY=flea-uistate-test-display PATH=/nonexistent-flea-test-path \
      XDG_STATE_HOME="$STATE" XDG_CONFIG_HOME="$CONFIG" $BIN --gui </dev/null 2>&1)
check "the launch got past the migration to the window" "1" "$(echo "$out" | grep -c '^UI_STATE_TEST_WINDOW_STARTED$')"
check "the window launch migrated view.json first" "1" "$(tr -d ' \n' < "$UI" | grep -c '"columns":\["name","mode","size","date"\]')"
check "the launch migration dropped uiScale" "0" "$(grep -c uiScale "$UI")"
check "the window received the migrated state before startup" "$(cat "$UI")" "$(cat "$capture" 2>/dev/null)"
before_migrate=$(cat "$UI")
before_migrate_ino=$(stat -c '%i' "$UI")
env WAYLAND_DISPLAY=flea-uistate-test-display PATH=/nonexistent-flea-test-path \
    XDG_STATE_HOME="$STATE" XDG_CONFIG_HOME="$CONFIG" $BIN --gui </dev/null >/dev/null 2>&1
check "a second launch does not migrate again" "$before_migrate" "$(cat "$UI")"
# view.json is unchanged between the two launches, so a second migration would render these same
# bytes and the contents alone cannot go red. The inode is what tells a rewrite from no rewrite.
check "and does not rewrite the file to say so" "$before_migrate_ino" "$(stat -c '%i' "$UI")"

# The window reads ui.json with its own FileView, so the launch settles the file through the schema
# first: a value this Flea refuses must never be what the first paint draws, and the two front ends
# must answer the same question the same way.
fresh
mkdir -p "$STATE/flea"
printf '{"columns":["name","size","owner"],"density":"comfortable","fromANewerFlea":{"a":1}}\n' > "$UI"
capture="$SANDBOX/run/at-window.json"
env UI_STATE_TEST_CAPTURE="$capture" WAYLAND_DISPLAY=flea-uistate-test-display PATH=/nonexistent-flea-test-path \
    XDG_STATE_HOME="$STATE" XDG_CONFIG_HOME="$CONFIG" $BIN --gui </dev/null >/dev/null 2>&1
check "the launch settles a refused value out of the file" "0" "$(grep -c 'owner' "$UI")"
check "the settled file carries the shipped columns instead" "1" "$(tr -d ' \n' < "$UI" | grep -c '"columns":\["name","size","date"\]')"
check "the settle leaves a good key beside it alone" "1" "$(grep -c '"density": "comfortable"' "$UI")"
check "the settle keeps a newer Flea's own key" "1" "$(grep -c 'fromANewerFlea' "$UI")"
check "the window received only settled state before startup" "$(cat "$UI")" "$(cat "$capture" 2>/dev/null)"
settled=$(cat "$UI")
settled_ino=$(stat -c '%i' "$UI")
env WAYLAND_DISPLAY=flea-uistate-test-display PATH=/nonexistent-flea-test-path \
    XDG_STATE_HOME="$STATE" XDG_CONFIG_HOME="$CONFIG" $BIN --gui </dev/null >/dev/null 2>&1
check "a second launch settles to the same bytes" "$settled" "$(cat "$UI")"
# Every launch would otherwise pay the settle's own write, 6.7 to 18.6 ms against 1.3 to 1.7 for a
# launch that only reads.
check "and does not rewrite a file that is already settled" "$settled_ino" "$(stat -c '%i' "$UI")"

# A ui.json the settle cannot read is the only copy of whatever the operator wrote, so the launch
# leaves it exactly as it is: both front ends already read such a file as the full default shape, and
# a settle rewrite would spend their settings to close nothing. A trailing comma is the ordinary way
# in. That is the settle alone, and the block below pins what the next patch does to the same file.
fresh
mkdir -p "$STATE/flea"
printf '{\n  "columns": ["name", "size"],\n  "density": "comfortable",\n}\n' > "$UI"
broken_sha=$(sha256sum "$UI" | cut -d' ' -f1)
broken_ino=$(stat -c '%i' "$UI")
out=$(env WAYLAND_DISPLAY=flea-uistate-test-display PATH=/nonexistent-flea-test-path \
      XDG_STATE_HOME="$STATE" XDG_CONFIG_HOME="$CONFIG" $BIN --gui </dev/null 2>&1)
check "the launch got past the settle to the window" "1" "$(echo "$out" | grep -c '^UI_STATE_TEST_WINDOW_STARTED$')"
check "a ui.json the settle cannot parse is left byte for byte" "$broken_sha" "$(sha256sum "$UI" | cut -d' ' -f1)"
check "and it is not replaced by a new file either" "$broken_ino" "$(stat -c '%i' "$UI")"
check "and the read still answers the full default shape" "1" "$(flea_ui 2>&1 | tr -d ' \n' | grep -c '"columns":\["name","size","date"\]')"
# The first patch onto that file writes a full default document, keeping the operator's bytes as ui.json.broken like 0.3.12.
out=$(flea_ui '{"hidden":true}' 2>&1); rc=$?
check "a patch onto that same file exits 0" "0" "$rc"
check "the operator's bytes are kept as ui.json.broken" "$broken_sha" "$(sha256sum "$STATE/flea/ui.json.broken" | cut -d' ' -f1)"
check "and does not leave the operator's bytes" "1" "$([ "$(sha256sum "$UI" | cut -d' ' -f1)" != "$broken_sha" ] && echo 1 || echo 0)"
check "it writes the full default document instead" "44" "$(grep -c '^  "' "$UI")"
check "so the hand-written key is gone" "1" "$(grep -c '"density": "compact"' "$UI")"
check "and the patch itself landed" "1" "$(grep -c '"hidden": true' "$UI")"

# A hand-edited number Rust's f64 parse takes and JSON does not is the same case: parse_number
# refuses the shape, so the document does not read, and the settle leaves it rather than rewriting
# it into bytes the window's own JSON.parse would then refuse.
fresh
mkdir -p "$STATE/flea"
printf '{"places":{"sidebarWidth":0192},"density":"comfortable"}\n' > "$UI"
rust_only_sha=$(sha256sum "$UI" | cut -d' ' -f1)
env WAYLAND_DISPLAY=flea-uistate-test-display PATH=/nonexistent-flea-test-path \
    XDG_STATE_HOME="$STATE" XDG_CONFIG_HOME="$CONFIG" $BIN --gui </dev/null >/dev/null 2>&1
check "a leading-zero literal is not written back into ui.json" "$rust_only_sha" "$(sha256sum "$UI" | cut -d' ' -f1)"
check "and that file reads as the full default shape" "1" "$(flea_ui 2>&1 | tr -d ' \n' | grep -c '"density":"compact"')"

# The same for a document that is valid JSON but not the object the merge reads.
fresh
mkdir -p "$STATE/flea"
printf '["name","size"]\n' > "$UI"
array_sha=$(sha256sum "$UI" | cut -d' ' -f1)
env WAYLAND_DISPLAY=flea-uistate-test-display PATH=/nonexistent-flea-test-path \
    XDG_STATE_HOME="$STATE" XDG_CONFIG_HOME="$CONFIG" $BIN --gui </dev/null >/dev/null 2>&1
check "a ui.json that is not a JSON object is left byte for byte" "$array_sha" "$(sha256sum "$UI" | cut -d' ' -f1)"

# And for one whose bytes are not text at all, which is also the state file: 0.1.3's view.json is
# neither migrated over it nor read in its place.
fresh
mkdir -p "$STATE/flea" "$CONFIG/flea"
printf '\377\376{"columns":["name"]}\n' > "$UI"
printf '{"hiddenCols":["date"]}\n' > "$CONFIG/flea/view.json"
notext_sha=$(sha256sum "$UI" | cut -d' ' -f1)
env WAYLAND_DISPLAY=flea-uistate-test-display PATH=/nonexistent-flea-test-path \
    XDG_STATE_HOME="$STATE" XDG_CONFIG_HOME="$CONFIG" $BIN --gui </dev/null >/dev/null 2>&1
check "a ui.json that is not text is left byte for byte" "$notext_sha" "$(sha256sum "$UI" | cut -d' ' -f1)"
check "and view.json is not read in its place" "1" "$(flea_ui 2>&1 | tr -d ' \n' | grep -c '"columns":\["name","size","date"\]')"

# The write half of that same file, which the settle alone did not close: read() answers the shipped
# defaults for bytes it cannot read, so a patch that went ahead would rename a full default document
# over the operator's only copy. update() refuses the write instead, and says which file and why.
out=$(flea_ui '{"hidden":true}' 2>&1); rc=$?
check "a patch onto a ui.json that is not text exits 2" "2" "$rc"
check "and names the read as the reason" "1" "$(echo "$out" | grep -c 'could not be read')"
check "and leaves that file byte for byte" "$notext_sha" "$(sha256sum "$UI" | cut -d' ' -f1)"

# The way in that needs no hex editor: a mode this process cannot read, which one `sudo flea` also
# leaves behind as a root-owned ui.json inside a user-owned directory. The rename needs the directory
# and not the file, so this guard is the only thing between one settings write and every key in it.
fresh
mkdir -p "$STATE/flea"
printf '{\n  "columns": ["name", "size"],\n  "density": "comfortable",\n  "fromANewerFlea": {"a": 1}\n}\n' > "$UI"
denied_sha=$(sha256sum "$UI" | cut -d' ' -f1)
denied_ino=$(stat -c '%i' "$UI")
chmod 000 "$UI"
out=$(flea_ui '{"hidden":true}' 2>&1); rc=$?
check "a patch onto an unreadable ui.json exits 2" "2" "$rc"
check "and names the read as the reason" "1" "$(echo "$out" | grep -c 'could not be read')"
check "and prints its sentence on stderr" "1" "$(flea_ui '{"hidden":true}' 2>&1 >/dev/null | grep -c '^flea: ')"
check "and prints nothing on stdout" "" "$(flea_ui '{"hidden":true}' 2>/dev/null)"
# ls -A: update() takes the lock before it looks at the target, so the lock is all a refusal leaves.
check "and the refusal left only the lock it took" "ui.json ui.json.lock" "$(ls -A "$STATE/flea" | sort | tr '\n' ' ' | sed 's/ $//')"
chmod 600 "$UI"
check "the operator's file is byte for byte what it was" "$denied_sha" "$(sha256sum "$UI" | cut -d' ' -f1)"
check "and is the same file, not a new one renamed over it" "$denied_ino" "$(stat -c '%i' "$UI")"

# The control the guard must not have broken: a first run has no file to spend, so it writes one.
fresh
out=$(flea_ui '{"hidden":true}' 2>&1); rc=$?
check "a first run still writes its state file" "0" "$rc"
check "and the patch landed in it" "1" "$(grep -c '"hidden": true' "$UI")"

# A launch with nothing to migrate leaves ~/.local/state alone, the way a first run always has.
fresh
env WAYLAND_DISPLAY=flea-uistate-test-display PATH=/nonexistent-flea-test-path \
    XDG_STATE_HOME="$STATE" XDG_CONFIG_HOME="$CONFIG" $BIN --gui </dev/null >/dev/null 2>&1
check "a first launch with no view.json writes nothing" "0" "$([ -e "$UI" ] && echo 1 || echo 0)"

# Twelve processes read, merge and write at once. Without the lock each one's write is built on a
# read taken before its neighbours' writes, so the keys they set are lost.
fresh
flea_ui '{"hidden":false}' >/dev/null 2>&1
racers='{"foldersFirst":false} {"groupByKind":true} {"hidden":true} {"wrapAtEnds":true}
{"places":{"showHome":false}} {"places":{"showNetwork":false}} {"places":{"showDevices":false}}
{"places":{"showTrash":false}} {"places":{"driveSize":false}} {"preview":{"column":false}}
{"preview":{"ctrlZoom":false}} {"keys":"windows"}'
for patch in $racers; do
  flea_ui "$patch" >/dev/null 2>&1 &
done
wait
landed=0
for line in '"foldersFirst": false' '"groupByKind": true' '"hidden": true' '"wrapAtEnds": true' \
            '"showHome": false' '"showNetwork": false' '"showDevices": false' '"showTrash": false' \
            '"driveSize": false' '"column": false' '"ctrlZoom": false' '"keys": "windows"'; do
  grep -qF "$line" "$UI" && landed=$((landed + 1))
done
check "twelve concurrent writers all land" "12" "$landed"
check "the racing writers left no temp behind" "ui.json ui.json.lock" "$(ls -A "$STATE/flea" | sort | tr '\n' ' ' | sed 's/ $//')"

# A write killed part way through leaves the previous file byte for byte, because the rename is last.
fresh
flea_ui '{"view":"list"}' >/dev/null 2>&1
before=$(cat "$UI")
flea_ui '{"view":"grid"}' >/dev/null 2>&1
after=$(cat "$UI")
printf '%s\n' "$before" > "$SANDBOX/run/before.json"
kills=0
partial=0
round=0
while [ "$round" -lt 120 ]; do
  cp "$SANDBOX/run/before.json" "$UI"
  timeout -s KILL "0.00$((round % 9 + 1))s" \
    env XDG_STATE_HOME="$STATE" XDG_CONFIG_HOME="$CONFIG" $BIN --ui-state '{"view":"grid"}' >/dev/null 2>&1 </dev/null
  [ $? -eq 137 ] && kills=$((kills + 1))
  now=$(cat "$UI")
  if [ "$now" != "$before" ] && [ "$now" != "$after" ]; then
    partial=$((partial + 1))
  fi
  round=$((round + 1))
done
# ls -A, because a suite that audits a directory with ls is blind to the dotfiles in it, and this
# block asserted the file's contents and the kill count and never listed the directory at all. A
# SIGKILL between write_new and rename leaves that pid's own temp for good, and tens of the 120
# rounds do: 47 to 90 across the four runs that added this check, a magnitude and not a number to cite.
# Nothing reaps them, because the block above holds twelve live temps at once, so no process can tell
# a peer's temp from a corpse and deleting one in flight is worse than the litter.
temps=$(ls -A "$STATE/flea" | grep -c '^ui\.json\.[0-9]\+\.tmp$')
strays=$(ls -A "$STATE/flea" | grep -vc '^ui\.json$\|^ui\.json\.lock$\|^ui\.json\.[0-9]\+\.tmp$')
echo "     the sweep left $temps temp file(s) behind, one per round killed inside the write"
check "the sweep left nothing but ui.json, its lock and killed writers' own temps" "0" "$strays"
check "the sweep left no more temps than its $kills killed rounds" "1" "$([ "$temps" -le "$kills" ] && echo 1 || echo 0)"

# The kill count is printed and never asserted: a floor inside the 1 to 9 ms budget measures box speed.
echo "     the sweep killed $kills of 120 rounds, diagnostic only"
# A 0-temp timed sweep proves nothing about the write window, so its temp count stays diagnostic.
echo "     the sweep left $temps write-window temp(s), diagnostic only"
check "none of the $kills killed rounds left a partial state file" "0" "$partial"

# The stage kills prove ui.json stays exactly the before or after document at each named write stage.
DET="$FIXTURE_ROOT/flea-uistate-det-$$"
sandbox_make "$DET" || exit 1
# Sample input: `deterministic receipt 12345 7 /…/flea/ui.json.12345.tmp` plus the sha256 line.
if python3 tests/uistate-deterministic.py "$DET" "$BIN" >"$DET/det.log" 2>&1; then
  check "a kill before the temp kept the seeded bytes" "1" "$(grep -c 'stage before-temp kill kept' "$DET/det.log")"
  check "deterministic interrupted publication kept the seeded bytes" "1" "$(grep -c 'interrupted publication kept' "$DET/det.log")"
  check "a kill past the rename kept the published bytes" "1" "$(grep -c 'stage after-rename kill kept' "$DET/det.log")"
  check "released barrier published the exact expected state" "1" "$(grep -c 'released barrier published' "$DET/det.log")"
  echo "     deterministic hit 3 of 3 stages killed on purpose, timed kills $kills of 120 separate"
else
  echo "FAIL deterministic interrupted publication proof"
  cat "$DET/det.log"
  fail=1
fi
sandbox_remove "$DET" || exit 1

sandbox_remove "$SANDBOX" || exit 1

[ "$fail" -eq 0 ] && echo "uistate: all checks passed"
exit $fail
