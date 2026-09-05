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
check "a refused patch writes no state file" "0" "$([ -e "$UI" ] && echo 1 || echo 0)"
# ls -A: update() makes the directory and takes the lock before patched() validates the patch, so a
# refused one leaves both behind and the state file itself is the only thing it never writes.
check "a refused patch leaves the directory and the lock it took" "ui.json.lock" "$(ls -A "$STATE/flea" | sort | tr '\n' ' ' | sed 's/ $//')"
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

# columns names what the list row SHOWS and src/uischema.rs says name is never optional, so an empty
# array, a subset without name and a duplicate are all refused. Measured through the real singleton:
# a stored ["name","size","size","date"] left one header-menu "Hide Size" click still drawing size.
for bad_columns in '{"columns":[]}' '{"columns":["size","date"]}' '{"columns":["name","size","size"]}'; do
  fresh
  out=$(flea_ui "$bad_columns" 2>&1); rc=$?
  check "a columns array that is not a set exits 2: $bad_columns" "2" "$rc"
  check "and names the key it refused: $bad_columns" "1" "$(echo "$out" | grep -c 'columns')"
  check "and writes no state file: $bad_columns" "0" "$([ -e "$UI" ] && echo 1 || echo 0)"
done

# A hand edit is not a patch: it costs that one key its own default and the key beside it stands.
fresh
mkdir -p "$STATE/flea"
printf '{"columns":["name","size","size"],"density":"compact"}\n' > "$UI"
out=$(flea_ui 2>&1)
check "a duplicated column in the file falls back to the shipped set" "1" "$(echo "$out" | tr -d ' \n' | grep -c '"columns":\["name","size","date"\]')"
check "the key beside the refused columns array stands" "1" "$(echo "$out" | grep -c '"density": "compact"')"

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
check "and the launch goes on to the window" "1" "$(echo "$out" | grep -c 'could not start the shell')"
check "and the link still points at untouched bytes" "planted" "$(cat "$SANDBOX/run/planted.json")"

# The other settle failure, and the one nothing drove at all: a state directory this cannot write in.
fresh
mkdir -p "$CONFIG/flea"
printf '{"hiddenCols":["kind"]}\n' > "$CONFIG/flea/view.json"
chmod 500 "$STATE"
out=$(env WAYLAND_DISPLAY=flea-uistate-test-display PATH=/nonexistent-flea-test-path \
      XDG_STATE_HOME="$STATE" XDG_CONFIG_HOME="$CONFIG" $BIN --gui </dev/null 2>&1)
check "an unwritable state directory is a settle failure too" "1" "$(echo "$out" | grep -c 'the view state was not settled')"
check "and that launch also goes on to the window" "1" "$(echo "$out" | grep -c 'could not start the shell')"
out=$(env XDG_STATE_HOME="$STATE" XDG_CONFIG_HOME="$CONFIG" $BIN --ui-state '{"hidden":true}' </dev/null 2>&1); rc=$?
check "a patch into an unwritable state directory exits 2" "2" "$rc"
check "and no state file appears under it" "0" "$([ -e "$UI" ] && echo 1 || echo 0)"
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
printf '{"columns":["name","size","owner"],"density":"compact","fromANewerFlea":{"a":1}}\n' > "$UI"
env WAYLAND_DISPLAY=flea-uistate-test-display PATH=/nonexistent-flea-test-path \
    XDG_STATE_HOME="$STATE" XDG_CONFIG_HOME="$CONFIG" $BIN --gui </dev/null >/dev/null 2>&1
check "the launch settles a refused value out of the file" "0" "$(grep -c 'owner' "$UI")"
check "the settled file carries the shipped columns instead" "1" "$(tr -d ' \n' < "$UI" | grep -c '"columns":\["name","size","date"\]')"
check "the settle leaves a good key beside it alone" "1" "$(grep -c '"density": "compact"' "$UI")"
check "the settle keeps a newer Flea's own key" "1" "$(grep -c 'fromANewerFlea' "$UI")"
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
printf '{\n  "columns": ["name", "size"],\n  "density": "compact",\n}\n' > "$UI"
broken_sha=$(sha256sum "$UI" | cut -d' ' -f1)
broken_ino=$(stat -c '%i' "$UI")
out=$(env WAYLAND_DISPLAY=flea-uistate-test-display PATH=/nonexistent-flea-test-path \
      XDG_STATE_HOME="$STATE" XDG_CONFIG_HOME="$CONFIG" $BIN --gui </dev/null 2>&1)
