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
if ! command -v cc >/dev/null 2>&1; then
    echo 'protocol.sh: cc is required to build the directory-size syscall barrier' >&2
    exit 1
fi
# The sandbox is the parent and the listing is a directory inside it, because the guard's marker is
# a real dotfile and this suite asserts what a hidden:true listing contains.
SB="$FIXTURE_ROOT/flea-proto-test-$$"
D="$SB/tree"
SIZES="$SB/sizes"
# damson.txt's size: far above any folder's walked size, so the folder sorts below it on every filesystem.
LARGEST_BYTES=1000000
# src/backend/thumbcache.rs honours XDG_CACHE_HOME, so this suite's thumbnails land inside its own
# sandbox and the operator's real cache is never written to, read from, or cleaned up after.
export XDG_CACHE_HOME="$SB/cache"
# The shared undo journal stays hermetic: this suite's backends share one scratch runtime dir.
export XDG_RUNTIME_DIR="$SB/runtime"
fail=0

setup() {
  sandbox_make "$SB"
  mkdir -p "$D/sub"
  printf 'abc' > "$D/three.txt"
  : > "$D/empty.txt"
  # Name order and size order disagree here, so an anchored size sort cannot pass on name order.
  mkdir -p "$SIZES"
  # The anchors sit two or more places from both ends of every name order, so no name order puts either at 1.
  printf '123' > "$SIZES/apple.txt"
  printf '12345' > "$SIZES/berry.txt"
  printf '1' > "$SIZES/cherry.txt"
  head -c "$LARGEST_BYTES" /dev/zero > "$SIZES/damson.txt"
  printf '1234567' > "$SIZES/elder.txt"
  printf '123456789' > "$SIZES/fig.txt"
  # A folder orders by its walked size (src/backend/ordering.rs): its 4 bytes plus its own entry, a few KB at most.
  mkdir -p "$SIZES/box"
  printf '1234' > "$SIZES/box/four.txt"
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

# A list naming a row to select answers its first window around that row when it sorts past row first, else from row 0.
listed_start() { printf '{"c":"list","path":"%s","first":%s,"select":"%s"}\n{"c":"quit"}\n' "$D" "$1" "$2" | $BIN --backend | sed -n 2p | grep -oE '"start":[0-9]+' | cut -d: -f2; }
check "a select past the first window starts the window at it" "2" "$(listed_start 1 "$D/three.txt")"
check "and the window holds it near its middle, clamped to the end" "1" "$(listed_start 2 "$D/three.txt")"
out=$(printf '{"c":"list","path":"%s","first":2,"select":"%s/three.txt"}\n{"c":"quit"}\n' "$D" "$D" | $BIN --backend)
check "that window's rows are the listing's own from there" "empty.txt three.txt" "$(echo "$out" | sed -n 2p | grep -oE '"n":"[^"]+"' | cut -d'"' -f4 | tr '\n' ' ' | sed 's/ $//')"
check "a select inside the first window starts it at row 0" "0" "$(listed_start 2 "$D/sub")"
check "a select the listing does not hold starts it at row 0" "0" "$(listed_start 1 "$D/missing.txt")"
check "a select in another directory starts it at row 0" "0" "$(listed_start 1 "$D/sub/three.txt")"

# ui/js/Swap.js onListed drops a listed line whose path is not the one it asked for, so each spelling comes back byte for byte.
ln -s "$D/sub" "$SB/sublink"
for asked in "$D/sub/" "$SB/sublink" "$D/sub/.."; do
  out=$(printf '{"c":"list","path":"%s","first":0}\n{"c":"quit"}\n' "$asked" | $BIN --backend)
  # Sample output, the listed line: {"t":"listed","n":2,"read":0.040,"sort":0.010,"v":42,"path":"/tmp/flea/sub/"}
  check "listed names ${asked#"$SB"/} exactly as it was asked" "\"path\":\"$asked\"" "$(echo "$out" | grep -F '"t":"listed"' | head -1 | grep -oE '"path":"[^"]*"')"
done

# listpaths: the picker's Recent, a listing built from the client's own list; see docs/protocol.md "listpaths".
# The order is the client's, a path that is gone is dropped, and every name is relative to the base "/".
out=$(printf '{"c":"listpaths","paths":["%s/three.txt","%s/gone.txt","%s/sub","%s/empty.txt"],"first":10}\n{"c":"quit"}\n' \
  "$D" "$D" "$D" "$D" | $BIN --backend)
check "listpaths drops the path that is gone" "3" "$(echo "$out" | head -1 | grep -oE '"n":[0-9]+' | cut -d: -f2)"
check "listpaths is followed by rows" "rows" "$(echo "$out" | sed -n 2p | grep -oE '"t":"[a-z]+"' | head -1 | cut -d'"' -f4)"
check "listpaths keeps the client's order and never sorts" "${D#/}/three.txt" \
  "$(echo "$out" | sed -n 2p | grep -oE '"n":"[^"]+"' | head -1 | cut -d'"' -f4)"
check "a directory in the list is still marked one" "1" "$(echo "$out" | sed -n 2p | grep -c "\"n\":\"${D#/}/sub\",\"d\":true")"
check "listpaths reports no sort pass" "0.000" "$(echo "$out" | head -1 | grep -oE '"sort":[0-9.]+' | cut -d: -f2)"
# A relative path and the root itself are refused: this list is read out of a file every application writes.
out=$(printf '{"c":"listpaths","paths":["etc/hostname","/",""],"first":10}\n{"c":"quit"}\n' | $BIN --backend)
check "listpaths refuses a path that is not absolute" "0" "$(echo "$out" | head -1 | grep -oE '"n":[0-9]+' | cut -d: -f2)"

# A local fsinfo answers figures and class in one line; only a slow mount sends a second.
# Sample output: {"t":"fsinfo","fs":"btrfs","free":442000000000,"path":"/x","class":""}
out=$(printf '{"c":"list","path":"%s","first":0}\n{"c":"fsinfo"}\n{"c":"quit"}\n' "$D" | $BIN --backend)
check "a local fsinfo answers exactly one line" "1" "$(echo "$out" | grep -c '"t":"fsinfo"')"
check "and that line names the directory's own filesystem, not unknown" "1" "$(echo "$out" | grep '"t":"fsinfo"' | grep -vc '"fs":""')"
check "for the directory just listed" "1" "$(echo "$out" | grep -c "\"t\":\"fsinfo\",\"fs\":\"[^\"]*\",\"free\":[0-9]*,\"path\":\"$D\"")"

# Shebang probe (src/backend/shebang.rs): the cursor row's two-byte read, answered off the loop, so stdin stays open until its lines land.
printf '#!/bin/sh\necho hi\n' > "$D/run.sh"
SHEBANG_REPLY="$SB/shebang-reply"
{ printf '{"c":"shebang","path":"%s/run.sh","id":3}\n{"c":"shebang","path":"%s/three.txt","id":4}\n{"c":"shebang","path":"%s/gone.txt","id":5}\n' "$D" "$D" "$D"
  for _ in $(seq 1 100); do
    [ "$(grep -c '"t":"shebang"' "$SHEBANG_REPLY" 2>/dev/null)" = "3" ] && break
    sleep 0.05
  done
  printf '{"c":"quit"}\n'; } | $BIN --backend > "$SHEBANG_REPLY"
shebang_out=$(cat "$SHEBANG_REPLY")
check "a script answers its shebang with its id" "1" "$(echo "$shebang_out" | grep -c "\"t\":\"shebang\",\"path\":\"$D/run.sh\",\"hasShebang\":true,\"id\":3")"
check "a plain file answers false with its id" "1" "$(echo "$shebang_out" | grep -c "\"t\":\"shebang\",\"path\":\"$D/three.txt\",\"hasShebang\":false,\"id\":4")"
check "a missing path answers false rather than an error" "1" "$(echo "$shebang_out" | grep -c "\"t\":\"shebang\",\"path\":\"$D/gone.txt\",\"hasShebang\":false,\"id\":5")"
rm -f "$D/run.sh"

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

# A request naming a replaced numbering (docs/protocol.md "listing") resolves nothing: $D/sub/kept.txt, row 0 of listing 2, survives a trash sent with 1.
printf 'x' > "$D/sub/kept.txt"
# The stale refusal's leading fields, which the check below cuts the first refusal to.
stale_paths='{"t":"error","where":"stale","path":"paths"'
# Sample output: {"t":"error","where":"stale","path":"paths","msg":"the folder changed before this could run, so nothing was done"}
out=$(printf '{"c":"list","path":"%s","first":5}\n{"c":"list","path":"%s/sub","first":5}\n{"c":"paths","rows":[0],"listing":1}\n{"c":"trash","rows":[0],"menuId":0,"listing":1}\n{"c":"menuaction","op":"snapshot","id":3,"rows":[0],"cursor":0,"listing":1}\n{"c":"paths","rows":[0],"listing":2}\n{"c":"paths","rows":[0]}\n{"c":"sort","by":"size","desc":false}\n{"c":"paths","rows":[0],"listing":2}\n{"c":"quit"}\n' "$D" "$D" | $BIN --backend)
check "each rows line names its listing's numbering" "1 2" "$(echo "$out" | grep '"t":"rows"' | grep -oE '"listing":[0-9]+' | cut -d: -f2 | tr '\n' ' ' | sed 's/ $//')"
check "paths naming the replaced numbering is refused by name and resolves nothing" "$stale_paths" \
  "$(echo "$out" | grep '"where":"stale"' | sed -n 1p | cut -c1-${#stale_paths})"
check "trash naming it is refused the same way" "1" "$(echo "$out" | grep -c '"where":"stale","path":"trash"')"
check "and the file that request would have trashed is still there" "yes" "$([ -f "$D/sub/kept.txt" ] && echo yes || echo no)"
check "a menu snapshot naming it is refused in the menu's own shape" "1" \
  "$(echo "$out" | grep '"t":"menuaction","id":3' | grep -c '"ok":false')"
check "the numbering in force resolves, and so does a request that names none" "2" \
  "$(echo "$out" | grep -c "\"t\":\"paths\",\"paths\":\[\"$D/sub/kept.txt\"\]")"
check "a sort renumbers, so the numbering the rows were read in is refused after it too" "2" \
  "$(echo "$out" | grep -c '"where":"stale","path":"paths"')"
rm -f "$D/sub/kept.txt"

# Size and mtime are orders now: answered with listed like name, and the pass rides in read.
# Sample output: {"t":"listed","n":3,"read":0.041,"sort":0.003}
out=$(printf '{"c":"list","path":"%s","first":0}\n{"c":"sort","by":"size","desc":false}\n{"c":"quit"}\n' "$D" | $BIN --backend)
check "sorting by size answers a listed line, not an error" "listed" "$(echo "$out" | sed -n 3p | grep -oE '"t":"[a-z]+"' | cut -d'"' -f4)"
check "and sorting by mtime does too" "listed" "$(printf '{"c":"list","path":"%s","first":0}\n{"c":"sort","by":"mtime","desc":true}\n{"c":"quit"}\n' "$D" | $BIN --backend | sed -n 3p | grep -oE '"t":"[a-z]+"' | cut -d'"' -f4)"
check "a sort that names no anchor answers the plain listed line" "0" "$(echo "$out" | sed -n 3p | grep -c anchor)"

