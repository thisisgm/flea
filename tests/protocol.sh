#!/bin/bash
# Drives the real binary over stdin and asserts the exact stdout contract.
set -u
# Hard rule 9's guard, which owns FIXTURE_ROOT and every create and delete below.
. "$(dirname "$0")/../tools/flea-sandbox-guard"

cd "$(dirname "$0")/.." || exit 1

BIN=${BIN:-./target/debug/flea}
# A clean git archive export carries no target/, and without this the suite runs every case
# against a missing binary and reports them as product failures.
if [ ! -x "$BIN" ]; then
    printf 'protocol.sh: no binary at %s\n' "$BIN" >&2
    printf 'protocol.sh: build it (cargo build) or set BIN to one; refusing to report on nothing\n' >&2
    exit 1
fi
# The sandbox is the parent and the listing is a directory inside it, because the guard's marker is
# a real dotfile and this suite asserts what a hidden:true listing contains.
SB="$FIXTURE_ROOT/flea-proto-test-$$"
D="$SB/tree"
# src/backend/thumbcache.rs honours XDG_CACHE_HOME, so this suite's thumbnails land inside its own
# sandbox and the operator's real cache is never written to, read from, or cleaned up after.
export XDG_CACHE_HOME="$SB/cache"
fail=0

setup() {
  sandbox_make "$SB"
  mkdir -p "$D/sub"
  printf 'abc' > "$D/three.txt"
  : > "$D/empty.txt"
}

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

setup

out=$(printf '{"c":"list","path":"%s","first":2}\n{"c":"quit"}\n' "$D" | $BIN --backend)
check "listed count" "3" "$(echo "$out" | head -1 | grep -oE '"n":[0-9]+' | cut -d: -f2)"
# One field for the whole listing, because every file in a directory shares its filesystem.
check "listed names the directory's own filesystem" "1" "$(echo "$out" | head -1 | grep -c '"v":[1-9]')"
check "list is followed by rows" "rows" "$(echo "$out" | sed -n 2p | grep -oE '"t":"[a-z]+"' | head -1 | cut -d'"' -f4)"
check "first window honours the count" "2" "$(echo "$out" | sed -n 2p | grep -o '"n":"' | wc -l | tr -d ' ')"

