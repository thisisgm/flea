#!/bin/bash
# Drives the fetch request through the real binary: a file:// source lands in the picker cache with
# its content intact, a source that is not there leaves no dir behind, and a share scheme is refused
# before any tool runs. The cancel path is a unit test in src/backend/fetchreq.rs, because a copy
# slow enough to cancel needs a stub and a shell suite drives the real gio.
set -u
# Hard rule 9's guard, which owns FIXTURE_ROOT and every create and delete below.
. "$(dirname "$0")/../tools/flea-sandbox-guard"

cd "$(dirname "$0")/.." || exit 1
BIN=${BIN:-./target/debug/flea}
# Without this every case below drives a missing binary and reports the result as a product failure.
[ -x "$BIN" ] || { echo "fetch.sh: $BIN is missing, run cargo build" >&2; exit 1; }
D="$FIXTURE_ROOT/flea-fetch-test-$$"
# src/pickercache.rs honours XDG_CACHE_HOME, so every download lands inside this suite's own sandbox
# and the operator's real picker cache is never written to or swept.
export XDG_CACHE_HOME="$D/cache"
CACHE="$XDG_CACHE_HOME/flea/picker"
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
field() { printf '%s' "$1" | grep -oE "\"$2\":\"[^\"]*\"" | head -1 | cut -d'"' -f4; }
subdirs() { find "$CACHE" -mindepth 1 -maxdepth 1 -type d 2>/dev/null | wc -l | tr -d ' '; }

sandbox_make "$D" || exit 1
mkdir -p "$D/src" || exit 1
printf 'the body of the file, three lines\nsecond\nthird\n' > "$D/src/report.txt"
printf 'spaced' > "$D/src/a b.txt"

echo "--- a file:// source lands in the picker cache with its content intact ---"
start_backend
send "{\"c\":\"fetch\",\"uri\":\"file://$D/src/report.txt\"}"
await '"t":"fetchdone","id":1' || fail=1
started=$(grep '"t":"fetchstarted"' "$D/out" | head -1)
done_line=$(grep '"t":"fetchdone"' "$D/out" | head -1)
check "fetchstarted names the id and the uri" "1" "$(printf '%s' "$started" | grep -c "\"id\":1,\"uri\":\"file://$D/src/report.txt\"")"
check "fetchdone is ok" "1" "$(printf '%s' "$done_line" | grep -c '"ok":true')"
path=$(field "$done_line" path)
check "the path is under this suite's picker cache" "yes" "$(case "$path" in "$CACHE"/*/report.txt) echo yes ;; *) echo "no: $path" ;; esac)"
check "the file is readable and identical to the source" "yes" "$(cmp -s "$path" "$D/src/report.txt" && echo yes || echo no)"
check "the started line came before the done line" "fetchstarted" "$(grep -oE '"t":"fetch[a-z]+"' "$D/out" | head -1 | cut -d'"' -f4)"
stop_backend
check "the download outlives the backend" "yes" "$([ -f "$path" ] && echo yes || echo no)"

echo "--- the leaf is percent-decoded, and one dir per fetch keeps two downloads apart ---"
start_backend
send "{\"c\":\"fetch\",\"uri\":\"file://$D/src/a%20b.txt\"}"
await '"t":"fetchdone","id":1' || fail=1
send "{\"c\":\"fetch\",\"uri\":\"file://$D/src/a%20b.txt\"}"
await '"t":"fetchdone","id":2' || fail=1
first=$(field "$(grep '"t":"fetchdone","id":1' "$D/out")" path)
second=$(field "$(grep '"t":"fetchdone","id":2' "$D/out")" path)
check "the leaf is the decoded name" "a b.txt" "$(basename "$first")"
check "two fetches of one name land in two dirs" "yes" "$([ "$first" != "$second" ] && [ -f "$first" ] && [ -f "$second" ] && echo yes || echo no)"
check "the second fetch got its own id" "1" "$(grep -c '"t":"fetchstarted","id":2' "$D/out")"
stop_backend

echo "--- a source that is not there fails with gio's own words and leaves no dir ---"
before=$(subdirs)
start_backend
send "{\"c\":\"fetch\",\"uri\":\"file://$D/src/absent.txt\"}"
await '"t":"fetchdone","id":1' || fail=1
missing=$(grep '"t":"fetchdone"' "$D/out" | head -1)
check "fetchdone is not ok" "1" "$(printf '%s' "$missing" | grep -c '"ok":false')"
check "err carries gio's own line" "1" "$(printf '%s' "$missing" | grep -c '"err":"gio: ')"
check "no path field rides on a failure" "0" "$(printf '%s' "$missing" | grep -c '"path"')"
stop_backend
check "the failed fetch's dir is gone" "$before" "$(subdirs)"

echo "--- a share scheme is refused before any tool runs, with started and done both answered ---"
start_backend
send '{"c":"fetch","uri":"smb://server/share/doc.pdf"}'
await '"t":"fetchdone","id":1' || fail=1
refused=$(grep '"t":"fetchdone"' "$D/out" | head -1)
check "a share is refused" "1" "$(printf '%s' "$refused" | grep -c '"ok":false,"err":"scheme not accepted')"
check "and still got its fetchstarted" "1" "$(grep -c '"t":"fetchstarted","id":1,"uri":"smb://server/share/doc.pdf"' "$D/out")"
send '{"c":"fetch","uri":"/etc/hostname"}'
await '"t":"fetchdone","id":2' || fail=1
check "a bare path is not a fetch either" "1" "$(grep '"t":"fetchdone","id":2' "$D/out" | grep -c '"ok":false')"
# A cancel for an id that finished, or never ran, is a no-op and not a crash.
send '{"c":"fetchcancel","id":1}'
send '{"c":"fetchcancel","id":99}'
send '{"c":"fsinfo"}'
await '"t":"fsinfo"' || fail=1
check "the backend is still answering after two idle cancels" "1" "$(grep -c '"t":"fsinfo"' "$D/out")"
stop_backend
check "a refused fetch made no dir" "$before" "$(subdirs)"

echo
if [ "$fail" = 0 ]; then echo "fetch.sh: all checks passed"; else echo "fetch.sh: FAILURES above"; fi
exit "$fail"