# A re-sort that names the cursor's row answers that row's index in the new order, through handle_line itself.
# Sample output: {"t":"listed","n":4,"read":0.041,"sort":0.003,"v":56,"path":"/x","anchor":"/x/cherry.txt","anchorIndex":2}
anchored() {
  printf '{"c":"list","path":"%s","first":0}\n{"c":"sort","by":"%s","desc":%s,"foldersFirst":true,"anchor":"%s"}\n{"c":"quit"}\n' \
    "$SIZES" "$1" "$2" "$3" | $BIN --backend | sed -n 3p
}
# Each anchor is 1 only in its own order: every name order, the other direction and size without folders first answer otherwise.
out=$(anchored size false "$SIZES/cherry.txt")
check "an anchored sort echoes the anchor it was given" "1" "$(echo "$out" | grep -c "\"anchor\":\"$SIZES/cherry.txt\"")"
check "and answers its index in the size order: [box, cherry, apple, berry, elder, fig, damson]" '"anchorIndex":1' "$(echo "$out" | grep -oE '"anchorIndex":-?[0-9]+')"
check "descending, the largest file answers its index: [box, damson, fig, elder, berry, apple, cherry]" '"anchorIndex":1' "$(anchored size true "$SIZES/damson.txt" | grep -oE '"anchorIndex":-?[0-9]+')"
check "a name sort answers the anchor's index too: [box, apple, berry, cherry, damson, elder, fig]" '"anchorIndex":3' "$(anchored name false "$SIZES/cherry.txt" | grep -oE '"anchorIndex":-?[0-9]+')"
check "an anchor the listing never held answers -1" '"anchorIndex":-1' "$(anchored name false "$SIZES/gone.txt" | grep -oE '"anchorIndex":-?[0-9]+')"

# Prefetch record (src/prefetch.rs): this shell plays the launcher's, since a pipeline's last command is its child.
PREFETCH_WAIT_TENTHS=30
LIST="$SB/prefetch-list"
SEEN="$SB/prefetch-seen"
REPLY="$SB/prefetch-reply"
# Runs a backend as this shell's child, naming pid $1 as the launcher's shell, until the list differs from $3 (marking $SEEN) or the wait runs out.
prefetch_run() {
  rm -f "$SEEN"
  { printf '{"c":"list","path":"%s","first":10}\n' "$D"
    for _ in $(seq 1 "$PREFETCH_WAIT_TENTHS"); do
      [ "$(cat "$2" 2>/dev/null)" != "$3" ] && { : > "$SEEN"; break; }
      sleep 0.1
    done
    printf '{"c":"quit"}\n'; } | FLEA_PREFETCH="$2" FLEA_PREFETCH_SHELL="$1" $BIN --backend > "$REPLY"
  PREFETCH_RC=${PIPESTATUS[1]}
}
# Sample reply line: {"t":"listed","n":3,"read":0.041,"sort":0.003,"v":1,"path":"/x"}, the proof the backend served the list.
answered() { echo "$PREFETCH_RC $(grep -c '"t":"listed"' "$REPLY")"; }
# A live process of this user, not the backend's parent: a missing parent check would read its maps and record.
sleep 30 &
stranger=$!
prefetch_run "$stranger" "$LIST" ""
kill "$stranger" 2>/dev/null; wait "$stranger" 2>/dev/null
check "a backend whose parent is not the named shell serves the list and exits cleanly" "0 1" "$(answered)"
check "and records no prefetch list" "absent" "$([ -e "$LIST" ] && echo present || echo absent)"
prefetch_run $$ "$LIST" ""
check "the named shell's own backend records one while it runs, before quit" "0 1 seen" "$(answered) $([ -e "$SEEN" ] && echo seen)"
# Sample line 2: "shell 4242 5561234", the shell's pid and start time.
identity=$(sed -n 2p "$LIST")
# Sample stat: "4242 (bash) S 1 ... 0 5561234 ...": after the name's ") ", starttime (field 22) is the 20th field.
stat_line=$(cat /proc/$$/stat)
check "and names this shell by its pid and start time" "shell $$ $(echo "${stat_line##*) }" | cut -d' ' -f20)" "$identity"
shell_exe=$(readlink -f /proc/$$/exe)
# Sample range line: "0 32768 /usr/bin/bash", offset, length and path, so " <path>" matches only the path field.
check "and lists the shell's own executable, not the backend's" "yes no" \
  "$(grep -qF " $shell_exe" "$LIST" && echo yes || echo no) $(grep -qF " $(readlink -f $BIN)" "$LIST" && echo yes || echo no)"
printf 'flea-prefetch 2\nshell %s 1\n0 4096 /sentinel\n' "$$" > "$LIST"
prefetch_run $$ "$LIST" "$(cat "$LIST")"
check "a list naming this pid with another start time is an earlier shell's, and is replaced" "$identity no" \
  "$(sed -n 2p "$LIST") $(grep -qF /sentinel "$LIST" && echo yes || echo no)"
printf 'flea-prefetch 2\n%s\n0 4096 /sentinel\n' "$identity" > "$LIST"
recorded=$(cat "$LIST")
prefetch_run $$ "$LIST" "$recorded"
check "a later backend of the same shell serves its list and leaves the launch's list alone" "0 1 same" \
  "$(answered) $([ "$(cat "$LIST")" = "$recorded" ] && echo same)"

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
# The target the row list draws beside the name, verbatim: absolute stays absolute, relative stays
# relative, and a broken link still names where it points. See docs/protocol.md "rows".
ln -s ../elsewhere "$SD/relativelink"
printf 'abc' > "$SD/plain.txt"
out=$(printf '{"c":"list","path":"%s","first":9}\n{"c":"quit"}\n' "$SD" | $BIN --backend)
check "a symlink carries its target" "1" "$(echo "$out" | sed -n 2p | grep -c "\"n\":\"linkdir\",\"d\":false,[^}]*\"l\":\"$SD/realdir\"")"
check "a broken symlink still names where it points" "1" "$(echo "$out" | sed -n 2p | grep -c "\"n\":\"brokenlink\",[^}]*\"l\":\"$SD/nowhere\"")"
check "a relative target stays relative" "1" "$(echo "$out" | sed -n 2p | grep -c '"n":"relativelink",[^}]*"l":"../elsewhere"')"
check "a plain file carries no target at all" "0" "$(echo "$out" | sed -n 2p | grep -c '"n":"plain.txt",[^}]*"l":')"
check "and exactly the three links carry one" "3" "$(echo "$out" | sed -n 2p | grep -o '"l":"' | wc -l | tr -d ' ')"
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

# Kind order must differ from name order; filename MIME lookup needs no image decoding.
KIND="$GR_SB/kind"
mkdir -p "$KIND"
printf 'text' > "$KIND/a.txt"
printf 'image fixture' > "$KIND/z.png"
out=$(printf '{"c":"list","path":"%s","first":0}\n{"c":"sort","by":"kind","desc":false}\n{"c":"window","start":0,"count":10}\n{"c":"quit"}\n' "$KIND" | "$BIN" --backend)
check "Kind is a supported sort and answers a listing" \
  "listed" "$(echo "$out" | sed -n 3p | grep -oE '"t":"[a-z]+"' | cut -d'"' -f4)"
check "Kind orders image/png before text/plain rather than sorting their names" \
  "z.png a.txt" "$(echo "$out" | sed -n 4p | row_names)"

# Sample input: {"t":"error","where":"sort","path":"mode","msg":"no such sort key; send name, size, mtime or kind"}
sort_reply() {
  printf '{"c":"list","path":"%s","first":0}\n%s\n{"c":"quit"}\n' "$GR" "$1" | $BIN --backend | sed -n 3p
}

unknown_reply=$(sort_reply '{"c":"sort","by":"mode","desc":false}')
check "a header column that is no sort key is refused, not answered as name order" \
  "error" "$(echo "$unknown_reply" | grep -oE '"t":"[a-z]+"' | cut -d'"' -f4)"
check "and the refusal names the key it refused" \
  "mode" "$(echo "$unknown_reply" | grep -oE '"path":"[a-z]*"' | cut -d'"' -f4)"
