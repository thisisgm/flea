#!/bin/bash
# Issue 144: a slow list scan no longer holds transfer progress or a transfercancel; a fake gio blocks until released, so the scan is provably out before any line is judged.
set -u
. "$(dirname "$0")/../tools/flea-sandbox-guard"

cd "$(dirname "$0")/.." || exit 1
BIN=./target/debug/flea
# Without this every case below drives a missing binary and reports the result as a product failure.
[ -x "$BIN" ] || { echo "mtp-scan.sh: $BIN is missing, run cargo build" >&2; exit 1; }
D="$FIXTURE_ROOT/flea-mtp-scan-$$"
fail=0

cleanup() {
  [ ! -d "$D" ] || { : > "$D/copy-release-1"; : > "$D/copy-release-2"; : > "$D/gio-release"; }
  exec 3>&-
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
# A folder reports no total until its independent sweep settles, so use the fixture's exact byte count.
# Sample: {"t":"transferprogress","id":1,"index":0,"name":"source","bytes":262144,"total":0,"scanned":0}
flying() {
  local line bytes_re='"bytes":([0-9]+)' bytes
  while IFS= read -r line; do
    [[ $line == *'"t":"transferprogress"'* && $line =~ $bytes_re ]] || continue
    bytes=${BASH_REMATCH[1]}
    (( bytes > 0 && bytes < COPY_BYTES )) && return 0
  done < <(tail -c "+$(( $1 + 1 ))" "$D/out")
  return 1
}
# Polls are 0.05 s apart and default to 200 tries, ten seconds, because a case that never lands must fail rather than hang.
wait_file() {
  local path="$1" tries="${2:-200}" i
  for ((i = 0; i < tries; i++)); do [ -e "$path" ] && return 0; sleep 0.05; done
  return 1
}
await() {
  local pattern="$1" tries="${2:-200}" from="${3:-0}" i
  for ((i = 0; i < tries; i++)); do after "$pattern" "$from" && return 0; sleep 0.05; done
  return 1
}

sandbox_make "$D"
mkdir -p "$D/dest" "$D/undo"
# A transfer journals into the session runtime dir unless told otherwise, so the suite gives the backend its own undo dir.
chmod 0700 "$D/undo" || { echo "mtp-scan.sh: the suite undo dir could not be locked down" >&2; exit 1; }
export FLEA_UNDO_DIR="$D/undo"
# /run/user/*/gvfs/* is what src/backend/gvfslist.rs routes through one gio child, so the path names a phone mount without one attached.
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
# Hold the first two real reads, so progress and cancel cannot race a fast copy.
mkdir "$D/source" || { echo "mtp-scan.sh: the source folder could not be made" >&2; exit 1; }
dd if=/dev/zero of="$D/source/data.bin" bs=1M count=1 status=none || { echo "mtp-scan.sh: the dense source data.bin could not be made" >&2; exit 1; }
COPY_BYTES=$(stat -c %s "$D/source/data.bin")
cat > "$D/copy-block.c" <<'COPY_BLOCK'
#define _GNU_SOURCE
#include <errno.h>
#include <limits.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/syscall.h>
#include <time.h>
#include <unistd.h>
#include <fcntl.h>

static atomic_int cancel_read;
static _Thread_local unsigned reads;

static void mark(const char *root, const char *name) {
    char path[PATH_MAX];
    snprintf(path, sizeof path, "%s/%s", root, name);
    int fd = open(path, O_WRONLY | O_CREAT, 0600);
    if (fd >= 0) close(fd);
}

ssize_t read(int fd, void *buffer, size_t count) {
    const char *root = getenv("FLEA_MTP_COPY_ROOT");
    if (root && fd == STDIN_FILENO && atomic_load(&cancel_read)) mark(root, "cancel-processed");
    struct stat source, current;
    char path[PATH_MAX];
    if (root && fd != STDIN_FILENO && reads < 2) {
        snprintf(path, sizeof path, "%s/source/data.bin", root);
        if (!stat(path, &source) && !fstat(fd, &current) && source.st_dev == current.st_dev && source.st_ino == current.st_ino) {
            char name[32];
            snprintf(name, sizeof name, "copy-ready-%u", ++reads);
            mark(root, name);
            snprintf(path, sizeof path, "%s/copy-release-%u", root, reads);
            struct timespec start, now, pause = { .tv_nsec = 1000000 };
            clock_gettime(CLOCK_MONOTONIC, &start);
            while (access(path, F_OK)) {
                clock_gettime(CLOCK_MONOTONIC, &now);
                if (now.tv_sec - start.tv_sec >= 30) { errno = ETIMEDOUT; return -1; }
                nanosleep(&pause, NULL);
            }
        }
    }
    ssize_t size = syscall(SYS_read, fd, buffer, count);
    const char cancel[] = "\"c\":\"transfercancel\"";
    if (root && fd == STDIN_FILENO && size > 0 && memmem(buffer, size, cancel, sizeof cancel - 1)) {
        atomic_store(&cancel_read, 1);
        mark(root, "cancel-read");
    }
    return size;
}
COPY_BLOCK
cc -shared -fPIC -Wall -Werror -o "$D/copy-block.so" "$D/copy-block.c" || { echo "mtp-scan.sh: the copy-block preload could not be built from $D/copy-block.c" >&2; exit 1; }

rm -f "$D/out"; : > "$D/out"
mkfifo "$D/in"
LD_PRELOAD="$D/copy-block.so" FLEA_MTP_COPY_ROOT="$D" FLEA_GIO_BIN="$D/gio" FLEA_PREFETCH= $BIN --backend < "$D/in" > "$D/out" 2>/dev/null &
BACKEND_PID=$!
exec 3> "$D/in"

echo "--- a slow scan no longer holds progress or cancel ---"
send "{\"c\":\"transfer\",\"op\":\"copy\",\"paths\":[\"$D/source\"],\"dest\":\"$D/dest\"}"
wait_file "$D/copy-ready-1" || { echo "FAIL copy never reached its first read barrier"; exit 1; }
send "{\"c\":\"list\",\"path\":\"$PHONE\",\"first\":10}"
# The scan is out and blocked: the fake gio said so, and will not answer until this script releases it.
if wait_file "$GIO_STARTED" 200; then
  echo "ok   the fake gio started, so the scan is out and blocked"
else
  echo "FAIL the fake gio never started"; fail=1
fi
# Everything already written was written before the scan was provably blocked.
OFFSET=$(mark)
: > "$D/copy-release-1"
wait_file "$D/copy-ready-2" || { echo "FAIL copy never reached its second read barrier"; exit 1; }
# The scan is blocked, so a part-way progress line after this offset can only come from a loop outside the scan, and it shows the copy in flight.
progress_after=0
for ((i = 0; i < 200; i++)); do
  if flying "$OFFSET"; then progress_after=1; break; fi
  sleep 0.05
done
check "part-way progress is written while the scan is still blocked" "1" "$progress_after"
id=$(grep -o '"t":"transferstarted","id":[0-9]*' "$D/out" | head -1 | grep -o '[0-9]*$')
if [ -z "$id" ]; then
  echo "FAIL the copy never announced its id"; fail=1
else
  check "the copy is still running when the cancel is sent" "0" "$(seen '"t":"transferdone"')"
  CANCEL_AT=$(mark)
  send "{\"c\":\"transfercancel\",\"id\":$id}"
  wait_file "$D/cancel-read" || { echo "FAIL cancel was never read"; exit 1; }
  # A second stdin read holds the same inbox lock, so the preceding cancel has passed events::feed.
  send '{"c":"paths","rows":[0]}'
  wait_file "$D/cancel-processed" || { echo "FAIL cancel was never processed"; exit 1; }
  : > "$D/copy-release-2"
  # The done line says "cancelled":true only when the cancel ended the copy; one that finished on its own says false.
  if await "\"t\":\"transferdone\",\"id\":$id,.*\"cancelled\":true" 200 "$CANCEL_AT"; then
    echo "ok   cancel ends the copy while the scan is still blocked"
  else
    echo "FAIL cancel was never answered"; fail=1
  fi
fi
# Releasing the scan is the only thing that lets its listing answer, and with it any request queued behind it, so the order below is the proof.
: > "$GIO_RELEASE"
await '"t":"listed"' 200 || { echo "FAIL the listing was never answered"; fail=1; }
await '"t":"paths"' 200 || { echo "FAIL the request queued behind the scan was never answered"; fail=1; }

check "the cancel is the one that ended it" "1" "$(grep '"t":"transferdone","id":' "$D/out" | grep -c '"cancelled":true')"
check "the cancelled copy leaves nothing at the destination" "0" "$(ls -A "$D/dest" | wc -l | tr -d ' ')"
check "the listing is still answered after the scan lands" "1" "$(seen '"t":"listed"')"
check "the listing names the phone directory it scanned" "1" "$(seen "\"path\":\"$PHONE\"")"
progress_line=$(grep -n '"t":"transferprogress"' "$D/out" | head -1 | cut -d: -f1)
done_line=$(grep -n '"t":"transferdone","id":' "$D/out" | head -1 | cut -d: -f1)
listed_line=$(grep -n '"t":"listed"' "$D/out" | head -1 | cut -d: -f1)
check "progress lands before the listing's own answer" "1" \
  "$([ -n "$progress_line" ] && [ -n "$listed_line" ] && [ "$progress_line" -lt "$listed_line" ] && echo 1 || echo 0)"
check "the cancelled done lands before the listing's own answer" "1" \
  "$([ -n "$done_line" ] && [ -n "$listed_line" ] && [ "$done_line" -lt "$listed_line" ] && echo 1 || echo 0)"
paths_line=$(grep -n '"t":"paths"' "$D/out" | head -1 | cut -d: -f1)
check "the request sent during the scan is answered after its listing" "1" \
  "$([ -n "$paths_line" ] && [ -n "$listed_line" ] && [ "$paths_line" -gt "$listed_line" ] && echo 1 || echo 0)"
check "and against the listing the scan produced" "1" "$(seen "\"paths\":\[\"$PHONE/photo.jpg\"\]")"
send '{"c":"quit"}'
# The backend is gone before its journal is read, so a late write cannot hide.
for ((i = 0; i < 200; i++)); do kill -0 "$BACKEND_PID" 2>/dev/null || break; sleep 0.05; done
journal="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/flea/undo-journal"
leaked=0
[ -f "$journal" ] && grep -qF -- "$D" "$journal" && leaked=1
check "the run kept its sandbox paths out of the operator's undo journal" "0" "$leaked"

[ "$fail" -eq 0 ] || exit 1
echo "mtp-scan: scan=off-loop progress=live cancel=live queued=in-order"
