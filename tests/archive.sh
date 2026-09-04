#!/bin/bash
# Drives compress, extract and convert through the real binary, including the hostile archive the
# operations design insists is proven on this bsdtar build rather than trusted as a libarchive default.
set -u
# Hard rule 9's guard, which owns FIXTURE_ROOT and every create and delete below.
. "$(dirname "$0")/../tools/flea-sandbox-guard"

cd "$(dirname "$0")/.." || exit 1
BIN=./target/debug/flea
D="$FIXTURE_ROOT/flea-archive-test-$$"
fail=0


cleanup() {
  exec 3>&- 2>/dev/null
  [ -n "${BACKEND_PID:-}" ] && kill "$BACKEND_PID" 2>/dev/null
  sandbox_remove "$D"
}
trap cleanup EXIT

check() {
  local label="$1" expected="$2" actual="$3"
  if [ "$expected" != "$actual" ]; then
    echo "FAIL $label"; echo "  expected: $expected"; echo "  actual:   $actual"; fail=1
  else
    echo "ok   $label"
  fi
}

start_backend() {
  rm -f "$D/out"; : > "$D/out"; rm -f "$D/in"; mkfifo "$D/in"
  $BIN --backend < "$D/in" > "$D/out" 2>/dev/null &
  BACKEND_PID=$!
  exec 3> "$D/in"
}
stop_backend() { send '{"c":"quit"}'; exec 3>&-; wait "$BACKEND_PID" 2>/dev/null; BACKEND_PID=""; }
send() { printf '%s\n' "$1" >&3; }
await() {
  local pattern="$1" i
  for i in $(seq 1 300); do grep -q -- "$pattern" "$D/out" && return 0; sleep 0.1; done
  echo "  TIMEOUT waiting for: $pattern"; return 1
}
seen() { grep -c -- "$1" "$D/out" | tr -d ' '; }

sandbox_make "$D" || exit 1
mkdir -p "$D/src/sub" || exit 1
printf 'alpha' > "$D/src/a.txt"
printf 'beta'  > "$D/src/sub/b.txt"

echo "--- the format table is what this box installs, never a fixed list ---"
start_backend
send '{"c":"formats"}'
await '"t":"formats"' || fail=1
formats=$(grep '"t":"formats"' "$D/out" | head -1)
check "bsdtar's own formats are offered" "1" "$(printf '%s' "$formats" | grep -c '"tar.zst"')"
check "and the convert tool is reported" "1" "$(printf '%s' "$formats" | grep -c '"convert":true')"
echo "  $formats"
stop_backend

seven_zip_installed=0
if command -v 7z >/dev/null 2>&1; then
  seven_zip_installed=1
fi
seven_zip_advertised=$(printf '%s' "$formats" | grep -c '"7z"')
check "7z availability matches the advertised format" "$seven_zip_installed" "$seven_zip_advertised"

# tar.zst and zip are required; exercise optional 7z exactly when the live backend offers it.
round_trip_formats="tar.zst zip"
if [ "$seven_zip_advertised" = 1 ]; then
  round_trip_formats="$round_trip_formats 7z"
else
  echo "--- 7z is unavailable; optional round trip skipped ---"
fi