check "and the refusal names every supported sort key" \
  "no such sort key; send name, size, mtime or kind" "$(echo "$unknown_reply" | grep -oE '"msg":"[^"]+"' | cut -d'"' -f4)"

nokey_reply=$(sort_reply '{"c":"sort","desc":false}')
check "a sort with no by at all is refused by the same sentence" \
  "no such sort key; send name, size, mtime or kind" "$(echo "$nokey_reply" | grep -oE '"msg":"[^"]+"' | cut -d'"' -f4)"
check "and its refusal carries back the empty key it was sent" \
  '"path":""' "$(echo "$nokey_reply" | grep -o '"path":""')"

# Sample input: {"c":"peek","path":"/fixture/tree","first":10,"by":"kind","desc":false}
peek_refused=$(printf '{"c":"peek","path":"%s","first":10,"by":"kind","desc":false}\n{"c":"quit"}\n' "$GR" | "$BIN" --backend)
check "a column peek under a key the peek has no order for is refused by the sort's own sentence" \
  "no such sort key; send name, size or mtime" "$(echo "$peek_refused" | grep '"where":"sort"' | grep -oE '"msg":"[^"]+"' | cut -d'"' -f4)"
check "and that refusal names the key it refused" \
  "kind" "$(echo "$peek_refused" | grep '"where":"sort"' | grep -oE '"path":"[a-z]*"' | cut -d'"' -f4)"
check "and no name-order column answers in its place" \
  "0" "$(echo "$peek_refused" | grep -c '"t":"peeked"')"

# A silent fallback to name ascending would undo this descending order.
for request in '{"c":"sort","by":"mode"}' '{"c":"sort"}'; do
  out=$(printf '{"c":"list","path":"%s","first":0}\n{"c":"sort","by":"name","desc":true}\n%s\n{"c":"window","start":0,"count":10}\n{"c":"quit"}\n' "$GR" "$request" | "$BIN" --backend)
  check "a refused sort keeps the previous descending order: $request" \
    "11 1 2" "$(echo "$out" | sed -n 5p | row_names)"
done

# Size and mtime go through the metadata pass and must keep the grouping. Each key is made to
# disagree with the others: 2 is the larger file and the oldest entry, 3 the smaller and the
# newest, 11 is older than 1, and 1 holds 20 bytes so its walked size is above 11's whatever
# the two directory entries measure. A build ordering folders by name would answer "1 11 3 2"
# for size ascending; one that lost the grouping would answer "2 11 1 3" for mtime ascending.
printf '%020d' 0 > "$GR/1/x"
printf '%05d' 0 > "$GR/2"
printf '0' > "$GR/3"
touch -d '2020-01-01 00:00:00' "$GR/2"
touch -d '2020-01-01 00:00:01' "$GR/11"
touch -d '2020-01-01 00:00:02' "$GR/1"
touch -d '2020-01-01 00:00:03' "$GR/3"
check "name ascending still groups the directories with the two files in place" \
  "1 11 2 3" "$(grouping_order name false)"
check "size ascending orders the files by size and the folders by walked size" \
  "11 1 3 2" "$(grouping_order size false)"
check "size descending keeps the folders first and reverses inside each group" \
  "1 11 2 3" "$(grouping_order size true)"
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
first_listing=$(printf '{"c":"list","path":"%s","first":2}\n{"c":"quit"}\n' "$D" | $BIN --backend | sed -n 2p | grep -oE '"listing":[0-9]+}$')
check "prewarm rows name the numbering a backend's first list answers in" "${first_listing:-no numbering from the backend}" \
  "$(sed -n 2p "$PW" | grep -oE '"listing":[0-9]+}$')"
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

# An extensionless ELF has no glob, so its execute bit alone makes it a program; the same bytes without the bit stay a document.
setup
cp /usr/bin/true "$D/PinyonShiftLauncher"
chmod 0755 "$D/PinyonShiftLauncher"
cp /usr/bin/true "$D/PinyonNoExec"
chmod 0644 "$D/PinyonNoExec"
out=$(printf '{"c":"list","path":"%s","first":10}\n{"c":"quit"}\n' "$D" | $BIN --backend)
check "an extensionless pie with its execute bit draws an executable" "1" "$(echo "$out" | sed -n 2p | grep -c '"n":"PinyonShiftLauncher","d":false,"s":[0-9]*,"m":[0-9-]*,"p":[0-9]*,"i":"application-x-executable"')"
check "the same bytes without the bit draw a document" "1" "$(echo "$out" | sed -n 2p | grep -c '"n":"PinyonNoExec","d":false,"s":[0-9]*,"m":[0-9-]*,"p":[0-9]*,"i":"text-x-generic"')"

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

# ExtThumbs, callout 3: off is cache-only. The decoder is jailed with only its input and output bound, so the witness is this suite's cache.
setup
cp "$FIXTURE_ROOT/flea-media-btrfs/photo_0.jpg" "$D/photo-a.jpg"
cp "$FIXTURE_ROOT/flea-media-btrfs/photo_0.jpg" "$D/photo-b.jpg"
out=$(printf '{"c":"list","path":"%s","first":10}\n{"c":"thumb","rows":[2]}\n{"c":"quit"}\n' "$D" | $BIN --backend)
# Sample input: {"t":"thumbed","row":2,"file":"/home/flea-sandbox/t/cache/thumbnails/large/0a1b.png","ms":98.1}
decoded=$(echo "$out" | grep -oE '"row":2,"file":"[^"]+"' | cut -d'"' -f6)
check "the full path starts the decoder" "1" "$([ -n "$decoded" ] && [ -s "$decoded" ] && echo 1 || echo 0)"
# A decode that started leaves a thumbnail or a fail marker here before quit returns, so an unchanged cache means none started.
cached=$(find "$XDG_CACHE_HOME" -type f | sort)
out=$(printf '{"c":"list","path":"%s","first":10}\n{"c":"thumb","rows":[3],"cacheOnly":true}\n{"c":"quit"}\n' "$D" | $BIN --backend)
check "a cache-only miss answers none" "1" "$(echo "$out" | grep -c '"row":3,"file":""')"
check "and it never starts the decoder" "$cached" "$(find "$XDG_CACHE_HOME" -type f | sort)"

# The class rides beside the fsinfo figures, once per directory change and never per row.
out=$(printf '{"c":"list","path":"%s","first":1}\n{"c":"fsinfo"}\n{"c":"quit"}\n' "$D" | $BIN --backend)
check "a local directory answers an empty class" "1" "$(echo "$out" | grep -c '"t":"fsinfo".*"class":""')"
check "and it answers the tag probe beside it" "1" "$(echo "$out" | grep -c '"t":"fsinfo".*"tagable":true')"

# Tag names ride the rows reply for exactly the rows a window stats, never swept.
setup
python3 -c 'import os,sys; os.setxattr(os.path.join(sys.argv[1], "three.txt"), b"user.xdg.tags", b"Red,Blue")' "$D"
out=$(printf '{"c":"list","path":"%s","first":10}\n{"c":"quit"}\n' "$D" | $BIN --backend)
check "a tagged row carries its names in stored order" "1" "$(echo "$out" | grep -c '"n":"three.txt"[^}]*"tg":\["Red","Blue"\]')"
check "and an untagged row carries no tg field at all" "0" "$(echo "$out" | grep -oE '"n":"empty.txt"[^}]*\}' | grep -c '"tg"')"

# A settags write lands in the next rows reply, and undo takes it back out again.
setup
out=$(printf '{"c":"list","path":"%s","first":10}\n{"c":"settags","rows":[0],"op":"add","tag":"Red","listing":1}\n{"c":"list","path":"%s","first":10}\n{"c":"settags","rows":[0],"op":"add","tag":"Red","listing":2}\n{"c":"undo"}\n{"c":"list","path":"%s","first":10}\n{"c":"settags","rows":[0],"op":"add","tag":"Red","listing":99}\n{"c":"quit"}\n' "$D" "$D" "$D" "$D" | $BIN --backend)
check "adding a tag answers how many files changed" "1" "$(echo "$out" | grep -c '"t":"settagged","ok":1')"
check "and the next rows reply carries it" "1" "$(echo "$out" | grep -c '"tg":\["Red"\]')"
check "re-adding answers no change and records nothing" "1" "$(echo "$out" | grep -c '"t":"settagged","ok":0')"
check "undo restores the previous attribute" "1" "$(echo "$out" | grep -c '"t":"undone","op":"settags"')"
check "so the rows after it carry no tg field" "0" "$(echo "$out" | sed -n '/undone/,$p' | grep -c '"tg"')"
check "a settags from a replaced numbering is refused before anything resolves" "1" "$(echo "$out" | grep -c '"where":"stale","path":"settags"')"

# A toggle over rows is one decision for all of them, and tagstates reads what those rows carry, answered off the loop.
setup
python3 -c 'import os,sys; os.setxattr(os.path.join(sys.argv[1], "three.txt"), b"user.xdg.tags", b"Red,Blue")' "$D"
TAG_REPLY="$SB/tagstates-reply"
{ printf '{"c":"list","path":"%s","first":10}\n{"c":"tagstates","rows":[0,1,2],"id":1,"listing":1}\n' "$D"
  for _ in $(seq 1 100); do grep -q '"t":"tagstates"' "$TAG_REPLY" 2>/dev/null && break; sleep 0.05; done
  printf '{"c":"settags","rows":[0,1,2],"op":"toggle","tag":"Red","listing":1}\n{"c":"tagstates","rows":[0,1,2],"id":2,"listing":1}\n'
  for _ in $(seq 1 100); do [ "$(grep -c '"t":"tagstates"' "$TAG_REPLY" 2>/dev/null)" = "2" ] && break; sleep 0.05; done
  printf '{"c":"settags","rows":[0,1,2],"op":"toggle","tag":"Red","listing":1}\n{"c":"quit"}\n'; } | $BIN --backend > "$TAG_REPLY"
tag_out=$(cat "$TAG_REPLY")
check "tagstates names a tag only some rows carry as mixed" "1" "$(echo "$tag_out" | grep -c '"t":"tagstates","id":1,"applied":\[\],"mixed":\["Red","Blue"\],"listing":1')"
check "a toggle where some rows carry the tag adds it to the rest and names add" "1" "$(echo "$tag_out" | grep -c '"t":"settagged","ok":2,"op":"add"')"
check "then every row carries it" "1" "$(echo "$tag_out" | grep -c '"t":"tagstates","id":2,"applied":\["Red"\],"mixed":\["Blue"\],"listing":1')"
check "a toggle where every row carries the tag removes it from all and names remove" "1" "$(echo "$tag_out" | grep -c '"t":"settagged","ok":3,"op":"remove"')"

# A directory on a system mountpoint adds "system":true (its device number in mountinfo), sent after class and tagable.
out=$(printf '{"c":"list","path":"/","first":0}\n{"c":"fsinfo"}\n{"c":"quit"}\n' | $BIN --backend)
check "the root answers the system flag" "1" "$(echo "$out" | grep -c '"t":"fsinfo".*,"system":true}')"
check "and the flag rides after the class" "1" "$(echo "$out" | grep -c '"class":"[a-z]*","tagable":[a-z]*,"system"')"

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

# Directory sizes: each argument is one stage; pacing allows asynchronous replies between stages.
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

# A queued cancellation suppresses the result even if the worker has already picked up the row.
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
# One clause per fault, as src/backend/opsname.rs names it: a separator and a dot name are different faults.
for bad in 'a/b' '.' '..'; do
  case $bad in
    a/b) want="a name cannot contain /" ;;
    *) want="a name cannot be a single dot or a double dot" ;;
  esac
  check "a name of $bad is refused before any syscall" "$want" "$(mkdir_refusal "$bad")"
done
long=$(head -c 256 /dev/zero | tr '\0' a)
check "a name past NAME_MAX names the cause in words" "that name is too long" "$(mkdir_refusal "$long")"
check "a relative parent is refused" "a parent must be an absolute path" "$(printf '{"c":"mkdir","path":"relative","name":"x"}\n{"c":"quit"}\n' | $BIN --backend | grep -oE '"msg":"[^"]+"' | cut -d'"' -f4)"
check "a parent that vanished since the listing names the cause in words" "item not found" "$(printf '{"c":"mkdir","path":"%s/gone","name":"x"}\n{"c":"quit"}\n' "$MK" | $BIN --backend | grep -oE '"msg":"[^"]+"' | cut -d'"' -f4)"
check "no refusal made anything" "$before" "$(ls -A "$MK" | wc -l | tr -d ' ')"

# A name of only spaces is legal, the same as it is for rename; the field trims, the wire does not.
printf '{"c":"mkdir","path":"%s","name":"   "}\n{"c":"quit"}\n' "$MK" | $BIN --backend >/dev/null
check "a name of only spaces is created as sent" "yes" "$([ -d "$MK/   " ] && echo yes || echo no)"

# The write bit and not root: this suite runs as a plain user, so 0555 is a real denial.
mkdir -p "$MK/locked"; chmod 0555 "$MK/locked"
out=$(printf '{"c":"mkdir","path":"%s/locked","name":"x"}\n{"c":"quit"}\n' "$MK" | $BIN --backend)
chmod 0755 "$MK/locked"
check "a parent the user cannot write answers permission denied honestly" "permission denied" "$(echo "$out" | grep -oE '"msg":"[^"]+"' | cut -d'"' -f4)"

# Undo, in the one process that holds the journal: an empty new folder goes, a filled one stays.
out=$(printf '{"c":"mkdir","path":"%s","name":"empty"}\n{"c":"undo"}\n{"c":"quit"}\n' "$MK" | $BIN --backend)
check "undo names mkdir as what it reversed" '{"t":"undone","op":"mkdir","ok":true}' "$(echo "$out" | sed -n 2p)"
check "and the empty folder is gone" "no" "$([ -e "$MK/empty" ] && echo yes || echo no)"
# The file lands between the two requests, from outside, the way a user would put it there.
out=$( ( printf '{"c":"mkdir","path":"%s","name":"filled"}\n' "$MK"; sleep 0.3; : > "$MK/filled/theirs.txt"; printf '{"c":"undo"}\n{"c":"quit"}\n' ) | $BIN --backend)
check "undo refuses a new folder the user has filled" "the new folder has been filled since, so undo left it in place" "$(echo "$out" | sed -n 2p | grep -oE '"msg":"[^"]+"' | cut -d'"' -f4)"
check "and what they put inside is still there" "yes" "$([ -f "$MK/filled/theirs.txt" ] && echo yes || echo no)"
sandbox_remove "$MK_SB"

# Bulk rename: the whole batch is validated first, swaps ride temp names, one undo reverses.
BR_SB="$FIXTURE_ROOT/flea-bulk-test-$$"
BR="$BR_SB/tree"
sandbox_make "$BR_SB"
mkdir -p "$BR"
printf 'aaa' > "$BR/a.txt"
printf 'bbb' > "$BR/b.txt"
out=$(printf '{"c":"bulkrename","paths":["%s/a.txt","%s/b.txt"],"names":["b.txt","a.txt"]}\n{"c":"quit"}\n' "$BR" "$BR" | $BIN --backend)
check "a swap answers one line for the whole batch" '{"t":"bulkrenamed","ok":2,"unchanged":0}' "$(echo "$out" | grep '"t":"bulkrenamed"')"
check "and the swap moved through temp names" "bbbaaa" "$(cat "$BR/a.txt" 2>/dev/null; cat "$BR/b.txt" 2>/dev/null)"
out=$(printf '{"c":"bulkrename","paths":["%s/a.txt","%s/b.txt"],"names":["c.txt","c.txt"]}\n{"c":"quit"}\n' "$BR" "$BR" | $BIN --backend)
check "colliding names refuse the whole batch" "bulkrename" "$(echo "$out" | grep '"t":"error"' | grep -oE '"where":"[a-z]+"' | cut -d'"' -f4)"
check "and the refusal moved nothing" "bbbaaa" "$(cat "$BR/a.txt" 2>/dev/null; cat "$BR/b.txt" 2>/dev/null)"
out=$(printf '{"c":"bulkrename","paths":["%s/a.txt"],"names":["a.txt"]}\n{"c":"quit"}\n' "$BR" | $BIN --backend)
check "an unchanged name is counted and records nothing" '{"t":"bulkrenamed","ok":0,"unchanged":1}' "$(echo "$out" | grep '"t":"bulkrenamed"')"
out=$(printf '{"c":"bulkrename","paths":["%s/a.txt","%s/b.txt"],"names":["b.txt","a.txt"]}\n{"c":"undo"}\n{"c":"quit"}\n' "$BR" "$BR" | $BIN --backend)
check "one undo reverses the whole batch" '{"t":"undone","op":"bulkrename","ok":true}' "$(echo "$out" | grep '"t":"undone"')"
check "and both names are back" "bbbaaa" "$(cat "$BR/a.txt" 2>/dev/null; cat "$BR/b.txt" 2>/dev/null)"
# A chain lands whichever of its items is listed first: a to b with b moving on to c, then back.
out=$(printf '{"c":"bulkrename","paths":["%s/a.txt","%s/b.txt"],"names":["b.txt","c.txt"]}\n{"c":"quit"}\n' "$BR" "$BR" | $BIN --backend)
check "a chain listed mover first lands whole" '{"t":"bulkrenamed","ok":2,"unchanged":0}' "$(echo "$out" | grep -E '"t":"(bulkrenamed|error)"')"
check "and each name moved one along" "bbbaaa|b.txt c.txt" "$(cat "$BR/b.txt" 2>/dev/null; cat "$BR/c.txt" 2>/dev/null)|$(ls -A "$BR" | tr '\n' ' ' | sed 's/ $//')"
out=$(printf '{"c":"bulkrename","paths":["%s/b.txt","%s/c.txt"],"names":["a.txt","b.txt"]}\n{"c":"quit"}\n' "$BR" "$BR" | $BIN --backend)
check "a chain listed occupant first lands whole" '{"t":"bulkrenamed","ok":2,"unchanged":0}' "$(echo "$out" | grep -E '"t":"(bulkrenamed|error)"')"
check "and each name moved one back" "bbbaaa|a.txt b.txt" "$(cat "$BR/a.txt" 2>/dev/null; cat "$BR/b.txt" 2>/dev/null)|$(ls -A "$BR" | tr '\n' ' ' | sed 's/ $//')"
out=$(printf '{"c":"bulkrename","paths":["%s/a.txt","%s/b.txt"],"names":["b.txt","c.txt"]}\n{"c":"undo"}\n{"c":"quit"}\n' "$BR" "$BR" | $BIN --backend)
check "one undo reverses a chain" '{"t":"undone","op":"bulkrename","ok":true}' "$(echo "$out" | grep '"t":"undone"')"
check "and the chain's names are back" "bbbaaa|a.txt b.txt" "$(cat "$BR/a.txt" 2>/dev/null; cat "$BR/b.txt" 2>/dev/null)|$(ls -A "$BR" | tr '\n' ' ' | sed 's/ $//')"
sandbox_remove "$BR_SB"

# Names a transfer would land on, asked first, then one choice for them; see docs/protocol.md "collisions".
CO_SB="$FIXTURE_ROOT/flea-collide-test-$$"
CO="$CO_SB/tree"
collide_fixture() {
  sandbox_remove "$CO_SB"
  sandbox_make "$CO_SB"
  mkdir -p "$CO/from/album" "$CO/to/album" "$CO_SB/data"
  printf 'yours' > "$CO/from/photo.png"
  printf 'there' > "$CO/to/photo.png"
  printf 'notes' > "$CO/from/notes.txt"
}
collide_ask() {
  printf '{"c":"collisions","id":%s,"paths":["%s/from/photo.png","%s/from/notes.txt","%s/from/album"],"dest":"%s/to"}\n' "$1" "$CO" "$CO" "$CO" "$CO"
}
collide_transfer() {
  printf '{"c":"transfer","op":"%s","paths":["%s/from/photo.png","%s/from/notes.txt"],"dest":"%s/to"%s}\n' "${2:-copy}" "$CO" "$CO" "$CO" "$1"
}
# One backend session run in steps, so quit is sent only once the reply it waits for is out and a loaded box cannot cancel a transfer.
# Sample steps: '{"c":"collisions",...}' 'wait:"t":"collisions"' 'do:printf x > "$CO/to/notes.txt"' 'wait:"t":"transferdone"'
backend_steps() {
  local out="$CO_SB/steps.out" in="$CO_SB/steps.in" step waited pid
  rm -f "$out" "$in"
  mkfifo "$in"
  : > "$out"
  ${step_wrap[@]+"${step_wrap[@]}"} $BIN --backend < "$in" > "$out" &
  pid=$!
  exec 8> "$in"
  for step in "$@"; do
    case "$step" in
      wait:*)
        waited=0
        until grep -qF -- "${step#wait:}" "$out"; do
          waited=$((waited + 1))
          [ "$waited" -le "$STEP_WAIT_TENTHS" ] || { echo "backend_steps: no ${step#wait:} within the wait" >&2; break; }
          sleep 0.1
        done ;;
      do:*) eval "${step#do:}" ;;
      *) printf '%s\n' "$step" >&8 ;;
    esac
  done
  printf '{"c":"quit"}\n' >&8
  exec 8>&-
  wait "$pid"
  cat "$out"
  rm -f "$out" "$in"
}
# Ten seconds, far past a transfer of three small files on a loaded box, and short enough to fail a stuck one.
STEP_WAIT_TENTHS=100
# What backend_steps runs the backend under, empty but for the Replace cases below.
step_wrap=()
collide_fixture
# Sample output: {"t":"collisions","id":7,"total":2,"names":[{"n":"photo.png","d":false,"i":"image-x-generic"},{"n":"album","d":true,"i":"folder"}],"mergeable":true}
out=$(backend_steps "$(collide_ask 7)" 'wait:"t":"collisions"')
check "collisions counts only the names the destination holds" '"total":2' "$(echo "$out" | grep -oE '"total":[0-9]+')"
check "and names them in request order" '"n":"photo.png","n":"album"' "$(echo "$out" | grep -oE '"n":"[^"]+"' | paste -sd, -)"
check "a folder carries the directory bit and its mark" "1" "$(echo "$out" | grep -c '{"n":"album","d":true,"i":"folder"}')"
out=$(backend_steps "$(printf '{"c":"collisions","id":8,"paths":["%s/from/photo.png"],"dest":"relative"}' "$CO")" 'wait:"t":"collisions"')
check "an unusable destination asks nothing and leaves the error to the transfer" '{"t":"collisions","id":8,"total":0,"names":[],"mergeable":false}' "$out"
# Rows resolve against the listing the way a transfer's do: album, notes.txt, photo.png, folders first.
out=$(backend_steps "$(printf '{"c":"list","path":"%s/from","first":10}' "$CO")" 'wait:"t":"rows"' "$(printf '{"c":"collisions","id":9,"rows":[2],"dest":"%s/to"}' "$CO")" 'wait:"t":"collisions"')
check "a rows question names the row's own file" '"id":9,"total":1,"names":[{"n":"photo.png"' "$(echo "$out" | grep -oE '"id":9,"total":[0-9]+,"names":\[\{"n":"[^"]+"')"

out=$(backend_steps "$(collide_ask 7)" 'wait:"t":"collisions"' "$(collide_transfer ',"collide":"keep","collideId":7')" 'wait:"t":"transferdone"')
check "keep both copies every item" '"ok":2,"failed":0,"skipped":0' "$(echo "$out" | grep -oE '"ok":[0-9]+,"failed":[0-9]+,"skipped":[0-9]+')"
check "and names the incoming one as Duplicate does" "yours" "$(cat "$CO/to/photo copy.png" 2>/dev/null)"
check "and leaves the one already there" "there" "$(cat "$CO/to/photo.png")"

collide_fixture
out=$(backend_steps "$(collide_ask 7)" 'wait:"t":"collisions"' "$(collide_transfer ',"collide":"skip","collideId":7')" 'wait:"t":"transferdone"')
check "skip counts the collision in skipped and copies the rest" '"ok":1,"failed":0,"skipped":1' "$(echo "$out" | grep -oE '"ok":[0-9]+,"failed":[0-9]+,"skipped":[0-9]+')"
check "and the name it skipped is untouched" "there" "$(cat "$CO/to/photo.png")"
check "and the free name was copied" "notes" "$(cat "$CO/to/notes.txt" 2>/dev/null)"

# A name that appears after the question is refused whatever the choice, exactly as before.
collide_fixture
out=$(backend_steps "$(collide_ask 7)" 'wait:"t":"collisions"' 'do:printf "arrived later" > "$CO/to/notes.txt"' "$(collide_transfer ',"collide":"keep","collideId":7')" 'wait:"t":"transferdone"')
check "a name that appeared after the question is refused" '"name":"notes.txt","ok":false,"err":"already exists"' "$(echo "$out" | grep -oE '"name":"notes.txt","ok":false,"err":"[^"]+"')"
check "and never replaced" "arrived later" "$(cat "$CO/to/notes.txt")"
check "while the name the question saw was kept both" "yours" "$(cat "$CO/to/photo copy.png" 2>/dev/null)"
# Copy to asks about its menu's selection, and its dialog's close expires that selection before the answer.
collide_fixture
# A snapshot publishes from the menu worker and answers nothing, so its step alone waits a fixed second; the app takes it when the menu opens.
out=$(backend_steps "$(printf '{"c":"list","path":"%s/from","first":10}' "$CO")" 'wait:"t":"rows"' '{"c":"menuaction","op":"snapshot","id":4,"rows":[2]}' 'do:sleep 1' \
  "$(printf '{"c":"collisions","id":7,"menuId":4,"dest":"%s/to"}' "$CO")" 'wait:"t":"collisions"' '{"c":"menuaction","op":"close","id":4}' \
  "$(printf '{"c":"transfer","op":"copy","menuId":4,"dest":"%s/to"}' "$CO")" 'wait:Selected items changed' \
  "$(printf '{"c":"transfer","op":"copy","menuId":4,"dest":"%s/to","collide":"keep","collideId":7}' "$CO")" 'wait:"t":"transferdone"' \
  "$(printf '{"c":"collisions","id":8,"menuId":4,"dest":"%s/to"}' "$CO")" 'wait:"id":8')
check "a menu question names the menu's own selection" '"id":7,"total":1,"names":[{"n":"photo.png"' "$(echo "$out" | grep -oE '"id":7,"total":[0-9]+,"names":\[\{"n":"[^"]+"')"
check "the close expired the live selection in this same process" "1" "$(echo "$out" | grep -c '"where":"transfer","path":"","msg":"Selected items changed; reopen the menu."')"
check "and its transfer runs on what it captured after the menu closed" '"ok":1,"failed":0,"skipped":0' "$(echo "$out" | grep -oE '"ok":[0-9]+,"failed":[0-9]+,"skipped":[0-9]+')"
check "keeping both beside the name that was there" "yours" "$(cat "$CO/to/photo copy.png" 2>/dev/null)"
check "a menu selection that is not there asks nothing" '{"t":"collisions","id":8,"total":0,"names":[],"mergeable":false}' "$(echo "$out" | grep '"id":8')"
# From here a case can send replace, so the backend runs with HOME and XDG_DATA_HOME in this sandbox and a private bus that starts gvfsd with them.
collide_env=(env HOME="$CO_SB" XDG_DATA_HOME="$CO_SB/data")
! command -v dbus-run-session >/dev/null || collide_env+=(dbus-run-session --)
step_wrap=("${collide_env[@]}")
# A choice naming no question covers nothing, and a transfer with no choice at all is today's.
collide_fixture
out=$(backend_steps "$(collide_transfer ',"collide":"replace","collideId":99')" 'wait:"t":"transferdone"')
check "a choice for a question never asked is refused" '"err":"already exists"' "$(echo "$out" | grep -oE '"err":"[^"]+"')"
check "and leaves the name already there untouched" "there" "$(cat "$CO/to/photo.png")"
collide_fixture
out=$(backend_steps "$(collide_ask 7)" 'wait:"t":"collisions"' "$(collide_transfer ',"collide":"replace","collideId":99')" 'wait:"t":"transferdone"')
check "a choice naming another id than the question kept is refused" '"name":"photo.png","ok":false,"err":"already exists"' "$(echo "$out" | grep -oE '"name":"photo.png","ok":false,"err":"[^"]+"')"
check "and the kept question's name is untouched" "there" "$(cat "$CO/to/photo.png")"
collide_fixture
out=$(backend_steps "$(collide_transfer '')" 'wait:"t":"transferdone"')
check "and so is a transfer with no choice at all" '"err":"already exists"' "$(echo "$out" | grep -oE '"err":"[^"]+"')"
check "neither touched the name already there" "there" "$(cat "$CO/to/photo.png")"

# Same folder: a copy keeps both under Duplicate's name, a move onto itself is no error and no work.
collide_fixture
out=$(backend_steps "$(printf '{"c":"transfer","op":"copy","paths":["%s/to/photo.png"],"dest":"%s/to","collide":"refuse"}' "$CO" "$CO")" 'wait:"t":"transferdone"')
check "a copy into its own folder keeps both" "there" "$(cat "$CO/to/photo copy.png" 2>/dev/null)"
out=$(backend_steps "$(printf '{"c":"transfer","op":"move","paths":["%s/to/photo.png"],"dest":"%s/to","collide":"refuse"}' "$CO" "$CO")" 'wait:"t":"transferdone"')
check "a move onto itself is skipped, not failed" '"ok":0,"failed":0,"skipped":1' "$(echo "$out" | grep -oE '"ok":[0-9]+,"failed":[0-9]+,"skipped":[0-9]+')"
out=$(backend_steps "$(printf '{"c":"transfer","op":"copy","paths":["%s/to/photo.png"],"dest":"%s/to"}' "$CO" "$CO")" 'wait:"t":"transferdone"')
check "an older client's same-folder copy is refused as before" '"err":"already in that folder"' "$(echo "$out" | grep -oE '"err":"[^"]+"')"

# Drops callout 3: the FAT answer rides the collisions question and the transfer, and changes nothing where every name is held; see docs/protocol.md "collisions".
collide_fixture
printf 'odd' > "$CO/from/odd: name?.txt"
out=$(backend_steps "$(printf '{"c":"collisions","id":7,"paths":["%s/from/odd: name?.txt","%s/from/photo.png"],"dest":"%s/to","fat":"rename"}' "$CO" "$CO" "$CO")" 'wait:"t":"collisions"')
check "a question carrying a FAT answer for a local destination answers collisions as usual" '"id":7,"total":1' "$(echo "$out" | grep -oE '"id":7,"total":[0-9]+')"
out=$(backend_steps "$(printf '{"c":"collisions","id":8,"paths":["%s/from/odd: name?.txt"],"dest":"%s/to"}' "$CO" "$CO")" 'wait:"t":"collisions"')
check "and one with no answer never asks the FAT card of a destination that holds the names" "0" "$(echo "$out" | grep -c '"t":"fatnames"')"
out=$(backend_steps "$(printf '{"c":"transfer","op":"copy","paths":["%s/from/odd: name?.txt"],"dest":"%s/to","collide":"refuse","collideId":8,"fat":"rename"}' "$CO" "$CO")" 'wait:"t":"transferdone"')
check "a name the destination holds lands as it is, whatever the answer says" "odd" "$(cat "$CO/to/odd: name?.txt" 2>/dev/null)"
check "and is not renamed beside it" "0" "$(ls "$CO/to" | grep -c '^odd_ name_')"

# Drops: a clipboard image as a named file, answered made and journaled so z undoes it; see docs/protocol.md "pasteimage".
mkdir -p "$SB/pastebin"
printf '#!/bin/sh\nprintf "\\211\\120\\116\\107"\n' > "$SB/pastebin/wl-paste"
chmod +x "$SB/pastebin/wl-paste"
out=$(PATH="$SB/pastebin:$PATH" backend_steps "$(printf '{"c":"pasteimage","dest":"%s/to","name":"Pasted image 2026-09-23 at 10.14.32.png"}' "$CO")" 'wait:"t":"made"')
check "a paste answers made for its name" '"path":"'"$CO"'/to/Pasted image 2026-09-23 at 10.14.32.png"' "$(echo "$out" | grep -oE '"path":"[^"]*Pasted image[^"]*"')"
check "and those are the clipboard's own bytes" "4" "$(wc -c < "$CO/to/Pasted image 2026-09-23 at 10.14.32.png" | tr -d ' ')"
check "and leaves no temp behind" "0" "$(ls -A "$CO/to" | grep -c 'flea-paste' || true)"
out=$(PATH="$SB/pastebin:$PATH" backend_steps "$(printf '{"c":"pasteimage","dest":"%s/to","name":"second.png"}' "$CO")" 'wait:"t":"made"' '{"c":"undo"}' 'wait:"t":"undone"')
check "and undo takes the pasted file back" "no" "$([ -e "$CO/to/second.png" ] && echo yes || echo no)"
out=$(printf '{"c":"pasteimage","dest":"%s/to","name":"a/b.png"}\n{"c":"quit"}\n' "$CO" | $BIN --backend)
check "a name no file can take is refused before anything runs" '"where":"pasteimage"' "$(echo "$out" | grep -oE '"where":"[a-z]+"')"

# A move whose Replace would trash the folder holding its own source is refused before any trash is tried.
collide_fixture
mkdir -p "$CO/to/album/album"
printf 'inner' > "$CO/to/album/album/in.txt"
out=$(backend_steps "$(printf '{"c":"collisions","id":7,"paths":["%s/to/album/album"],"dest":"%s/to"}' "$CO" "$CO")" 'wait:"t":"collisions"' \
  "$(printf '{"c":"transfer","op":"move","paths":["%s/to/album/album"],"dest":"%s/to","collide":"replace","collideId":7}' "$CO" "$CO")" 'wait:"t":"transferdone"')
check "a move replacing the folder it sits in is refused" "the item already there holds the one being moved in, so it was not replaced" "$(echo "$out" | grep -oE '"err":"[^"]+"' | cut -d'"' -f4)"
check "and both folders stay where they were" "inner" "$(cat "$CO/to/album/album/in.txt" 2>/dev/null)"

# Merge folds folders together: one walk before anything is written, then the second card's answer.
collide_fixture
mkdir -p "$CO/from/trip/sub" "$CO/to/trip"
printf 'yours' > "$CO/from/trip/a.txt"
printf 'yours-b' > "$CO/from/trip/sub/b.txt"
printf 'there' > "$CO/to/trip/a.txt"
out=$(backend_steps "$(printf '{"c":"collisions","id":7,"paths":["%s/from/trip"],"dest":"%s/to"}' "$CO" "$CO")" 'wait:"t":"collisions"' \
  "$(printf '{"c":"mergecollisions","id":8,"paths":["%s/from/trip"],"dest":"%s/to"}' "$CO" "$CO")" 'wait:"t":"mergecollisions"' \
  "$(printf '{"c":"transfer","op":"copy","paths":["%s/from/trip"],"dest":"%s/to","collide":"merge","collideId":8,"inner":"keep"}' "$CO" "$CO")" 'wait:"t":"transferdone"')
check "a merge is offered only for a folder on a folder" '"mergeable":true' "$(echo "$out" | grep -oE '"mergeable":[a-z]+' | head -1)"
check "the walk lists the files that exist in both" '"total":1' "$(echo "$out" | grep '"id":8' | grep -oE '"total":[0-9]+')"
check "everything missing is copied in" "yours-b" "$(cat "$CO/to/trip/sub/b.txt" 2>/dev/null)"
check "and the name already there is kept both" "yours" "$(cat "$CO/to/trip/a copy.txt" 2>/dev/null)"
check "while the file that was there is untouched" "there" "$(cat "$CO/to/trip/a.txt")"
check "and the terminal line counts what landed inside" '"merged":3,"keptBoth":1' "$(echo "$out" | grep -oE '"merged":[0-9]+,"keptBoth":[0-9]+')"
check "and it names the one folder that merged" '"into":"trip"' "$(echo "$out" | grep -oE '"into":"[^"]+"')"

# A drop inside one window names rows, not paths: the same fold answers them.
collide_fixture
mkdir -p "$CO/drop/trip" "$CO/to/trip"
printf 'yours' > "$CO/drop/trip/a.txt"
printf 'there' > "$CO/to/trip/a.txt"
out=$(backend_steps "$(printf '{"c":"list","path":"%s/drop","first":10}' "$CO")" 'wait:"t":"rows"' \
  "$(printf '{"c":"collisions","id":7,"rows":[0],"dest":"%s/to"}' "$CO")" 'wait:"t":"collisions"' \
  "$(printf '{"c":"mergecollisions","id":8,"rows":[0],"dest":"%s/to"}' "$CO")" 'wait:"t":"mergecollisions"' \
  "$(printf '{"c":"transfer","op":"move","rows":[0],"dest":"%s/to","collide":"merge","collideId":8,"inner":"keep"}' "$CO")" 'wait:"t":"transferdone"')
check "a rows merge is offered only for a folder on a folder" '"mergeable":true' "$(echo "$out" | grep -oE '"mergeable":[a-z]+' | head -1)"
check "the rows walk lists the files that exist in both" '"total":1' "$(echo "$out" | grep '"id":8' | grep -oE '"total":[0-9]+')"
check "and the moved folder lands merged" "yours" "$(cat "$CO/to/trip/a copy.txt" 2>/dev/null)"
check "while the file that was there is untouched" "there" "$(cat "$CO/to/trip/a.txt")"
check "and the emptied source folder is gone" "gone" "$([ ! -e "$CO/drop/trip" ] && echo gone)"

# gio alone decides which branch runs, never the output under test: a scratch file it trashes here, then lists.
collide_trash_state() {
  collide_fixture
  printf 'probe' > "$CO_SB/probe"
  "${collide_env[@]}" sh -c 'gio trash -- "$1" >/dev/null 2>&1; [ ! -e "$1" ] || { echo refuses; exit; }
    if gio trash --list 2>/dev/null | grep -qF "$1"; then echo listed; else echo unlisted; fi' _ "$CO_SB/probe"
}
trash_state=$(collide_trash_state)
echo "note gio trash in this sandbox: $trash_state (a box with gvfs lists, a build container only trashes)"
for op in copy move; do
  collide_fixture
  undone='wait:"t":"undone"'
  [ "$trash_state" = unlisted ] && undone='wait:"where":"undo"'
  out=$(backend_steps "$(collide_ask 7)" 'wait:"t":"collisions"' "$(collide_transfer ',"collide":"replace","collideId":7' "$op")" 'wait:"t":"transferdone"' \
    'do:cat "$CO/to/photo.png" > "$CO_SB/landed"; cat "$CO_SB/data/Trash/files/photo.png" > "$CO_SB/trashed" 2>/dev/null; if [ -e "$CO/from/photo.png" ]; then echo stayed; else echo left; fi > "$CO_SB/source"' \
    '{"c":"undo"}' "$undone")
  counts=$(echo "$out" | grep -oE '"ok":[0-9]+,"failed":[0-9]+,"skipped":[0-9]+')
  if [ "$trash_state" = refuses ]; then
    check "$op: a trash that refuses replaces nothing" '"ok":1,"failed":1,"skipped":0' "$counts"
    check "$op: and says so" "the item already there could not be moved to Trash, so nothing was replaced" "$(echo "$out" | grep -oE '"err":"[^"]+"' | cut -d'"' -f4)"
    check "$op: and the item that was there is there now" "there" "$(cat "$CO/to/photo.png")"
    continue
  fi
  check "$op: replace lands every item" '"ok":2,"failed":0,"skipped":0' "$counts"
  check "$op: the incoming photo took the name" "yours" "$(cat "$CO_SB/landed" 2>/dev/null)"
  check "$op: and the one it replaced went to this sandbox's trash" "there" "$(cat "$CO_SB/trashed" 2>/dev/null)"
  if [ "$op" = move ]; then
    check "move: the source left its folder" "left" "$(cat "$CO_SB/source" 2>/dev/null)"
    check "move: and undo puts it back where it was" "yours" "$(cat "$CO/from/photo.png" 2>/dev/null)"
  else
    check "copy: the source stays in its folder" "stayed" "$(cat "$CO_SB/source" 2>/dev/null)"
  fi
  if [ "$trash_state" = listed ]; then
    check "$op: and one undo reverses the whole transfer" "{\"t\":\"undone\",\"op\":\"$op\",\"ok\":true}" "$(echo "$out" | grep '"t":"undone"')"
    check "$op: restoring the item that was there to its name" "there" "$(cat "$CO/to/photo.png" 2>/dev/null)"
    check "$op: and out of the trash" "no" "$([ -e "$CO_SB/data/Trash/files/photo.png" ] && echo yes || echo no)"
  else
    check "$op: a trash gio cannot list leaves undo nothing to restore by, and it says so" "this item was trashed without a Trash entry, so it cannot be restored" "$(echo "$out" | grep '"where":"undo"' | grep -oE '"msg":"[^"]+"' | cut -d'"' -f4)"
    check "$op: so the item that was there waits in the sandbox trash" "there" "$(cat "$CO_SB/data/Trash/files/photo.png" 2>/dev/null)"
    check "$op: while undo still took the incoming photo off the name" "no" "$([ -e "$CO/to/photo.png" ] && echo yes || echo no)"
  fi
done
step_wrap=()
sandbox_remove "$CO_SB"

# An op that names neither compress nor extract used to fall through to extract, which would have
# unpacked into a destination the caller never meant. It is refused by name and starts no job.
out=$(printf '{"c":"archive","op":"bogus","paths":[],"path":"%s/three.txt","dest":"%s/out","format":"zip"}\n{"c":"quit"}\n' "$D" "$D" | $BIN --backend)
check "an archive op that names neither is refused by name" "op must be compress or extract" "$(echo "$out" | grep -oE '"msg":"[^"]+"' | cut -d'"' -f4)"
check "and the refusal names the op it was given" "bogus" "$(echo "$out" | grep -oE '"path":"[^"]*"' | head -1 | cut -d'"' -f4)"
check "and no job was started for it" "0" "$(echo "$out" | grep -c '"t":"archivestarted"')"

# Issue 68: the listed directory is watched, so a change made from outside answers a changed line.
# The only unsolicited line on the wire, so every case here is driven by a real create, rename or
# delete landing between two requests rather than by a request asking for it.
WT_SB="$FIXTURE_ROOT/flea-watch-test-$$"
WT="$WT_SB/tree"
OTHER="$WT_SB/other"
sandbox_make "$WT_SB"
mkdir -p "$WT" "$OTHER"
printf 'a' > "$WT/alpha.txt"

# The change runs argv-direct between the list and the quit, which is where an outside write lands.
watch_run() {
  local dir="$1"
  shift
  ( printf '{"c":"list","path":"%s","first":10}\n' "$dir"
    sleep 0.4
    "$@"
    sleep 0.6
    printf '{"c":"quit"}\n'
  ) | $BIN --backend
}

# One create can wake the reader once or twice (the create, then the close and the timestamp), so
# the assertion is that the wire said something and did not say it per event, never an exact count.
check_changed() {
  local label="$1" out="$2" low="$3" high="$4"
  local n
  n=$(echo "$out" | grep -c '"t":"changed"')
  [ "$n" -ge "$low" ] && [ "$n" -le "$high" ]
  check "$label (saw $n)" "0" "$?"
}

burst_of_creates() {
  local i
  for i in $(seq 1 100); do
    : > "$WT/burst-$i.txt"
  done
}

out=$(watch_run "$WT" touch "$WT/created.txt")
check_changed "a create from outside answers a changed line" "$out" 1 3
check "and that line names the directory being listed" "$WT" "$(echo "$out" | grep '"t":"changed"' | head -1 | grep -oE '"path":"[^"]*"' | cut -d'"' -f4)"
check "and the listing itself is not re-sent, because the client asks for that" "1" "$(echo "$out" | grep -c '"t":"listed"')"

out=$(watch_run "$WT" mv "$WT/created.txt" "$WT/renamed.txt")
check_changed "a rename from outside answers a changed line" "$out" 1 3

out=$(watch_run "$WT" rm -f "$WT/renamed.txt")
check_changed "a delete from outside answers a changed line" "$out" 1 3

out=$(watch_run "$WT" mkdir "$WT/made")
check_changed "a new directory from outside answers a changed line" "$out" 1 3
rmdir "$WT/made"

# The negative control, which is what proves the watch is on the listed directory and not on the box.
out=$(watch_run "$WT" touch "$OTHER/elsewhere.txt")
check_changed "a change in a directory that is not listed answers nothing" "$out" 0 0

# Navigating drops the old watch. Without the descriptor check in src/backend/watch.rs the removal's
# own IN_IGNORED would answer here, so this case reddens on exactly the bug it was written for.
out=$( ( printf '{"c":"list","path":"%s","first":10}\n' "$WT"
         sleep 0.4
         printf '{"c":"list","path":"%s","first":10}\n' "$OTHER"
         sleep 0.6
         touch "$WT/after-leaving.txt"
         sleep 0.6
         printf '{"c":"quit"}\n' ) | $BIN --backend)
check_changed "a change in the directory just left answers nothing" "$out" 0 0
check "and moving to a new directory answers nothing on its own" "2" "$(echo "$out" | grep -c '"t":"listed"')"

# A search replaces the listing with matches, which are not a directory, so nothing is watched. The
# list comes first on purpose: without a watch to stop, this case passes with the stop deleted.
out=$( ( printf '{"c":"list","path":"%s","first":10}\n' "$WT"
         sleep 0.4
         printf '{"c":"search","path":"%s","query":"alpha"}\n' "$WT"
         sleep 0.6
         touch "$WT/during-search.txt"
         sleep 0.6
         printf '{"c":"quit"}\n' ) | $BIN --backend)
check_changed "a change under a search answers nothing, because matches are not a directory" "$out" 0 0

# A live session answers from its index: open changes no rows, a zz query finds nothing, the backend's own create shows next, and the gen names each answer.
SW="$SB/session"
mkdir -p "$SW/sub/deep"
printf 'x' > "$SW/other.txt"
out=$( ( printf '{"c":"searchopen","path":"%s","folder":"%s"}\n' "$SW" "$SW"
         sleep 0.5
         printf '{"c":"searchquery","query":"zz","gen":1}\n'
         sleep 0.4
         printf '{"c":"newfile","path":"%s","name":"zzfile.txt","id":7}\n' "$SW"
         sleep 0.4
         printf '{"c":"searchquery","query":"zz","gen":2}\n'
         sleep 0.4
         printf '{"c":"quit"}\n' ) | $BIN --backend)
check "open answers searchopened and changes no rows" "1" "$(echo "$out" | grep -c '"t":"searchopened"')"
check "the first query finds no zz" "0" "$(echo "$out" | grep '"t":"searched"' | sed -n 1p | grep -oE '"n":[0-9]+' | cut -d: -f2)"
check "the backend's own create shows in the next query" "1" "$(echo "$out" | grep '"t":"searched"' | sed -n 2p | grep -oE '"n":[0-9]+' | cut -d: -f2)"
check "that answer carries the newer generation" "2" "$(echo "$out" | grep '"t":"searched"' | sed -n 2p | grep -oE '"gen":[0-9]+' | cut -d: -f2)"
check "that answer lands after the walk" "1" "$(echo "$out" | grep '"t":"searched"' | sed -n 2p | grep -c '"walking":false')"

# An outside file two levels down stays invisible until a reopen re-walks the scope.
out=$( ( printf '{"c":"searchopen","path":"%s","folder":"%s"}\n' "$SW" "$SW"
         sleep 0.5
         printf '{"c":"searchquery","query":"deepzz","gen":1}\n'
         sleep 0.4
         : > "$SW/sub/deep/deepzz.txt"
         sleep 0.4
         printf '{"c":"searchquery","query":"deepzz","gen":2}\n'
         sleep 0.4
         printf '{"c":"searchclose"}\n'
         sleep 0.2
         printf '{"c":"searchopen","path":"%s","folder":"%s"}\n' "$SW" "$SW"
         sleep 0.5
         printf '{"c":"searchquery","query":"deepzz","gen":3}\n'
         sleep 0.4
         printf '{"c":"quit"}\n' ) | $BIN --backend)
check "an outside change two levels down hides until a reopen" "0" "$(echo "$out" | grep '"t":"searched"' | sed -n 2p | grep -oE '"n":[0-9]+' | cut -d: -f2)"
check "the reopen walks it into the index" "1" "$(echo "$out" | grep '"t":"searched"' | sed -n 3p | grep -oE '"n":[0-9]+' | cut -d: -f2)"
rm -rf "$SW"

# A cancel right behind the open races the 200-directory walk: mid-walk it answers one searched line naming the stop, after the landing it says nothing; run_session_tests parks the walk to pin the stop.
SWC="$SB/sessioncancel"
mkdir -p "$SWC"
i=1; while [ "$i" -le 200 ]; do mkdir "$SWC/d$i"; i=$((i + 1)); done
printf 'x' > "$SWC/markA.txt"
printf 'x' > "$SWC/markB.txt"
out=$( ( printf '{"c":"searchopen","path":"%s","folder":"%s"}\n' "$SWC" "$SWC"
         printf '{"c":"searchcancel"}\n'
         sleep 0.5
         printf '{"c":"searchquery","query":"mark","gen":1}\n'
         sleep 0.5
         printf '{"c":"searchquery","query":"mark","gen":2}\n'
         sleep 0.5
         printf '{"c":"quit"}\n' ) | $BIN --backend)
check "the raced open still answers searchopened" "1" "$(echo "$out" | grep -c '"t":"searchopened"')"
# Sample input: the lines between searchopened and gen 1's first line, either none or {"t":"searched","n":0,...,"cancelled":true,"gen":0}.
pre=$(echo "$out" | sed -n '/"t":"searchopened"/,/"gen":1[,}]/p' | sed '1d;$d')
check "a cancel answers at most one searched line, and it names the stop" "ok" "$(if [ -z "$pre" ] || { [ "$(printf '%s\n' "$pre" | wc -l)" = 1 ] && printf '%s' "$pre" | grep -q '"t":"searched".*"cancelled":true'; }; then echo ok; else printf '%s' "$pre"; fi)"
check "a later query is still answered from the kept session" "1" "$(echo "$out" | grep '"t":"searched"' | grep -c '"gen":1[,}]')"
rm -rf "$SWC"
# listpaths lists a set the client named, whose base is the root; watching that would be a lie.
out=$( ( printf '{"c":"list","path":"%s","first":10}\n' "$WT"
         sleep 0.4
         printf '{"c":"listpaths","paths":["%s/alpha.txt"],"first":10}\n' "$WT"
         sleep 0.6
         touch "$WT/during-listpaths.txt"
         sleep 0.6
         printf '{"c":"quit"}\n' ) | $BIN --backend)
check_changed "a change under listpaths answers nothing, because named paths are not a directory" "$out" 0 0

# One bit of MASK each, isolated: a chmod is IN_ATTRIB alone, and appending to a file that already
# exists is IN_CLOSE_WRITE alone, since IN_MODIFY is deliberately not in the mask.
append_to_alpha() {
  printf 'x' >> "$WT/alpha.txt"
}

out=$(watch_run "$WT" chmod 0640 "$WT/alpha.txt")
check_changed "a chmod from outside answers a changed line" "$out" 1 3
chmod 0644 "$WT/alpha.txt"

out=$(watch_run "$WT" append_to_alpha)
check_changed "a writer closing a file it appended to answers a changed line" "$out" 1 3

# The watched directory itself: a move keeps the watch, which IN_MOVE_SELF is what reports, and a
# delete takes the watch with it, which the kernel reports as IN_IGNORED whatever the mask holds.
SELFDIR="$WT_SB/self-move"
mkdir -p "$SELFDIR"
out=$(watch_run "$SELFDIR" mv "$SELFDIR" "$WT_SB/self-moved")
check_changed "moving the watched directory itself answers a changed line" "$out" 1 3

SELFDEL="$WT_SB/self-delete"
mkdir -p "$SELFDEL"
out=$(watch_run "$SELFDEL" rmdir "$SELFDEL")
check_changed "deleting the watched directory answers a changed line, from the watch's own removal" "$out" 1 3

# A failed list answers its error and the directory still on screen keeps answering changed. Two changes
# a burst apart, because a watch the failure took away answers the one line its own removal made.
out=$( ( printf '{"c":"list","path":"%s","first":10}\n' "$WT"
         sleep 0.4
         printf '{"c":"list","path":"/no/such/directory","first":10}\n'
         sleep 0.4
         touch "$WT/after-a-failed-list-one.txt"
         sleep 1.0
         touch "$WT/after-a-failed-list-two.txt"
         sleep 0.8
         printf '{"c":"quit"}\n' ) | $BIN --backend)
check "a failed list still answers its error" "1" "$(echo "$out" | grep -c '"t":"error"')"
check_changed "and the directory still on screen is still watched after it" "$out" 2 4

# inotify answers the descriptor it already holds, so a commit that dropped it would unwatch the folder.
out=$( ( printf '{"c":"list","path":"%s","first":10}\n' "$WT"
         sleep 0.5
         printf '{"c":"list","path":"%s","first":10}\n' "$WT"
         sleep 1.5
         printf '{"c":"quit"}\n' ) | $BIN --backend)
check_changed "re-listing the same directory answers nothing on its own" "$out" 0 0

# Two changes, a second apart so each is its own burst: a dead watch answers the one spurious line
# its own removal made and nothing else, which one change alone could not be told apart from.
out=$( ( printf '{"c":"list","path":"%s","first":10}\n' "$WT"
         sleep 0.5
         printf '{"c":"list","path":"%s","first":10}\n' "$WT"
         sleep 0.5
         touch "$WT/after-a-relist-one.txt"
         sleep 1.0
         touch "$WT/after-a-relist-two.txt"
         sleep 0.8
         printf '{"c":"quit"}\n' ) | $BIN --backend)
check_changed "and two changes after that re-list are both answered" "$out" 2 4

# A burst is coalesced by the reader, so a hundred creates cost a handful of lines, not a hundred.
out=$(watch_run "$WT" burst_of_creates)
check_changed "a hundred creates answer a handful of changed lines, not a hundred" "$out" 1 10
sandbox_remove "$WT_SB"

# A test-only opendir barrier pins a real size worker until the parent releases it. These
# requests must finish while it is blocked; no sleeps or tree-size guesses choose the race.
ASYNC_SB="$FIXTURE_ROOT/flea-dirsize-async-$$"
sandbox_make "$ASYNC_SB"
python3 tests/dirsize-async.py "$ASYNC_SB" "$BIN"
check "running size jobs allow cancel, list, sort and quit without stale replies" "0" "$?"
sandbox_remove "$ASYNC_SB"

# The folder jump through the real binary, with a zoxide of this suite's own first on PATH, so the
# operator's database is never read. The missing favourite and zoxide row are dropped, the recent file
# stands for its folder, each folder is answered once, in the first source that names it, and zoxide's
# score rides along for every folder it ranks, the favourite it also ranks included.
JUMP_BIN="$SB/jump-bin"
NO_ZOXIDE="$SB/no-zoxide"
mkdir -p "$JUMP_BIN" "$NO_ZOXIDE" "$D/ranked"
cat > "$JUMP_BIN/zoxide" <<EOF
#!/bin/sh
[ "\$*" = "query --list --all --score" ] || exit 3
printf '  %s %s\n' 9.5 '$D/ranked' 4.0 '$D/gone' 2.0 '$D/sub'
EOF
chmod +x "$JUMP_BIN/zoxide"
jump_run() {
  ( printf '{"c":"jump","id":7,"favourites":["%s/sub","%s/nope"],"recent":["%s/three.txt"]}\n' "$D" "$D" "$D"
    sleep 1
    printf '{"c":"quit"}\n' ) | PATH="$1" $BIN --backend 2>/dev/null | grep '"t":"jumped"' | sed 's/,"ms":[0-9.]*}$//'
}
check "jump answers each source's existing folders once, in order" \
  "{\"t\":\"jumped\",\"id\":7,\"favourites\":[\"$D/sub\"],\"zoxide\":[\"$D/ranked\"],\"recent\":[\"$D\"],\"frecency\":{\"$D/sub\":2,\"$D/ranked\":9.5}" \
  "$(jump_run "$JUMP_BIN:$PATH")"
check "with no zoxide installed its source is empty and nothing else changes" \
  "{\"t\":\"jumped\",\"id\":7,\"favourites\":[\"$D/sub\"],\"zoxide\":[],\"recent\":[\"$D\"],\"frecency\":{}" \
  "$(jump_run "$NO_ZOXIDE")"

# devs through the real binary: a symlink answers the device of the directory it names (/proc is never on the sandbox's device), and a missing or relative path answers 0.
DV_SB="$SB/devs"
mkdir -p "$DV_SB"
ln -s /proc "$DV_SB/link"
devs_run() {
  ( printf '{"c":"devs","paths":["%s","%s/link","%s/gone","."]}\n' "$DV_SB" "$DV_SB" "$DV_SB"
    sleep 1
    printf '{"c":"quit"}\n' ) | $BIN --backend 2>/dev/null | grep '"t":"devs"'
}
check "devs answers a symlink with its target's device and 0 for a path it cannot read" \
  "{\"t\":\"devs\",\"devs\":{\"$DV_SB\":$(stat -c %d "$DV_SB"),\"$DV_SB/link\":$(stat -L -c %d "$DV_SB/link"),\"$DV_SB/gone\":0,\".\":0}}" \
  "$(devs_run)"
check "and the link's answer is not the device its own directory sits on" "differs" \
  "$([ "$(stat -c %d "$DV_SB")" != "$(stat -L -c %d "$DV_SB/link")" ] && echo differs || echo same)"

# One zoxide run per open, one ranking for both asks; a second run answers another folder so a rerun shows in the rows.
COUNT_BIN="$SB/count-bin"
mkdir -p "$COUNT_BIN"
cat > "$COUNT_BIN/zoxide" <<EOF
#!/bin/sh
echo run >> '$SB/zoxide-runs'
[ "\$(wc -l < '$SB/zoxide-runs')" = 1 ] && printf '  %s %s\n' 9.5 '$D/ranked' || printf '  %s %s\n' 3.5 '$D/sub'
EOF
chmod +x "$COUNT_BIN/zoxide"
# Each ask waits for the previous answer, as the client's whole ask waits for the provisional one.
jump_pair() {
  rm -f "$SB/zoxide-runs"
  coproc BACKEND { PATH="$COUNT_BIN:$PATH" $BIN --backend 2>/dev/null; }
  printf '{"c":"jump","id":7,"favourites":[],"recent":[]}\n' >&"${BACKEND[1]}"
  IFS= read -r first <&"${BACKEND[0]}"
  printf '%s\n' "$1" >&"${BACKEND[1]}"
  IFS= read -r second <&"${BACKEND[0]}"
  printf '{"c":"quit"}\n' >&"${BACKEND[1]}"
  wait "$BACKEND_PID"
  printf '%s\n%s\n' "$first" "$second"
}
pair_answers=$(jump_pair '{"c":"jump","id":8,"ranking":7,"favourites":[],"recent":[]}')
check "a whole ask naming its provisional ask runs zoxide once" "1" "$(wc -l < "$SB/zoxide-runs" | tr -d ' ')"
check "and both asks answer with the same ranking" "2" "$(printf '%s\n' "$pair_answers" | grep -c "\"zoxide\":\[\"$D/ranked\"\],\"recent\":\[\],\"frecency\":{\"$D/ranked\":9.5}")"
jump_pair '{"c":"jump","id":8,"favourites":[],"recent":[]}' > /dev/null
check "an ask naming no ranking runs zoxide itself" "2" "$(wc -l < "$SB/zoxide-runs" | tr -d ' ')"
# Properties on a folder counts through the menu worker's own dispatch (src/backend/menu_actions.rs), which a direct serve() call in Rust never reaches.
# A snapshot answers nothing, so its step waits a fixed second; each contents then reads the folder that snapshot captured.
sandbox_make "$CO_SB"
mkdir -p "$CO_SB/count/box/deep"
printf 'abcd' > "$CO_SB/count/box/four.txt"
printf 'ab' > "$CO_SB/count/box/deep/two.txt"
out=$(backend_steps "$(printf '{"c":"list","path":"%s/count","first":10}' "$CO_SB")" 'wait:"t":"rows"' \
  '{"c":"menuaction","op":"snapshot","id":6,"rows":[0]}' 'do:sleep 1' \
  '{"c":"menuaction","op":"contents","id":6}' 'wait:"op":"contents"' \
  '{"c":"menuaction","op":"contents","id":7}' 'wait:"id":7' \
  '{"c":"menuaction","op":"snapshot","id":8,"rows":[0]}' 'do:sleep 1' \
  '{"c":"menuaction","op":"contents","id":8}' 'wait:"id":8,"op":"contents"')
check "contents through the worker ends on one done line for the captured folder" '"id":6,"op":"contents","ok":true' "$(echo "$out" | grep -oE '"id":6,"op":"contents","ok":true')"
check "that line counts every entry below the folder and ends the count" '"items":3,"partial":false,"done":true' "$(echo "$out" | grep '"id":6,"op":"contents"' | grep -oE '"items":[0-9]+,"partial":(true|false),"done":(true|false)')"
check "a contents naming another id than the snapshot is refused and not walked" '"id":7,"op":"contents","ok":false' "$(echo "$out" | grep -oE '"id":7,"op":"contents","ok":false')"
check "and the worker keeps serving the next snapshot after both" '"id":8,"op":"contents","ok":true' "$(echo "$out" | grep -oE '"id":8,"op":"contents","ok":true')"
check "a folder that ends inside the cadence answers that one line and no other" "1" "$(echo "$out" | grep -c '"id":6,"op":"contents"')"
sandbox_remove "$CO_SB"


# The flasher's wire: the probe runs on fixture bytes and no case opens a block device for writing.
mkdir -p "$SB/flash"
head -c 100000 /dev/urandom > "$SB/flash/disk.img"
printf 'not compressed at all' > "$SB/flash/fake.img.xz"
head -c 20000 /dev/urandom > "$SB/flash/plain.img"
gzip -k -f "$SB/flash/plain.img"
out=$(printf '{"c":"flashimage","path":"%s"}\n{"c":"quit"}\n' "$SB/flash/disk.img" | $BIN --backend)
check "a raw image probes its kind with its size known" "1" "$(echo "$out" | grep -c "\"t\":\"flashimage\",\"path\":\"$SB/flash/disk.img\",\"kind\":\"img\",\"bytes\":100000,\"unpacked\":100000,\"known\":true,\"member\":\"\"")"
out=$(printf '{"c":"flashimage","path":"%s"}\n{"c":"quit"}\n' "$SB/flash/fake.img.xz" | $BIN --backend)
check "magic refusing a mismatched suffix answers no kind" "1" "$(echo "$out" | grep -c "\"t\":\"flashimage\",\"path\":\"$SB/flash/fake.img.xz\",\"kind\":\"\"")"
out=$(printf '{"c":"flashimage","path":"%s"}\n{"c":"quit"}\n' "$SB/flash/plain.img.gz" | $BIN --backend)
check "a gzip image probes with an unknown size" "1" "$(echo "$out" | grep -c "\"kind\":\"gz\",\"bytes\":[0-9]*,\"unpacked\":0,\"known\":false")"
out=$(printf '{"c":"flashtargets","image":""}\n{"c":"quit"}\n' | FLEA_UDISKS=/nonexistent-helper $BIN --backend)
check "flashtargets answers one line with a drives array" "1" "$(echo "$out" | grep -c '^{"t":"flashtargets","drives":\[')"
# The refusal field of every listed drive is pinned by the flashtargets_line unit test, since this host may list no drive at all.
out=$(printf '{"c":"flash","id":41,"image":"%s","dev":"flea-no-such-drive","seq":0,"serial":"","bytes":0}\n{"c":"quit"}\n' "$SB/flash/disk.img" | FLEA_UDISKS=/nonexistent-helper $BIN --backend)
check "a flash that names no live drive still starts before it refuses" "1" "$(echo "$out" | grep -c '"t":"flashstarted","id":41')"
check "and ends changed with nothing touched" "1" "$(echo "$out" | grep -c '"t":"flashdone","id":41,"ok":false,"phase":"prepare","at":0,"err":"changed","blank":false')"
out=$(printf '{"c":"flashcancel","id":41}\n{"c":"flashimage","path":"%s"}\n{"c":"quit"}\n' "$SB/flash/disk.img" | $BIN --backend)
check "a cancel naming no running flash answers nothing of its own" "0" "$(echo "$out" | grep -c flashdone)"
check "and the stream behind it is still answered" "1" "$(echo "$out" | grep -c '"t":"flashimage"')"
out=$(printf '{"c":"flasheject","dev":"flea-no-such-drive","seq":0}\n{"c":"quit"}\n' | $BIN --backend)
check "ejecting a drive the kernel never listed is gone" "1" "$(echo "$out" | grep -c '{"t":"flasheject","ok":false,"err":"gone"}')"
# The helper's own decisions, gi stubbed: the installed helper carries no seam and unlocked volumes unmount before they lock.
udisks_out=$(python3 tests/flash-udisks.py "$PWD/tools/flea-udisks" 2>&1)
check "the udisks helper's decisions all hold" "1" "$(printf '%s\n' "$udisks_out" | grep -c '^flash-udisks: all checks passed$')"
printf '%s\n' "$udisks_out" | grep '^FAIL' || true

# The filter answers over the names the listing holds (issue 315): the matches as runs, then one view
# window of their rows, each naming its listing row. Its own folder, since the cases above grew the tree: 0 sub, 1 empty.txt, 2 three.txt.
F="$SB/filter"
mkdir -p "$F/sub"
: > "$F/empty.txt"
: > "$F/three.txt"
out=$(printf '{"c":"list","path":"%s","first":0}\n{"c":"filter","query":"T","gen":1,"at":0,"before":0,"count":10}\n{"c":"quit"}\n' "$F" | $BIN --backend)
check "a filter answers its matches as runs, case-insensitively, in listing order" '"n":2,"runs":[[1,2]]' "$(echo "$out" | sed -n 3p | grep -oE '"n":[0-9]+,"runs":\[\[[0-9,]*\]\]')"
check "the filtered line is stamped with the gen and the listing" '"gen":1,"n":2' "$(echo "$out" | sed -n 3p | grep -oE '"gen":[0-9]+,"n":[0-9]+')"
check "and its first window carries the matched rows, each with its listing row" "empty.txt:1 three.txt:2" "$(echo "$out" | sed -n 4p | grep -oE '"n":"[^"]+"|"r":[0-9]+' | paste -d: - - | tr -d '"' | sed 's/^n://; s/:r:/:/' | tr '\n' ' ' | sed 's/ $//')"
check "the frows line is stamped with the listing and the gen" '"listing":1,"gen":1}' "$(echo "$out" | sed -n 4p | grep -oE '"listing":[0-9]+,"gen":[0-9]+}$')"
check "a rows line still carries no listing row of its own" "0" "$(echo "$out" | sed -n 2p | grep -c '"r":')"
# The first window sits around the client's cursor row: the matches of t are rows 1 and 2, so a cursor on row 2 is position 1.
out=$(printf '{"c":"list","path":"%s","first":0}\n{"c":"filter","query":"t","gen":1,"at":2,"before":0,"count":10}\n{"c":"filter","query":"t","gen":2,"at":2,"before":5,"count":10}\n{"c":"filter","query":"t","gen":3,"at":0,"before":0,"count":10}\n{"c":"filterwindow","start":9,"count":5}\n{"c":"quit"}\n' "$F" | $BIN --backend)
check "the first window starts at the match on the cursor's row, less the buffer ahead" '"start":1' "$(echo "$out" | sed -n 4p | grep -oE '"start":[0-9]+')"
check "and holds that row" "three.txt:2" "$(echo "$out" | sed -n 4p | grep -oE '"n":"[^"]+"|"r":[0-9]+' | paste -d: - - | tr -d '"' | sed 's/^n://; s/:r:/:/' | tr '\n' ' ' | sed 's/ $//')"
check "a buffer longer than the matches ahead clamps the start at the first match" '"start":0' "$(echo "$out" | sed -n 6p | grep -oE '"start":[0-9]+')"
check "a cursor row no match is at starts the window at the first match too" '"start":0' "$(echo "$out" | sed -n 8p | grep -oE '"start":[0-9]+')"
check "a window past the matches says where they end and names no row" '"start":2,"rows":[]' "$(echo "$out" | sed -n 9p | grep -oE '"start":[0-9]+,"rows":\[\]')"
out=$(printf '{"c":"list","path":"%s","first":0}\n{"c":"filter","query":"t","gen":1,"at":0,"before":0,"count":10}\n{"c":"filterwindow","start":1,"count":5}\n{"c":"filter","query":"e","gen":1,"at":0,"before":0,"count":10}\n{"c":"filterclose"}\n{"c":"filterwindow","start":0,"count":5}\n{"c":"quit"}\n' "$F" | $BIN --backend)
check "a filter window names the standing matches from a view position" "three.txt" "$(echo "$out" | sed -n 5p | grep -oE '"n":"[^"]+"' | cut -d'"' -f4 | tr '\n' ' ' | sed 's/ $//')"
check "a query whose gen is not newer is dropped in silence" "1" "$(echo "$out" | grep -c '"t":"filtered"')"
check "and a window after the close answers nothing" "2" "$(echo "$out" | grep -c '"t":"frows"')"
out=$(printf '{"c":"list","path":"%s","first":0}\n{"c":"filter","query":"t","gen":1,"at":0,"before":0,"count":10}\n{"c":"sort","by":"name","desc":true}\n{"c":"filterwindow","start":0,"count":5}\n{"c":"filter","query":"t","gen":2,"at":0,"before":0,"count":10}\n{"c":"list","path":"%s","first":0}\n{"c":"filterwindow","start":0,"count":5}\n{"c":"quit"}\n' "$F" "$F" | $BIN --backend)
check "a sort or a list drops the standing matches, so a window over the new numbering is silent until the next query" "2" "$(echo "$out" | grep -c '"t":"frows"')"
check "and the next query answers over the new order" "three.txt:1 empty.txt:2" "$(echo "$out" | grep '"t":"frows"' | tail -1 | grep -oE '"n":"[^"]+"|"r":[0-9]+' | paste -d: - - | tr -d '"' | sed 's/^n://; s/:r:/:/' | tr '\n' ' ' | sed 's/ $//')"

# No per-key cleanup: the cache is inside the sandbox, so it goes when the sandbox does.
sandbox_remove "$SB"
exit $fail