check "the launch got past the settle to the missing shell" "1" "$(echo "$out" | grep -c 'could not start the shell')"
check "a ui.json the settle cannot parse is left byte for byte" "$broken_sha" "$(sha256sum "$UI" | cut -d' ' -f1)"
check "and it is not replaced by a new file either" "$broken_ino" "$(stat -c '%i' "$UI")"
check "and the read still answers the full default shape" "1" "$(flea_ui 2>&1 | tr -d ' \n' | grep -c '"columns":\["name","size","date"\]')"
# The settle preserves the file and update() does not: read() answers the shipped defaults for it, so
# the first patch the window sends merges onto those and renames a full default document over it.
# Deliberate, because the window has already said the file was not used and a save has to land, but
# nothing pinned it, so the next change to update() would have been invisible here.
out=$(flea_ui '{"hidden":true}' 2>&1); rc=$?
check "a patch onto that same file exits 0" "0" "$rc"
check "and does not leave the operator's bytes" "1" "$([ "$(sha256sum "$UI" | cut -d' ' -f1)" != "$broken_sha" ] && echo 1 || echo 0)"
check "it writes the full default document instead" "17" "$(grep -c '^  "' "$UI")"
check "so the hand-written key is gone" "1" "$(grep -c '"density": "normal"' "$UI")"
check "and the patch itself landed" "1" "$(grep -c '"hidden": true' "$UI")"

# A hand-edited number Rust's f64 parse takes and JSON does not is the same case: parse_number
# refuses the shape, so the document does not read, and the settle leaves it rather than rewriting
# it into bytes the window's own JSON.parse would then refuse.
fresh
mkdir -p "$STATE/flea"
printf '{"places":{"sidebarWidth":0192},"density":"compact"}\n' > "$UI"
rust_only_sha=$(sha256sum "$UI" | cut -d' ' -f1)
env WAYLAND_DISPLAY=flea-uistate-test-display PATH=/nonexistent-flea-test-path \
    XDG_STATE_HOME="$STATE" XDG_CONFIG_HOME="$CONFIG" $BIN --gui </dev/null >/dev/null 2>&1
check "a leading-zero literal is not written back into ui.json" "$rust_only_sha" "$(sha256sum "$UI" | cut -d' ' -f1)"
check "and that file reads as the full default shape" "1" "$(flea_ui 2>&1 | tr -d ' \n' | grep -c '"density":"normal"')"

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
check "and never more temps than there were kills" "1" "$([ "$temps" -le "$kills" ] && echo 1 || echo 0)"

# A floor of one kill is what "120 SIGKILL rounds" was being read off, and a 15 ms budget kills 3 of
# 120 and still clears it. The floor is a fifth of the rounds: a magnitude, not the 88 to 102 this
# box reached when the floor was set, nor the 92 to 120 the four runs after it reached, because a
# faster box finishes more rounds inside the 1 to 9 ms budget and a measured
# number in an assertion is a red gate waiting for the next machine. The count is printed, so
# anything said about this sweep is read off the run and not off the floor.
kill_floor=24
echo "     the sweep killed $kills of 120 rounds, floor $kill_floor"
check "the kill sweep killed a fifth of its rounds at least" "1" "$([ "$kills" -ge "$kill_floor" ] && echo 1 || echo 0)"
# The positive control for the line below, which passes vacuously on a sweep that never reached the
# write: a kill during process startup satisfies kills>0 and enters nothing, while a temp survives
# only when the kill landed between write_new's exclusive create and the rename, so the count
# printed above IS the count of rounds killed inside the write window and 0 of them proves nothing.
check "and a kill landed inside the write window at all" "1" "$([ "$temps" -ge 1 ] && echo 1 || echo 0)"
check "no kill ever left a partial state file" "0" "$partial"

sandbox_remove "$SANDBOX" || exit 1

[ "$fail" -eq 0 ] && echo "uistate: all checks passed"
exit $fail
