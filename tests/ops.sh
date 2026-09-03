#!/bin/bash
# Drives every write operation through the real binary and asserts both the wire lines and the filesystem.
set -u
# Hard rule 9's guard, which owns FIXTURE_ROOT and every create and delete below.
. "$(dirname "$0")/../tools/flea-sandbox-guard"

cd "$(dirname "$0")/.." || exit 1
BIN=./target/debug/flea
D="$FIXTURE_ROOT/flea-ops-test-$$"
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

# One backend process per scenario, because the undo journal is process-lifetime by design.
start_backend() {
  sandbox_make "$D"
  rm -f "$D/out"; : > "$D/out"
  mkfifo "$D/in"
  $BIN --backend < "$D/in" > "$D/out" 2>/dev/null &
  BACKEND_PID=$!
  exec 3> "$D/in"
}

stop_backend() {
  send '{"c":"quit"}'
  exec 3>&-
  wait "$BACKEND_PID" 2>/dev/null
  BACKEND_PID=""
}

send() { printf '%s\n' "$1" >&3; }

# A bounded poll that reads the file directly, never a background watcher.
await() {
  local pattern="$1" i
  for i in $(seq 1 200); do
    grep -q -- "$pattern" "$D/out" && return 0
    sleep 0.05
  done
  echo "  TIMEOUT waiting for: $pattern"
  return 1
}

seen() { grep -c -- "$1" "$D/out" | tr -d ' '; }

echo "--- rename, and undo puts the name back ---"
start_backend
printf 'body' > "$D/before.txt"
send "{\"c\":\"rename\",\"path\":\"$D/before.txt\",\"to\":\"after.txt\"}"
await '"t":"renamed"' || fail=1
check "rename answers ok" "1" "$(seen '"t":"renamed","ok":true')"
check "the new name exists" "yes" "$([ -f "$D/after.txt" ] && echo yes || echo no)"
send '{"c":"undo"}'
await '"t":"undone"' || fail=1
check "undo names the operation it reversed" "1" "$(seen '"t":"undone","op":"rename","ok":true')"
check "the original name is back" "yes" "$([ -f "$D/before.txt" ] && echo yes || echo no)"
check "the renamed file is gone" "no" "$([ -f "$D/after.txt" ] && echo yes || echo no)"
stop_backend

echo "--- rename refuses to clobber, and records nothing to undo ---"
start_backend
printf 'source' > "$D/a.txt"; printf 'target' > "$D/b.txt"
send "{\"c\":\"rename\",\"path\":\"$D/a.txt\",\"to\":\"b.txt\"}"
await '"t":"error"' || fail=1
check "the refusal is an error line naming rename" "1" "$(seen '"where":"rename"')"
check "the file that was there is untouched" "target" "$(cat "$D/b.txt")"
send '{"c":"undo"}'
await '"msg":"there is nothing to undo"' || fail=1
check "a refused rename left the journal empty" "1" "$(seen 'there is nothing to undo')"
stop_backend

echo "--- duplicate, and undo removes only the copy ---"
start_backend
printf 'pixels' > "$D/photo.jpg"
send "{\"c\":\"duplicate\",\"path\":\"$D/photo.jpg\"}"
await '"t":"duplicated"' || fail=1
check "duplicate names the file it wrote" "1" "$(seen '"t":"duplicated","ok":true,"path":"'"$D"'/photo copy.jpg"')"
check "the copy holds the same bytes" "pixels" "$(cat "$D/photo copy.jpg")"
send '{"c":"undo"}'
await '"t":"undone"' || fail=1
check "the copy is gone" "no" "$([ -f "$D/photo copy.jpg" ] && echo yes || echo no)"
check "the original is untouched" "pixels" "$(cat "$D/photo.jpg")"
stop_backend

echo "--- trash through gio, and undo restores from the trash ---"
start_backend
printf 'trash me' > "$D/doomed.txt"
send "{\"c\":\"trash\",\"paths\":[\"$D/doomed.txt\"]}"
await '"t":"trashed"' || fail=1
check "trash reports one ok and no failures" "1" "$(seen '"t":"trashed","ok":1,"failed":0')"
check "the file left the directory" "no" "$([ -e "$D/doomed.txt" ] && echo yes || echo no)"
send '{"c":"undo"}'
await '"t":"undone"' || fail=1
check "undo names trash as what it reversed" "1" "$(seen '"t":"undone","op":"trash","ok":true')"
check "the file is back with its bytes" "trash me" "$(cat "$D/doomed.txt" 2>/dev/null)"
stop_backend