for format in $round_trip_formats; do
  echo "--- compress and extract $format ---"
  start_backend
  send "{\"c\":\"archive\",\"op\":\"compress\",\"paths\":[\"$D/src/a.txt\",\"$D/src/sub\"],\"dest\":\"$D/out.$format\",\"format\":\"$format\"}"
  await '"t":"archivedone"' || fail=1
  check "$format compressed without error" "1" "$(seen '"t":"archivedone","id":1,"ok":true')"
  # A compress writes the archive rather than reading one, so it has nothing to verify against.
  check "and a compress reports itself verified" "1" "$(seen '"ok":true,"verified":true')"
  check "$format archive is on disk and not empty" "yes" "$([ -s "$D/out.$format" ] && echo yes || echo no)"
  check "$format announced its start with an id" "1" "$(seen '"t":"archivestarted","id":1')"
  send "{\"c\":\"archive\",\"op\":\"extract\",\"path\":\"$D/out.$format\",\"dest\":\"$D/back-$format\"}"
  # The started line carries the same id, so the wait names the terminal line and not just the id.
  await '"t":"archivedone","id":2' || fail=1
  check "$format extracted without error" "1" "$(seen '"t":"archivedone","id":2,"ok":true')"
  check "and a readable index means the extract was verified" "1" "$(seen '"id":2,"ok":true,"verified":true')"
  check "$format round trip kept the file" "alpha" "$(cat "$D/back-$format/a.txt" 2>/dev/null)"
  check "$format round trip kept the subdirectory" "beta" "$(cat "$D/back-$format/sub/b.txt" 2>/dev/null)"
  stop_backend
done

echo "--- a destination already there is refused, both directions ---"
start_backend
printf 'already here' > "$D/taken.zip"
send "{\"c\":\"archive\",\"op\":\"compress\",\"paths\":[\"$D/src/a.txt\"],\"dest\":\"$D/taken.zip\",\"format\":\"zip\"}"
await '"t":"archivedone"' || fail=1
check "a compress onto a taken name fails" "1" "$(seen '"ok":false')"
check "and the file that was there is untouched" "already here" "$(cat "$D/taken.zip")"
send "{\"c\":\"archive\",\"op\":\"extract\",\"path\":\"$D/out.zip\",\"dest\":\"$D/src\"}"
await '"t":"archivedone","id":2' || fail=1
check "an extract into a directory in use fails rather than merging" "2" "$(seen '"ok":false')"
check "and that directory still holds only what it had" "alpha" "$(cat "$D/src/a.txt")"
stop_backend

# Staged like compress and convert: a tool that fails mid-way must leave no destination at all.
echo "--- a failed extract leaves no partial destination and no work directory ---"
start_backend
printf 'this is not an archive' > "$D/notreally.zip"
send "{\"c\":\"archive\",\"op\":\"extract\",\"path\":\"$D/notreally.zip\",\"dest\":\"$D/nowhere\"}"
await '"t":"archivedone"' || fail=1
check "a corrupt archive fails" "1" "$(seen '"ok":false')"
check "and no destination was left behind" "no" "$([ -e "$D/nowhere" ] && echo yes || echo no)"
check "and no work directory survived it" "0" "$(find "$D" -maxdepth 1 -name '.flea-work-*' | wc -l | tr -d ' ')"
stop_backend

echo "--- the hostile archive: nothing escapes the destination ---"
# Built by hand with a .. component, which is the zip-slip shape with its own CVE history.
mkdir -p "$D/evil/sub"
printf 'ESCAPED' > "$D/evil/sub/escape.txt"
( cd "$D/evil/sub" && bsdtar -c -f "$D/hostile.tar" -C "$D/evil" ../evil/sub/escape.txt 2>/dev/null ) || \
  ( cd "$D/evil" && bsdtar -c -f "$D/hostile.tar" --format ustar -s '|^|../|' sub/escape.txt 2>/dev/null )
echo "  archive holds: $(bsdtar -tf "$D/hostile.tar" 2>/dev/null | tr '\n' ' ')"
start_backend
send "{\"c\":\"archive\",\"op\":\"extract\",\"path\":\"$D/hostile.tar\",\"dest\":\"$D/unpacked\"}"
await '"t":"archivedone"' || fail=1
check "nothing landed beside the destination" "no" "$([ -e "$D/escape.txt" ] && echo yes || echo no)"
check "nothing landed above it either" "no" "$([ -e "$FIXTURE_ROOT/escape.txt" ] && echo yes || echo no)"
stop_backend

