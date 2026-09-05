#!/bin/bash
# Drives the real binary against ui.json: main() is the only place both front ends' one update path
# is reachable from, and the lock is only a lock across processes.
set -u
# Hard rule 9's guard, which owns FIXTURE_ROOT and every create and delete below.
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1

BIN=./target/debug/flea
SANDBOX=$FIXTURE_ROOT/uistate
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

sandbox_make "$SANDBOX" || exit 1

# A read answers the whole shape and writes nothing: a first launch leaves no file behind.
fresh
out=$(flea_ui 2>&1); rc=$?
check "a read exits 0" "0" "$rc"
check "a read answers the shipped view" "1" "$(echo "$out" | grep -c '"view": "list"')"
check "a read answers the shipped menu.hidden" "1" "$(echo "$out" | grep -c '"copypath"')"
check "a read answers every top-level key" "17" "$(echo "$out" | grep -c '^  "')"
check "a read leaves no state file behind" "0" "$([ -e "$UI" ] && echo 1 || echo 0)"

# A patch is the write, and it prints what it stored so a caller needs no second read.
out=$(flea_ui '{"view":"grid","places":{"sidebarWidth":240}}' 2>&1); rc=$?
check "a patch exits 0" "0" "$rc"
check "a patch prints the stored view" "1" "$(echo "$out" | grep -c '"view": "grid"')"
check "a patch writes the file" "1" "$([ -f "$UI" ] && echo 1 || echo 0)"
check "the stored file carries the patched width" "1" "$(grep -c '"sidebarWidth": 240' "$UI")"
check "the stored file keeps every other key" "17" "$(grep -c '^  "' "$UI")"
check "the state file is owner only" "600" "$(stat -c '%a' "$UI")"
check "the state directory is owner only" "700" "$(stat -c '%a' "$STATE/flea")"
# ls -A: ui.json and its lock, and no temp file left behind by the rename.
check "no temp file survives the write" "ui.json ui.json.lock" "$(ls -A "$STATE/flea" | sort | tr '\n' ' ' | sed 's/ $//')"

# A second patch merges rather than replacing, which is what "caller-key merge" has to mean.
flea_ui '{"hidden":true}' >/dev/null 2>&1
check "the earlier key survives the later patch" "1" "$(grep -c '"view": "grid"' "$UI")"
check "the later key landed" "1" "$(grep -c '"hidden": true' "$UI")"

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
printf '{"view":"miller","density":"compact","hidden":true}\n' > "$UI"
out=$(flea_ui 2>&1)
check "an unknown value falls back to its default" "1" "$(echo "$out" | grep -c '"view": "list"')"
check "the key beside it is untouched" "1" "$(echo "$out" | grep -c '"density": "compact"')"
check "the second key beside it is untouched" "1" "$(echo "$out" | grep -c '"hidden": true')"

# A caller that sends junk is told which key, and nothing is half applied.
fresh
out=$(flea_ui '{"view":"miller"}' 2>&1); rc=$?
check "a bad patch value exits 2" "2" "$rc"
check "a bad patch value names its key" "1" "$(echo "$out" | grep -c 'view')"
check "a refused patch writes nothing" "0" "$([ -e "$UI" ] && echo 1 || echo 0)"
out=$(flea_ui '{"notAKey":1}' 2>&1); rc=$?
check "an unknown patch key exits 2" "2" "$rc"
check "an unknown patch key names itself" "1" "$(echo "$out" | grep -c 'notAKey')"
out=$(flea_ui 'not json at all' 2>&1); rc=$?
check "a patch that is not JSON exits 2" "2" "$rc"
out=$(env XDG_STATE_HOME="$STATE" XDG_CONFIG_HOME="$CONFIG" $BIN --ui-state a b </dev/null 2>&1); rc=$?
check "two arguments is a usage error" "2" "$rc"

# The path is predictable, so a link planted at it is refused and what it points at is untouched.
fresh
mkdir -p "$STATE/flea"
printf 'planted\n' > "$SANDBOX/run/planted.json"
ln -s "$SANDBOX/run/planted.json" "$UI"
out=$(flea_ui '{"hidden":true}' 2>&1); rc=$?
check "a symlink at the target exits 2" "2" "$rc"
check "a symlink at the target says so" "1" "$(echo "$out" | grep -c 'symbolic link')"
check "what the link points at is untouched" "planted" "$(cat "$SANDBOX/run/planted.json")"

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

# The migration runs before the window, so an upgraded install's first paint reads it. The launch is
# driven to the point where qs is missing from PATH, which is after the migration and before any window.
fresh
mkdir -p "$CONFIG/flea"
printf '{"hiddenCols":["kind"],"uiScale":1.4}\n' > "$CONFIG/flea/view.json"
out=$(env WAYLAND_DISPLAY=flea-uistate-test-display PATH=/nonexistent-flea-test-path \
      XDG_STATE_HOME="$STATE" XDG_CONFIG_HOME="$CONFIG" $BIN --gui </dev/null 2>&1)
check "the launch got past the migration to the missing shell" "1" "$(echo "$out" | grep -c 'could not start the shell')"
check "the window launch migrated view.json first" "1" "$(tr -d ' \n' < "$UI" | grep -c '"columns":\["name","mode","size","date"\]')"
check "the launch migration dropped uiScale" "0" "$(grep -c uiScale "$UI")"
before_migrate=$(cat "$UI")
env WAYLAND_DISPLAY=flea-uistate-test-display PATH=/nonexistent-flea-test-path \
    XDG_STATE_HOME="$STATE" XDG_CONFIG_HOME="$CONFIG" $BIN --gui </dev/null >/dev/null 2>&1
check "a second launch does not migrate again" "$before_migrate" "$(cat "$UI")"

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
{"preview":{"ctrlZoom":false}} {"updates":{"check":false}}'
for patch in $racers; do
  flea_ui "$patch" >/dev/null 2>&1 &
done
wait
landed=0
for line in '"foldersFirst": false' '"groupByKind": true' '"hidden": true' '"wrapAtEnds": true' \
            '"showHome": false' '"showNetwork": false' '"showDevices": false' '"showTrash": false' \
            '"driveSize": false' '"column": false' '"ctrlZoom": false' '"check": false'; do
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
check "the kill sweep actually killed something" "1" "$([ "$kills" -gt 0 ] && echo 1 || echo 0)"
check "no kill ever left a partial state file" "0" "$partial"

sandbox_remove "$SANDBOX" || exit 1

[ "$fail" -eq 0 ] && echo "uistate: all checks passed"
exit $fail