out=$(printf '{"c":"list","path":"%s","first":0}\n{"c":"window","start":0,"count":10}\n{"c":"quit"}\n' "$D" | $BIN --backend)
check "a rows object follows list even when first is 0" "rows" "$(echo "$out" | sed -n 2p | grep -oE '"t":"[a-z]+"' | head -1 | cut -d'"' -f4)"
check "that rows object is empty" "0" "$(echo "$out" | sed -n 2p | grep -o '"n":"' | wc -l | tr -d ' ')"
check "directories sort first" "sub" "$(echo "$out" | sed -n 3p | grep -oE '"n":"[^"]+"' | head -1 | cut -d'"' -f4)"

# Task 11: rows carries a per-response Kind dictionary, read against the box's real freedesktop tables, see docs/protocol.md "rows".
kind_out=$(printf '{"c":"list","path":"%s","first":10}\n{"c":"quit"}\n' "$D" | $BIN --backend)
kind_row=$(echo "$kind_out" | sed -n 2p)
check "rows carries a kinds dictionary" "1" "$(echo "$kind_row" | grep -c '"kinds":\[')"
check "a directory's kind is Folder" "1" "$(echo "$kind_row" | grep -c '"Folder"')"
check "the two text files share one Plain text document entry, not two" "1" "$(echo "$kind_row" | grep -o '"Plain text document"' | wc -l | tr -d ' ')"
check "every one of the three rows names its kind by an index" "3" "$(echo "$kind_row" | grep -o '"k":[0-9]*' | wc -l | tr -d ' ')"
# A drop destination is always a directory, so only a directory row carries its filesystem id. The
# 100k scale fixture holds no directories, which is why this field costs the headline listing nothing.
check "only the directory row carries a filesystem id" "1" "$(echo "$kind_row" | grep -o '"v":[0-9]*' | wc -l | tr -d ' ')"
check "and that id is a real device, not a zero placeholder" "0" "$(echo "$kind_row" | grep -c '"v":0[,}]')"

# Size and mtime are orders now: answered with listed like name, and the pass rides in read.
# Sample output: {"t":"listed","n":3,"read":0.041,"sort":0.003}
out=$(printf '{"c":"list","path":"%s","first":0}\n{"c":"sort","by":"size","desc":false}\n{"c":"quit"}\n' "$D" | $BIN --backend)
check "sorting by size answers a listed line, not an error" "listed" "$(echo "$out" | sed -n 3p | grep -oE '"t":"[a-z]+"' | cut -d'"' -f4)"
check "and sorting by mtime does too" "listed" "$(printf '{"c":"list","path":"%s","first":0}\n{"c":"sort","by":"mtime","desc":true}\n{"c":"quit"}\n' "$D" | $BIN --backend | sed -n 3p | grep -oE '"t":"[a-z]+"' | cut -d'"' -f4)"

out=$(printf '{"c":"list","path":"%s","first":0}\n{"c":"sort","by":"name","desc":true}\n{"c":"window","start":0,"count":10}\n{"c":"quit"}\n' "$D" | $BIN --backend)
check "descending name sort keeps directories first" "sub" "$(echo "$out" | sed -n 4p | grep -oE '"n":"[^"]+"' | head -1 | cut -d'"' -f4)"
check "descending name sort reverses the files" "three.txt" "$(echo "$out" | sed -n 4p | grep -oE '"n":"[^"]+"' | sed -n 2p | cut -d'"' -f4)"

out=$(printf '{"c":"list","path":"%s","first":0}\n{"c":"window","start":999999,"count":10}\n{"c":"quit"}\n' "$D" | $BIN --backend)
check "a window past the end echoes the clamped start" '"start":3' "$(echo "$out" | sed -n 3p | grep -o '"start":[0-9]*')"
check "and returns no rows" "0" "$(echo "$out" | sed -n 3p | grep -o '"n":"' | wc -l | tr -d ' ')"

out=$(printf '{"c":"list","path":"/definitely/not/here","first":1}\n{"c":"quit"}\n' | $BIN --backend)
check "a missing path is an error message" "error" "$(echo "$out" | head -1 | grep -oE '"t":"[a-z]+"' | cut -d'"' -f4)"
# rename-kept carries a hyphen, so a [a-z]+ class matches no part of that line and yields nothing.
check "the error names the operation" "scan" "$(echo "$out" | head -1 | grep -oE '"where":"[a-z-]+"' | cut -d'"' -f4)"

printf '{"c":"list","path":"/definitely/not/here","first":1}\n{"c":"quit"}\n' | $BIN --backend >/dev/null
check "the backend exits 0 even after an error" "0" "$?"

out=$(printf 'total junk\n{"c":"quit"}\n' | $BIN --backend)
check "junk produces no output and no crash" "" "$out"

ND_SB="$FIXTURE_ROOT/flea-newline-test-$$"
ND="$ND_SB/tree"
sandbox_make "$ND_SB"
mkdir -p "$ND"
: > "$ND/$(printf 'two\nlines.txt')"
out=$(printf '{"c":"list","path":"%s","first":5}\n{"c":"quit"}\n' "$ND" | $BIN --backend)
check "a newline in a real filename keeps the response on two lines" "2" "$(echo "$out" | wc -l | tr -d ' ')"
check "and the name is escaped in the row" "1" "$(echo "$out" | sed -n 2p | grep -c 'two\\nlines.txt')"
sandbox_remove "$ND_SB"

SD_SB="$FIXTURE_ROOT/flea-symlink-test-$$"
SD="$SD_SB/tree"
sandbox_make "$SD_SB"
mkdir -p "$SD"
mkdir -p "$SD/realdir"
ln -s "$SD/realdir" "$SD/linkdir"
ln -s "$SD/nowhere" "$SD/brokenlink"
out=$(printf '{"c":"list","path":"%s","first":5}\n{"c":"quit"}\n' "$SD" | $BIN --backend)
check "a broken symlink is listed, not dropped" "1" "$(echo "$out" | sed -n 2p | grep -c '"n":"brokenlink"')"
check "a symlink to a directory reports d false" "1" "$(echo "$out" | sed -n 2p | grep -c '"n":"linkdir","d":false')"
sandbox_remove "$SD_SB"

# Directories first is not optional, so the fixture that proves it has to be one the two orders can
# actually disagree about. Directories "1" and "11" beside a file "2": grouped gives 1 11 2 and
# ungrouped gives 1 2 11, so a build with the grouping taken out fails this and only this shape can
# tell them apart. Descending needs the whole order and not the first name: grouped gives 11 1 2 and
# ungrouped gives 11 2 1, which share a first row.
GR_SB="$FIXTURE_ROOT/flea-grouping-test-$$"
GR="$GR_SB/tree"
sandbox_make "$GR_SB"
mkdir -p "$GR/1" "$GR/11"
: > "$GR/2"

# Sample input: {"t":"rows","start":0,"rows":[{"n":"1","d":true,...},{"n":"11",...}],...} becomes "1 11 2".
row_names() {
  grep -oE '"n":"[^"]+"' | cut -d'"' -f4 | tr '\n' ' ' | sed 's/ $//'
}

grouping_order() {
  local by="$1" desc="$2"
  printf '{"c":"list","path":"%s","first":0}\n{"c":"sort","by":"%s","desc":%s}\n{"c":"window","start":0,"count":10}\n{"c":"quit"}\n' \
    "$GR" "$by" "$desc" | $BIN --backend | sed -n 4p | row_names
}

check "ascending name sort groups the directories ahead of a file that sorts between them" \
  "1 11 2" "$(grouping_order name false)"
check "descending name sort keeps that grouping, and reverses only inside it" \
  "11 1 2" "$(grouping_order name true)"

# The refusal ui/js/Sort.js now reports instead of predicting. The two sentences are different facts
# and the UI captions them differently, so both are pinned here rather than only the error type.
# Sample input: {"t":"error","where":"sort","path":"kind","msg":"no such sort key; send name, size or mtime"}
sort_reply() {
  printf '{"c":"list","path":"%s","first":0}\n%s\n{"c":"quit"}\n' "$GR" "$1" | $BIN --backend | sed -n 3p
}

kind_reply=$(sort_reply '{"c":"sort","by":"kind","desc":false}')
check "a header column that is no sort key is refused, not answered as name order" \
  "error" "$(echo "$kind_reply" | grep -oE '"t":"[a-z]+"' | cut -d'"' -f4)"
check "and the refusal names the key it refused" \
  "kind" "$(echo "$kind_reply" | grep -oE '"path":"[a-z]*"' | cut -d'"' -f4)"
check "and it is the no-such-key sentence, not the metadata-pass one" \
  "no such sort key; send name, size or mtime" "$(echo "$kind_reply" | grep -oE '"msg":"[^"]+"' | cut -d'"' -f4)"

nokey_reply=$(sort_reply '{"c":"sort","desc":false}')
check "a sort with no by at all is refused by the same sentence" \
  "no such sort key; send name, size or mtime" "$(echo "$nokey_reply" | grep -oE '"msg":"[^"]+"' | cut -d'"' -f4)"
check "and its refusal carries back the empty key it was sent" \
  '"path":""' "$(echo "$nokey_reply" | grep -o '"path":""')"

# The whole point of refusing rather than answering: the grouped order the listing already had stays.
check "a refused sort leaves the listing in the order it already had" \
  "1 11 2" "$(grouping_order kind false)"

# Size and mtime go through the metadata pass and must keep the grouping. Each key is made to
# disagree with the others: 2 is the larger file and the oldest entry, 3 the smaller and the
# newest, 11 is older than 1, and 1 holds a file so its st_size is not 0. A build ordering
# directories by st_size would answer "11 1 3 2" for size ascending; one that lost the grouping
# would answer "2 11 1 3" for mtime ascending.
: > "$GR/1/x"
printf '%05d' 0 > "$GR/2"
printf '0' > "$GR/3"
touch -d '2020-01-01 00:00:00' "$GR/2"
touch -d '2020-01-01 00:00:01' "$GR/11"
touch -d '2020-01-01 00:00:02' "$GR/1"
touch -d '2020-01-01 00:00:03' "$GR/3"
check "name ascending still groups the directories with the two files in place" \
  "1 11 2 3" "$(grouping_order name false)"
check "size ascending orders the files by size and the directories by name" \
  "1 11 3 2" "$(grouping_order size false)"
check "size descending keeps the directories first and reverses inside each group" \
  "11 1 2 3" "$(grouping_order size true)"
check "mtime ascending orders both groups by time, directories still first" \
  "11 1 2 3" "$(grouping_order mtime false)"
check "mtime descending keeps the directories first and reverses inside each group" \
  "1 11 3 2" "$(grouping_order mtime true)"
# The order must agree with the column: the window after a size sort carries each row's own s.
check "the reordered window carries the sizes the order was built from" \
  '"n":"3","d":false,"s":1 "n":"2","d":false,"s":5' \
  "$(printf '{"c":"list","path":"%s","first":0}\n{"c":"sort","by":"size","desc":false}\n{"c":"window","start":0,"count":10}\n{"c":"quit"}\n' "$GR" | $BIN --backend | sed -n 4p | grep -oE '"n":"[23]","d":false,"s":[0-9]+' | tr '\n' ' ' | sed 's/ $//')"
sandbox_remove "$GR_SB"

out=$(printf '{"c":"list","path":"%s","first":0}\n{"c":"list","path":"/definitely/not/here","first":0}\n{"c":"window","start":0,"count":10}\n{"c":"quit"}\n' "$D" | $BIN --backend)
check "a failed list leaves the previous listing intact" "sub" "$(echo "$out" | sed -n 4p | grep -oE '"n":"[^"]+"' | head -1 | cut -d'"' -f4)"
check "and that listing still stats against its own directory" '"n":"three.txt","d":false,"s":3' "$(echo "$out" | sed -n 4p | grep -o '"n":"three.txt","d":false,"s":3')"

setup
PW="$FIXTURE_ROOT/flea-prewarm-test-$$.json"
rm -f "$PW"
$BIN --prewarm "$D" 2 "$PW"
check "prewarm file exists" "0" "$([ -f "$PW" ] && echo 0 || echo 1)"
check "prewarm first line is listed" "listed" "$(head -1 "$PW" | grep -oE '"t":"[a-z]+"' | cut -d'"' -f4)"
check "prewarm second line is rows" "rows" "$(sed -n 2p "$PW" | grep -oE '"t":"[a-z]+"' | head -1 | cut -d'"' -f4)"
check "prewarm rows honour the count" "2" "$(sed -n 2p "$PW" | grep -o '"n":"' | wc -l | tr -d ' ')"
check "no temp file is left behind" "0" "$(ls "$PW".*.tmp 2>/dev/null | wc -l | tr -d ' ')"
check "the prewarm file is owner-only" "600" "$(stat -c '%a' "$PW")"

printf 'STALE\n' > "$PW"
$BIN --prewarm /definitely/not/here 2 "$PW" >/dev/null 2>&1
check "a failed prewarm exits non-zero" "1" "$?"

TGT="$FIXTURE_ROOT/flea-prewarm-target-$$.txt"
printf 'TARGET UNTOUCHED' > "$TGT"
rm -f "$PW"
ln -s "$TGT" "$PW"
$BIN --prewarm "$D" 2 "$PW" >/dev/null 2>&1
check "a symlink at the destination is replaced" "1" "$([ -L "$PW" ] && echo 0 || echo 1)"
check "and the symlink target is untouched" "TARGET UNTOUCHED" "$(cat "$TGT")"
rm -f "$PW" "$TGT"

# Directories sort first, so index 1 of the row order is "sub" and index 2 is "empty.txt".
out=$(printf '{"c":"list","path":"%s","first":10}\n{"c":"quit"}\n' "$D" | $BIN --backend)
check "a directory row carries the folder icon" "folder" "$(echo "$out" | sed -n 2p | grep -oE '"i":"[^"]+"' | sed -n 1p | cut -d'"' -f4)"
check "a file row carries an icon name" "text-x-generic" "$(echo "$out" | sed -n 2p | grep -oE '"i":"[^"]+"' | sed -n 2p | cut -d'"' -f4)"

setup
printf 'x' > "$D/photo.jpg"
out=$(printf '{"c":"list","path":"%s","first":10}\n{"c":"quit"}\n' "$D" | $BIN --backend)
check "an image row carries the image icon" "1" "$(echo "$out" | sed -n 2p | grep -c '"i":"image-x-generic"')"
check "the icon field never arrives empty" "0" "$(echo "$out" | sed -n 2p | grep -c '"i":""')"

# A symlink to a directory carries d:false by contract, so only its icon follows the target.
setup
ln -s "$D/sub" "$D/linkdir"
printf 'x' > "$D/cert.pem"
chmod 0644 "$D/cert.pem"
# *.so is application/x-sharedlib, one of the 190 application types generic-icons does not list.
cp /usr/bin/true "$D/prog.so"
chmod 0755 "$D/prog.so"
# Row order is sub, cert.pem, empty.txt, linkdir, prog.so, three.txt.
out=$(printf '{"c":"list","path":"%s","first":10}\n{"c":"quit"}\n' "$D" | $BIN --backend)
check "a symlink to a directory keeps d false" "false" "$(echo "$out" | sed -n 2p | grep -oE '"n":"linkdir","d":(true|false)' | cut -d: -f3)"
check "and draws as a folder" "1" "$(echo "$out" | sed -n 2p | grep -c '"n":"linkdir","d":false,"s":[0-9]*,"m":[0-9-]*,"p":[0-9]*,"i":"folder"')"
# Each icon grep is anchored to its own row's fields: a .* here spans into the next row's icon.
check "a pem is not an executable" "0" "$(echo "$out" | sed -n 2p | grep -c '"n":"cert.pem","d":false,"s":[0-9]*,"m":[0-9-]*,"p":[0-9]*,"i":"application-x-executable"')"
check "a pem draws as a generic file" "1" "$(echo "$out" | sed -n 2p | grep -c '"n":"cert.pem","d":false,"s":[0-9]*,"m":[0-9-]*,"p":[0-9]*,"i":"application-x-generic"')"
check "an executable shared object still draws as an executable" "1" "$(echo "$out" | sed -n 2p | grep -c '"n":"prog.so","d":false,"s":[0-9]*,"m":[0-9-]*,"p":[0-9]*,"i":"application-x-executable"')"

# Row order after setup plus the copy is sub, empty.txt, photo.jpg, three.txt, so the indices are 1, 3 and 4.
setup
cp "$FIXTURE_ROOT/flea-media-btrfs/photo_0.jpg" "$D/photo.jpg" 2>/dev/null || printf 'x' > "$D/photo.jpg"
out=$(printf '{"c":"list","path":"%s","first":10}\n{"c":"quit"}\n' "$D" | $BIN --backend)
check "a directory row cannot be thumbnailed" "false" "$(echo "$out" | sed -n 2p | grep -oE '"t":(true|false)' | sed -n 1p | cut -d: -f2)"
check "a jpeg row can be thumbnailed" "true" "$(echo "$out" | sed -n 2p | grep -oE '"t":(true|false)' | sed -n 3p | cut -d: -f2)"
check "a text row cannot be thumbnailed" "false" "$(echo "$out" | sed -n 2p | grep -oE '"t":(true|false)' | sed -n 4p | cut -d: -f2)"
check "every row carries the field" "4" "$(echo "$out" | sed -n 2p | grep -oE '"t":(true|false)' | wc -l | tr -d ' ')"

# Guards the channel restructure: a reader thread that never closes would hang here instead.
out=$(printf '{"c":"list","path":"%s","first":1}\n' "$D" | timeout 10 $BIN --backend; echo "rc=$?")
check "a closed stdin ends the loop without a quit" "rc=0" "$(echo "$out" | tail -1)"

# Row order after setup plus the copy is sub, empty.txt, photo.jpg, three.txt, so row 2 is the jpeg.
setup
cp "$FIXTURE_ROOT/flea-media-btrfs/photo_0.jpg" "$D/photo.jpg"
out=$(printf '{"c":"list","path":"%s","first":10}\n{"c":"thumb","rows":[2]}\n{"c":"quit"}\n' "$D" | $BIN --backend)
check "a thumb request answers a thumbed line" "thumbed" "$(echo "$out" | grep -oE '"t":"thumbed"' | head -1 | cut -d'"' -f4)"
check "the thumbed line names its row" '"row":2' "$(echo "$out" | grep -o '"row":2' | head -1)"
check "the thumbed line carries a file path" "1" "$(echo "$out" | grep -c '"file":"/')"

out=$(printf '{"c":"list","path":"%s","first":10}\n{"c":"thumb","rows":[3]}\n{"c":"quit"}\n' "$D" | $BIN --backend)
check "a row with no thumbnailer answers an empty file" "1" "$(echo "$out" | grep -c '"file":""')"

printf '{"c":"list","path":"%s","first":10}\n{"c":"thumb","rows":[2]}\n{"c":"thumbcancel","rows":[2]}\n{"c":"quit"}\n' "$D" | timeout 30 $BIN --backend >/dev/null
check "a cancelled row is not waited on at quit" "0" "$?"

out=$(printf '{"c":"list","path":"%s","first":10}\n{"c":"thumb","rows":[99999]}\n{"c":"quit"}\n' "$D" | $BIN --backend)
check "a row past the end of the listing is answered with silence" "0" "$(echo "$out" | grep -c '"t":"thumbed"')"

out=$(printf '{"c":"list","path":"%s","first":10}\n{"c":"thumb","rows":[0,1,3]}\n{"c":"quit"}\n' "$D" | $BIN --backend)
check "every row of a multi-row request is answered" "3" "$(echo "$out" | grep -c '"t":"thumbed"')"
check "a directory row answers an empty file" '"row":0,"file":""' "$(echo "$out" | grep -o '"row":0,"file":""')"

# Dotfiles are dropped from the scan itself, so they never reach the sort at all.
setup
: > "$D/.dotfile"
mkdir -p "$D/.dotdir"
out=$(printf '{"c":"list","path":"%s","first":10}\n{"c":"quit"}\n' "$D" | $BIN --backend)
check "hidden defaults to false, so the count excludes both dotfile entries" "3" "$(echo "$out" | head -1 | grep -oE '"n":[0-9]+' | cut -d: -f2)"
check "and neither dotfile row is emitted" "0" "$(echo "$out" | sed -n 2p | grep -c '"n":"\.')"

out=$(printf '{"c":"list","path":"%s","first":10,"hidden":false}\n{"c":"quit"}\n' "$D" | $BIN --backend)
check "hidden explicitly false is the same as omitted" "3" "$(echo "$out" | head -1 | grep -oE '"n":[0-9]+' | cut -d: -f2)"

out=$(printf '{"c":"list","path":"%s","first":10,"hidden":true}\n{"c":"quit"}\n' "$D" | $BIN --backend)
check "hidden true includes both dotfile entries" "5" "$(echo "$out" | head -1 | grep -oE '"n":[0-9]+' | cut -d: -f2)"
check "the dotfile row is present" "1" "$(echo "$out" | sed -n 2p | grep -c '"n":"\.dotfile"')"
check "the dot-directory row is present and marked a directory" "1" "$(echo "$out" | sed -n 2p | grep -c '"n":"\.dotdir","d":true')"

# Task 16: directory sizes; each argument is one stage, and the walker only gets a turn between stages once stdin drains to empty, see docs/protocol.md "dirsize".
dirsize_run() {
  ( for stage in "$@"; do
      # $(...) strips a stage's trailing newline, so it comes back here or the next stage glues onto this one's last line.
      printf '%s\n' "$stage"
      sleep 0.3
    done
    printf '{"c":"quit"}\n'
  ) | $BIN --backend
}

DZ_SB="$FIXTURE_ROOT/flea-dirsize-test-$$"
DZ="$DZ_SB/tree"
sandbox_make "$DZ_SB"
mkdir -p "$DZ"
mkdir -p "$DZ/sub"
printf 'abc' > "$DZ/sub/a.txt"
printf 'de' > "$DZ/file.txt"
# Directories sort first, so row 0 is sub and row 1 is file.txt.
out=$(dirsize_run "$(printf '{"c":"list","path":"%s","first":10}\n{"c":"dirsize","rows":[0]}\n' "$DZ")")
check "a dirsize request answers one dirsized line" "1" "$(echo "$out" | grep -c '"t":"dirsized"')"
check "it names the row it was asked for" "1" "$(echo "$out" | grep -c '"row":0')"
check "the walk is not marked partial well inside the 2000 ms deadline" "1" "$(echo "$out" | grep -c '"partial":false')"
bytes=$(echo "$out" | grep -oE '"bytes":[0-9]+' | cut -d: -f2)
[ -n "$bytes" ] && [ "$bytes" -gt 3 ] 2>/dev/null
check "sub's size counts its own entry plus a.txt inside it" "0" "$?"

# A file row is never a valid dirsize target, and neither is one past the end.
out=$(dirsize_run "$(printf '{"c":"list","path":"%s","first":10}\n{"c":"dirsize","rows":[1,99999]}\n' "$DZ")")
check "a file row and an out-of-range row both answer nothing" "0" "$(echo "$out" | grep -c '"t":"dirsized"')"

# A row already answered is re-answered at once from the cache; staged separately, or two requests sent together would just dedup against the queue instead.
out=$(dirsize_run \
    "$(printf '{"c":"list","path":"%s","first":10}\n{"c":"dirsize","rows":[0]}\n' "$DZ")" \
    "$(printf '{"c":"dirsize","rows":[0]}\n')")
check "a repeated ask for an already-answered row still answers" "2" "$(echo "$out" | grep -c '"t":"dirsized"')"

# dirsizecancel carries no rows and drops everything queued; no pacing here on purpose, so the row is cancelled before the walker could ever get a turn.
out=$(printf '{"c":"list","path":"%s","first":10}\n{"c":"dirsize","rows":[0]}\n{"c":"dirsizecancel"}\n{"c":"quit"}\n' "$DZ" | $BIN --backend)
check "a row cancelled before it was walked is never answered" "0" "$(echo "$out" | grep -c '"t":"dirsized"')"

# list and sort both reassign what a row index names, the same reason a list or a sort clears the thumbnail map, see docs/protocol.md "dirsized".
SZ_SB="$FIXTURE_ROOT/flea-dirsize-sort-test-$$"
SZ="$SZ_SB/tree"
sandbox_make "$SZ_SB"
mkdir -p "$SZ"
mkdir -p "$SZ/aaa" "$SZ/zzz"
printf 'abc' > "$SZ/aaa/small.txt"
printf '%050d' 0 > "$SZ/zzz/bigger.txt"
out=$(dirsize_run \
    "$(printf '{"c":"list","path":"%s","first":10}\n{"c":"dirsize","rows":[0]}\n' "$SZ")" \
    "$(printf '{"c":"sort","by":"name","desc":true}\n{"c":"dirsize","rows":[0]}\n')")
check "a sort still answers a fresh dirsize for the row at its new position" "2" "$(echo "$out" | grep -c '"t":"dirsized"')"
first_bytes=$(echo "$out" | grep -oE '"bytes":[0-9]+' | head -1 | cut -d: -f2)
second_bytes=$(echo "$out" | grep -oE '"bytes":[0-9]+' | sed -n 2p | cut -d: -f2)
# aaa sorts first ascending (the list default) and zzz first descending; a stale cache would repeat aaa's answer.
[ -n "$first_bytes" ] && [ -n "$second_bytes" ] && [ "$second_bytes" -gt "$first_bytes" ] 2>/dev/null
check "row 0's answer after the sort is zzz's larger size, not aaa's stale cache entry" "0" "$?"
sandbox_remove "$SZ_SB"; sandbox_remove "$DZ_SB"

# A new folder: one mkdir(2), answered like rename and journaled so z removes it; see docs/protocol.md "mkdir".
MK_SB="$FIXTURE_ROOT/flea-mkdir-test-$$"
MK="$MK_SB/tree"
sandbox_make "$MK_SB"
mkdir -p "$MK"
out=$(printf '{"c":"mkdir","path":"%s","name":"Invoices"}\n{"c":"quit"}\n' "$MK" | $BIN --backend)
check "a named mkdir answers made with the full path" "{\"t\":\"made\",\"ok\":true,\"path\":\"$MK/Invoices\"}" "$out"
check "and the folder is on disk" "yes" "$([ -d "$MK/Invoices" ] && echo yes || echo no)"

# The default name, numbered past a taken one whatever kind of entry holds it.
: > "$MK/New Folder 2"
out=$(printf '{"c":"mkdir","path":"%s"}\n{"c":"mkdir","path":"%s"}\n{"c":"quit"}\n' "$MK" "$MK" | $BIN --backend)
check "a mkdir with no name makes New Folder" "1" "$(echo "$out" | sed -n 1p | grep -c "\"path\":\"$MK/New Folder\"")"
check "the next one steps past the taken number to New Folder 3" "1" "$(echo "$out" | sed -n 2p | grep -c "\"path\":\"$MK/New Folder 3\"")"

# Sample output: {"t":"error","where":"mkdir","path":"/x/Invoices","msg":"a folder or file with that name already exists"}
before=$(ls -A "$MK" | wc -l | tr -d ' ')
out=$(printf '{"c":"mkdir","path":"%s","name":"Invoices"}\n{"c":"quit"}\n' "$MK" | $BIN --backend)
check "a name already taken is refused with a sentence" "{\"t\":\"error\",\"where\":\"mkdir\",\"path\":\"$MK/Invoices\",\"msg\":\"a folder or file with that name already exists\"}" "$out"

mkdir_refusal() {
  printf '{"c":"mkdir","path":"%s","name":"%s"}\n{"c":"quit"}\n' "$MK" "$1" | $BIN --backend | grep -oE '"msg":"[^"]+"' | cut -d'"' -f4
}
for bad in 'a/b' '.' '..'; do
  check "a name of $bad is refused before any syscall" "a name cannot be . or .., or contain a separator" "$(mkdir_refusal "$bad")"
done
long=$(head -c 256 /dev/zero | tr '\0' a)
check "a name past NAME_MAX carries the OS sentence" "File name too long (os error 36)" "$(mkdir_refusal "$long")"
check "a relative parent is refused" "a parent must be an absolute path" "$(printf '{"c":"mkdir","path":"relative","name":"x"}\n{"c":"quit"}\n' | $BIN --backend | grep -oE '"msg":"[^"]+"' | cut -d'"' -f4)"
check "a parent that vanished since the listing carries the OS sentence" "No such file or directory (os error 2)" "$(printf '{"c":"mkdir","path":"%s/gone","name":"x"}\n{"c":"quit"}\n' "$MK" | $BIN --backend | grep -oE '"msg":"[^"]+"' | cut -d'"' -f4)"
check "no refusal made anything" "$before" "$(ls -A "$MK" | wc -l | tr -d ' ')"

# A name of only spaces is legal, the same as it is for rename; the field trims, the wire does not.
printf '{"c":"mkdir","path":"%s","name":"   "}\n{"c":"quit"}\n' "$MK" | $BIN --backend >/dev/null
check "a name of only spaces is created as sent" "yes" "$([ -d "$MK/   " ] && echo yes || echo no)"

# The write bit and not root: this suite runs as a plain user, so 0555 is a real denial.
mkdir -p "$MK/locked"; chmod 0555 "$MK/locked"
out=$(printf '{"c":"mkdir","path":"%s/locked","name":"x"}\n{"c":"quit"}\n' "$MK" | $BIN --backend)
chmod 0755 "$MK/locked"
check "a parent the user cannot write answers permission denied honestly" "Permission denied (os error 13)" "$(echo "$out" | grep -oE '"msg":"[^"]+"' | cut -d'"' -f4)"

# Undo, in the one process that holds the journal: an empty new folder goes, a filled one stays.
out=$(printf '{"c":"mkdir","path":"%s","name":"empty"}\n{"c":"undo"}\n{"c":"quit"}\n' "$MK" | $BIN --backend)
check "undo names mkdir as what it reversed" '{"t":"undone","op":"mkdir","ok":true}' "$(echo "$out" | sed -n 2p)"
check "and the empty folder is gone" "no" "$([ -e "$MK/empty" ] && echo yes || echo no)"
# The file lands between the two requests, from outside, the way a user would put it there.
out=$( ( printf '{"c":"mkdir","path":"%s","name":"filled"}\n' "$MK"; sleep 0.3; : > "$MK/filled/theirs.txt"; printf '{"c":"undo"}\n{"c":"quit"}\n' ) | $BIN --backend)
check "undo refuses a new folder the user has filled" "the new folder has been filled since, so undo left it in place" "$(echo "$out" | sed -n 2p | grep -oE '"msg":"[^"]+"' | cut -d'"' -f4)"
check "and what they put inside is still there" "yes" "$([ -f "$MK/filled/theirs.txt" ] && echo yes || echo no)"
sandbox_remove "$MK_SB"

# An op that names neither compress nor extract used to fall through to extract, which would have
# unpacked into a destination the caller never meant. It is refused by name and starts no job.
out=$(printf '{"c":"archive","op":"bogus","paths":[],"path":"%s/three.txt","dest":"%s/out","format":"zip"}\n{"c":"quit"}\n' "$D" "$D" | $BIN --backend)
check "an archive op that names neither is refused by name" "op must be compress or extract" "$(echo "$out" | grep -oE '"msg":"[^"]+"' | cut -d'"' -f4)"
check "and the refusal names the op it was given" "bogus" "$(echo "$out" | grep -oE '"path":"[^"]*"' | head -1 | cut -d'"' -f4)"
check "and no job was started for it" "0" "$(echo "$out" | grep -c '"t":"archivestarted"')"

# No per-key cleanup: the cache is inside the sandbox, so it goes when the sandbox does.
sandbox_remove "$SB"
exit $fail