echo "--- convert, with and without the metadata strip ---"
magick -size 64x48 xc:navy "$D/shot.png"
magick "$D/shot.png" -set comment 'flea test comment' "$D/tagged.png"
before=$(magick identify -verbose "$D/tagged.png" 2>/dev/null | grep -c 'flea test comment')
check "the fixture really carries the metadata to be stripped" "1" "$before"
start_backend
send "{\"c\":\"convert\",\"path\":\"$D/shot.png\",\"dest\":\"$D/shot (converted).jpg\",\"strip\":false}"
await '"t":"convertdone"' || fail=1
check "a convert answers with the path it wrote" "1" "$(seen '"t":"convertdone","id":1,"ok":true')"
check "and the jpeg is on disk" "JPEG" "$(magick identify -format '%m' "$D/shot (converted).jpg" 2>/dev/null)"
check "the source is untouched" "PNG" "$(magick identify -format '%m' "$D/shot.png" 2>/dev/null)"
send "{\"c\":\"convert\",\"path\":\"$D/tagged.png\",\"dest\":\"$D/tagged (stripped).png\",\"strip\":true}"
await '"t":"convertdone","id":2' || fail=1
after=$(magick identify -verbose "$D/tagged (stripped).png" 2>/dev/null | grep -c 'flea test comment')
check "strip actually removed the metadata" "0" "$after"
check "and the original still has it" "1" "$(magick identify -verbose "$D/tagged.png" 2>/dev/null | grep -c 'flea test comment')"
stop_backend

# The count is exact however long the listing is; only the names are capped, and the tile's
# "+ N more" line is the difference. Proven on a real archive built past that cap.
echo "--- a long index counts every entry and sends only its first names ---"
CAP=$(grep -o 'ARCHIVE_NAME_CAP: usize = [0-9_]*' src/backend/archivelist.rs | grep -o '[0-9_]*$' | tr -d _)
check "the cap is a number read out of the source, not one written here" "yes" \
  "$([ -n "$CAP" ] && [ "$CAP" -gt 0 ] && echo yes || echo no)"
TOTAL=$((CAP + 200))
mkdir -p "$D/big/payload" "$D/small" "$D/odd"
i=0
while [ "$i" -lt "$TOTAL" ]; do printf 'x' > "$D/big/payload/f$i"; i=$((i + 1)); done
bsdtar -a -c -f "$D/big/many.tar" -C "$D/big/payload" .
sandbox_require "$D/big/payload"; rm -rf "$D/big/payload"
bsdtar -a -c -f "$D/small/few.tar" -C "$D/src" a.txt

start_backend
# The archive is alone in its directory, so it is row 0 and no row search is needed.
send "{\"c\":\"list\",\"path\":\"$D/big\",\"first\":10}"
await '"t":"rows"' || fail=1
send '{"c":"meta","row":0,"archive":true}'
await '"t":"meta"' || fail=1
big=$(grep '"t":"meta"' "$D/out" | head -1)
# The archive holds every payload file plus the "." directory entry bsdtar writes for -C .
check "every entry is counted, not just the ones whose names were sent" "1" \
  "$(printf '%s' "$big" | grep -c "\"entries\":$((TOTAL + 1)),")"
check "the listing is never reported as failed" "1" "$(printf '%s' "$big" | grep -c '"afailed":false')"
check "and exactly the cap of names reached the wire" "$CAP" \
  "$(printf '%s' "$big" | grep -o '{"n":' | wc -l | tr -d ' ')"
stop_backend

start_backend
send "{\"c\":\"list\",\"path\":\"$D/small\",\"first\":10}"
await '"t":"rows"' || fail=1
send '{"c":"meta","row":0,"archive":true}'
await '"t":"meta"' || fail=1
small=$(grep '"t":"meta"' "$D/out" | head -1)
check "a one-entry archive counts one" "1" "$(printf '%s' "$small" | grep -c '"entries":1,')"
check "and names it" "1" "$(printf '%s' "$small" | grep -c '"names":\[{"n":"a.txt","d":false}\]')"
echo "  $small"
stop_backend