echo "--- copy transfer, and undo removes what it created ---"
start_backend
printf 'one' > "$D/c1.txt"; printf 'two' > "$D/c2.txt"; mkdir -p "$D/dest"
send "{\"c\":\"transfer\",\"op\":\"copy\",\"paths\":[\"$D/c1.txt\",\"$D/c2.txt\"],\"dest\":\"$D/dest\"}"
await '"t":"transferdone"' || fail=1
check "started names the top-level count" "1" "$(seen '"t":"transferstarted","id":1,"n":2')"
check "and the verb, so the client never has to read its own clipboard" "1" "$(seen '"n":2,"moving":false')"
check "each item answers its own terminal line" "2" "$(seen '"t":"transferitem"')"
check "done counts two ok and nothing else" "1" "$(seen '"t":"transferdone","id":1,"ok":2,"failed":0,"skipped":0,"cancelled":false')"
check "both copies landed" "yes" "$([ -f "$D/dest/c1.txt" ] && [ -f "$D/dest/c2.txt" ] && echo yes || echo no)"
check "the sources are still there" "yes" "$([ -f "$D/c1.txt" ] && echo yes || echo no)"
send '{"c":"undo"}'
await '"t":"undone"' || fail=1
check "undo removed both copies" "no" "$([ -e "$D/dest/c1.txt" ] || [ -e "$D/dest/c2.txt" ] && echo yes || echo no)"
check "undo left the sources alone" "one" "$(cat "$D/c1.txt")"
stop_backend

echo "--- move transfer, and undo moves it back ---"
start_backend
printf 'moving' > "$D/m.txt"; mkdir -p "$D/dest"
send "{\"c\":\"transfer\",\"op\":\"move\",\"paths\":[\"$D/m.txt\"],\"dest\":\"$D/dest\"}"
await '"t":"transferdone"' || fail=1
check "a move says so on the wire, which is what the status line reads" "1" "$(seen '"moving":true')"
check "the source is gone after a move" "no" "$([ -e "$D/m.txt" ] && echo yes || echo no)"
check "the destination holds it" "moving" "$(cat "$D/dest/m.txt")"
send '{"c":"undo"}'
await '"t":"undone"' || fail=1
check "undo put it back where it came from" "moving" "$(cat "$D/m.txt" 2>/dev/null)"
check "the destination copy is gone" "no" "$([ -e "$D/dest/m.txt" ] && echo yes || echo no)"
stop_backend

echo "--- transfers between two synthetic GVFS mount roots use the normal backend ---"
start_backend
mkdir -p "$D/run/user/1000/gvfs/sftp:host=source" "$D/run/user/1000/gvfs/smb-share:server=dest,share=data"
printf 'remote copy' > "$D/run/user/1000/gvfs/sftp:host=source/copy.txt"
printf 'remote move' > "$D/run/user/1000/gvfs/sftp:host=source/move.txt"
remote_src="$D/run/user/1000/gvfs/sftp:host=source"
remote_dst="$D/run/user/1000/gvfs/smb-share:server=dest,share=data"
send "{\"c\":\"transfer\",\"op\":\"copy\",\"paths\":[\"$remote_src/copy.txt\"],\"dest\":\"$remote_dst\"}"
await '"t":"transferdone"' || fail=1
check "remote-to-remote copy preserves source bytes" "remote copy|remote copy" "$(cat "$remote_src/copy.txt")|$(cat "$remote_dst/copy.txt")"
send "{\"c\":\"transfer\",\"op\":\"move\",\"paths\":[\"$remote_src/move.txt\"],\"dest\":\"$remote_dst\"}"
for _attempt in $(seq 1 200); do [[ $(seen '"t":"transferdone"') -ge 2 ]] && break; sleep 0.05; done
check "remote-to-remote move removes the source only after landing" "no|remote move" "$([ -e "$remote_src/move.txt" ] && echo yes || echo no)|$(cat "$remote_dst/move.txt")"
stop_backend

echo "--- one failing item is data, and the batch carries on ---"
start_backend
printf 'good' > "$D/good.txt"; mkdir -p "$D/dest"
send "{\"c\":\"transfer\",\"op\":\"copy\",\"paths\":[\"$D/never-existed.txt\",\"$D/good.txt\"],\"dest\":\"$D/dest\"}"
await '"t":"transferdone"' || fail=1
check "the failure is one item's own line" "1" "$(seen '"ok":false,"err"')"
check "the item after it still ran" "good" "$(cat "$D/dest/good.txt" 2>/dev/null)"
check "done reports one of each" "1" "$(seen '"ok":1,"failed":1,"skipped":0')"
stop_backend

echo "--- a destination Flea will not create is refused before anything is touched ---"
start_backend
printf 'x' > "$D/x.txt"
send "{\"c\":\"transfer\",\"op\":\"copy\",\"paths\":[\"$D/x.txt\"],\"dest\":\"$D/no-such-dir\"}"
await '"t":"error"' || fail=1
check "the refusal names transfer" "1" "$(seen '"where":"transfer"')"
check "nothing was started" "0" "$(seen '"t":"transferstarted"')"
stop_backend

echo "--- a transfer never overwrites what is already at the destination ---"
start_backend
printf 'new' > "$D/dup.txt"; mkdir -p "$D/dest"; printf 'already here' > "$D/dest/dup.txt"
send "{\"c\":\"transfer\",\"op\":\"copy\",\"paths\":[\"$D/dup.txt\"],\"dest\":\"$D/dest\"}"
await '"t":"transferdone"' || fail=1
check "the item failed rather than clobbering" "1" "$(seen '"ok":false,"err"')"
check "the destination file is untouched by the refused copy" "already here" "$(cat "$D/dest/dup.txt")"
stop_backend

echo
if [ "$fail" = 0 ]; then echo "ops.sh: all checks passed"; else echo "ops.sh: FAILURES above"; fi
exit "$fail"
