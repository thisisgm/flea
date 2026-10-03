#!/bin/bash
# Issue 144: a list's scan runs off the backend's event loop, so a mount that takes seconds to answer
# a read_dir no longer freezes transfer progress or swallows a transfercancel. The slow scan is a
# fake gio (FLEA_GIO_BIN) that blocks until this script releases it, so nothing here races the clock:
# the scan is provably out and blocked before any line it must not hold back is judged.
set -u
. "$(dirname "$0")/../tools/flea-sandbox-guard"

cd "$(dirname "$0")/.." || exit 1
BIN=./target/debug/flea
# Without this every case below drives a missing binary and reports the result as a product failure.
[ -x "$BIN" ] || { echo "mtp-scan.sh: $BIN is missing, run cargo build" >&2; exit 1; }
D="$FIXTURE_ROOT/flea-mtp-scan-$$"
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
    echo "FAIL $label"
    echo "  expected: $expected"
    echo "  actual:   $actual"
    fail=1
  else
    echo "ok   $label"
  fi
}

send() { printf '%s\n' "$1" >&3; }
seen() { grep -c -- "$1" "$D/out" | tr -d ' '; }
# Bytes written so far: what tells a line written after the scan was provably blocked from one written before it.
mark() { wc -c < "$D/out" | tr -d ' '; }
# True when the pattern appears at or after byte offset $2.
after() { tail -c "+$(( $2 + 1 ))" "$D/out" | grep -q -- "$1"; }
# Bounded polls, in tenths of a second, because a case that never lands must fail rather than hang.
wait_file() {
  local path="$1" tries="${2:-200}" i
  for ((i = 0; i < tries; i++)); do [ -e "$path" ] && return 0; sleep 0.05; done
  return 1
}
await() {
  local pattern="$1" tries="${2:-200}" i
  for ((i = 0; i < tries; i++)); do grep -q -- "$pattern" "$D/out" && return 0; sleep 0.05; done
  return 1
}

sandbox_make "$D"
mkdir -p "$D/dest"
# /run/user/*/gvfs/* is exactly what src/backend/gvfslist.rs routes through one gio child, so the
# path names a phone mount without one being attached.
PHONE="/run/user/$(id -u)/gvfs/mtp:host=Pixel_7a/Internal shared storage/DCIM/OPENCAMERA"
GIO_STARTED="$D/gio-started"
GIO_RELEASE="$D/gio-release"
# The fake gio: it says it started, waits for this script, then answers one row and exits clean.
cat > "$D/gio" <<EOF
#!/bin/sh
: > "$GIO_STARTED"
i=0
while [ ! -e "$GIO_RELEASE" ] && [ "\$i" -lt 400 ]; do sleep 0.05; i=\$((i + 1)); done
printf 'mtp://phone/store/photo.jpg\t10\t(regular)\ttime::modified=1790537811\n'
EOF
chmod +x "$D/gio"
# A sparse source: the copy is cancelled long before its end, so almost none of it is ever written,
# and the file is big enough that a fast disk cannot finish it before the cancel lands.
truncate -s 4G "$D/source.bin"

rm -f "$D/out"; : > "$D/out"
mkfifo "$D/in"
FLEA_GIO_BIN="$D/gio" FLEA_PREFETCH= $BIN --backend < "$D/in" > "$D/out" 2>/dev/null &
BACKEND_PID=$!
exec 3> "$D/in"

echo "--- a slow scan no longer holds progress or cancel ---"
send "{\"c\":\"transfer\",\"op\":\"copy\",\"paths\":[\"$D/source.bin\"],\"dest\":\"$D/dest\"}"
send "{\"c\":\"list\",\"path\":\"$PHONE\",\"first\":10}"
# The scan is out and blocked: the fake gio said so, and will not answer until this script releases it.
if wait_file "$GIO_STARTED" 200; then
  echo "ok   the fake gio started, so the scan is out and blocked"
else
  echo "FAIL the fake gio never started"; fail=1
fi
# Everything already written was written before the scan was provably blocked.
OFFSET=$(mark)
# The copy is a 4 GiB sparse file and the scan is blocked, so a progress line after this offset can only
# have been written by a loop that is not inside the scan. A loop held by the scan writes none.
progress_after=0
for ((i = 0; i < 200; i++)); do
  if after '"t":"transferprogress"' "$OFFSET"; then progress_after=1; break; fi
  sleep 0.05
done
check "progress is written while the scan is still blocked" "1" "$progress_after"
# The id is read off the wire rather than assumed, so an earlier id-claiming request cannot skew this.
id=$(grep -o '"t":"transferstarted","id":[0-9]*' "$D/out" | head -1 | grep -o '[0-9]*$')
send "{\"c\":\"transfercancel\",\"id\":${id:-1}}"
if await '"t":"transferdone","id":' 200; then
  echo "ok   cancel ends the copy while the scan is still blocked"
else
  echo "FAIL cancel was never answered"; fail=1
fi
# Releasing the scan is the only thing that lets its listing answer, so the order below is the proof.
: > "$GIO_RELEASE"
await '"t":"listed"' 200 || { echo "FAIL the listing was never answered"; fail=1; }

check "the cancel is the one that ended it" "1" "$(seen '"cancelled":true')"
check "the listing is still answered after the scan lands" "1" "$(seen '"t":"listed"')"
check "the listing names the phone directory it scanned" "1" "$(seen "\"path\":\"$PHONE\"")"
progress_line=$(grep -n '"t":"transferprogress"' "$D/out" | head -1 | cut -d: -f1)
done_line=$(grep -n '"t":"transferdone","id":' "$D/out" | head -1 | cut -d: -f1)
listed_line=$(grep -n '"t":"listed"' "$D/out" | head -1 | cut -d: -f1)
check "progress lands before the listing's own answer" "1" \
  "$([ -n "$progress_line" ] && [ -n "$listed_line" ] && [ "$progress_line" -lt "$listed_line" ] && echo 1 || echo 0)"
check "the cancelled done lands before the listing's own answer" "1" \
  "$([ -n "$done_line" ] && [ -n "$listed_line" ] && [ "$done_line" -lt "$listed_line" ] && echo 1 || echo 0)"
send '{"c":"quit"}'

[ "$fail" -eq 0 ] || exit 1
echo "mtp-scan: scan=off-loop progress=live cancel=live"