# bsdtar escapes a non-UTF-8 name to \351 in its own output, so this proves the end-to-end path and
# not the raw-byte one; the decisive case for that is the unit test in src/backend/archive.rs, which
# feeds the bytes straight to the parser.
echo "--- a name bsdtar escapes does not truncate the count ---"
printf 'one' > "$D/odd/a.txt"
printf 'two' > "$D/odd/$(printf 'caf\xe9').txt"
printf 'three' > "$D/odd/z.txt"
mkdir -p "$D/oddarc"
bsdtar -a -c -f "$D/oddarc/odd.tar" -C "$D/odd" . 2>/dev/null
start_backend
send "{\"c\":\"list\",\"path\":\"$D/oddarc\",\"first\":10}"
await '"t":"rows"' || fail=1
send '{"c":"meta","row":0,"archive":true}'
await '"t":"meta"' || fail=1
odd=$(grep '"t":"meta"' "$D/out" | head -1)
check "all four entries are counted past the latin-1 name" "1" "$(printf '%s' "$odd" | grep -c '"entries":4,')"
check "and every byte is summed" "1" "$(printf '%s' "$odd" | grep -c '"unpacked":11,')"
echo "  $odd"
stop_backend

# The count is documented as exact, so the one input that could inflate it is checked rather than
# assumed: a member whose name holds a newline. Neither tool emits it raw, bsdtar escapes it to a
# literal backslash-n and 7z substitutes an underscore, so one member stays one line in both. This
# test exists to catch that changing under us, because the wire calls the number exact.
echo "--- a newline in a member name does not become two entries ---"
mkdir -p "$D/nl/src"
printf 'one'   > "$D/nl/src/plain.txt"
printf 'two'   > "$D/nl/src/$(printf 'we\nird').txt"
printf 'three' > "$D/nl/src/z.txt"
bsdtar -a -c -f "$D/nl/nl.tar" -C "$D/nl/src" . 2>/dev/null
check "bsdtar prints the newline escaped, never raw" "1" \
  "$(bsdtar -tvf "$D/nl/nl.tar" | wc -l | tr -d ' ' | grep -c '^4$')"
start_backend
send "{\"c\":\"list\",\"path\":\"$D/nl\",\"first\":10}"
await '"t":"rows"' || fail=1
# The archive is row 0: src sorts as a directory first, and the archive is the only file.
send '{"c":"meta","row":1,"archive":true}'
await '"t":"meta"' || fail=1
nl=$(grep '"t":"meta"' "$D/out" | head -1)
check "the odd name counts once, not twice" "1" "$(printf '%s' "$nl" | grep -c '"entries":4,')"
echo "  $nl"
stop_backend

# An unreadable archive answering zero entries is indistinguishable from a read that has not
# happened, so the tool's own exit status is what says failed.
echo "--- an archive the tool cannot read says so, rather than answering zero ---"
mkdir -p "$D/badarc"
printf 'this is not an archive at all' > "$D/badarc/broken.tar"
start_backend
send "{\"c\":\"list\",\"path\":\"$D/badarc\",\"first\":10}"
await '"t":"rows"' || fail=1
send '{"c":"meta","row":0,"archive":true}'
await '"t":"meta"' || fail=1
bad=$(grep '"t":"meta"' "$D/out" | head -1)
check "a corrupt archive is reported failed" "1" "$(printf '%s' "$bad" | grep -c '"afailed":true')"
echo "  $bad"
stop_backend

echo "--- no work directory is left behind ---"
check "every private work directory was cleaned up" "0" "$(find "$D" -maxdepth 2 -name '.flea-work-*' | wc -l | tr -d ' ')"

echo
if [ "$fail" = 0 ]; then echo "archive.sh: all checks passed"; else echo "archive.sh: FAILURES above"; fi
exit "$fail"
