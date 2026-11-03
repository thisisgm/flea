#!/bin/bash
. "$(dirname "$0")/headless-commons.sh"
# The archive password threat suite: a canary travels every password flow and may reach only a tool's stdin (A real 7z, B cards, C static).
set -u
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1
BIN=./target/debug/flea
[ -x "$BIN" ] || { echo "archive-threat.sh: $BIN is missing, run cargo build" >&2; exit 1; }
CANARY=S3cr3t-canary-1006
WRONG=Not-The-Password-77
# The checks a full green walk makes, pinned exactly so a walk that skips steps cannot pass.
EXPECTED_CHECKS=42
D="$FIXTURE_ROOT/flea-archive-threat-$$"
fail=0

check() {
  local label="$1" expected="$2" actual="$3"
  if [ "$expected" != "$actual" ]; then
    echo "FAIL $label"; echo "  expected: $expected"; echo "  actual:   $actual"; fail=1
  else
    echo "ok   $label"
  fi
}

# Counts matching lines across files, and a missing file is a failure rather than a zero.
count_in() {
  local pattern="$1" f
  shift
  for f in "$@"; do [ -f "$f" ] || { echo "missing:$f"; return; }; done
  cat -- "$@" | grep -c -- "$pattern"
}

sandbox_make "$D" || exit 1
mkdir -p "$D/src" "$D/tmp" || exit 1
printf 'alpha' > "$D/src/a.txt"
printf 'beta' > "$D/src/b.txt"
printf 'not a 7z archive' > "$D/src/locked.7z"
printf 'taken' > "$D/dup.7z"

cleanup() {
  exec 3>&-
  [ -n "${BACKEND_PID:-}" ] && kill "$BACKEND_PID" 2>/dev/null
  [ -n "${SAMPLER:-}" ] && kill "$SAMPLER" 2>/dev/null
  sandbox_remove "$D"
}
trap cleanup EXIT

start_backend() {
  rm -f "$D/out" "$D/err"; : > "$D/out"; : > "$D/err"; rm -f "$D/in"; mkfifo "$D/in"
  TMPDIR="$D/tmp" $BIN --backend < "$D/in" > "$D/out" 2> "$D/err" &
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
# The same pattern can answer several jobs, so later waits count lines rather than matching stale ones.
await_count() {
  local pattern="$1" want="$2" i
  for i in $(seq 1 600); do [ "$(grep -c -- "$pattern" "$D/out")" -ge "$want" ] 2>/dev/null && return 0; sleep 0.1; done
  echo "  TIMEOUT waiting for $want lines of: $pattern"; return 1
}

dones=0
# One request, then its terminal line; the answer is the line a check then reads.
job() {
  send "$1"
  dones=$((dones + 1))
  await_count '"t":"archivedone"' "$dones" || fail=1
  ANSWER=$(grep '"t":"archivedone"' "$D/out" | sed -n "${dones}p")
}
compress() { # dest format password encryptNames, then the source names
  local dest="$1" format="$2" pw="$3" enc="$4" paths="" name
  shift 4
  for name in "$@"; do paths="$paths,\"$D/src/$name\""; done
  job "{\"c\":\"archive\",\"op\":\"compress\",\"paths\":[${paths#,}],\"dest\":\"$D/$dest\",\"format\":\"$format\",\"password\":\"$pw\",\"encryptNames\":$enc,\"menuId\":0}"
}
extract() { # archive dest, then the password when there is one
  local pw=""
  [ -n "${3+x}" ] && pw=",\"password\":\"$3\""
  job "{\"c\":\"archive\",\"op\":\"extract\",\"path\":\"$D/$1\",\"dest\":\"$D/$2\"$pw,\"menuId\":0}"
}
answered() { printf '%s\n' "$ANSWER" | grep -c -- "$1"; }

# One pass over every live 7z child: its argv and environment hold no canary, or the child was unreadable.
sample_children() {
  local pid
  for pid in $(pgrep -x '7z|7zz|7za'); do
    if cat "/proc/$pid/cmdline" "/proc/$pid/environ" >/dev/null 2>&1; then
      echo "$pid $(cat "/proc/$pid/cmdline" "/proc/$pid/environ" 2>/dev/null | grep -a -c -- "$CANARY")" >> "$D/sample.log"
    else
      echo "$pid unreadable" >> "$D/sample.log"
    fi
  done
}
sample_until_stopped() { while [ ! -e "$D/sample.stop" ]; do sample_children; sleep 0.02; done; }

phase_a() {
  head -c $((64 * 1024 * 1024)) /dev/urandom > "$D/src/big.bin" || { echo "FAIL archive-threat.sh: cannot write the 64 MiB fixture $D/src/big.bin"; exit 1; }
  # PPMd decodes about 5 MB/s, so slow.7z holds its extract open for over a second where an LZMA2 one closes in 75 ms.
  head -c $((8 * 1024 * 1024)) /dev/urandom > "$D/src/slow.bin" || { echo "FAIL archive-threat.sh: cannot write the 8 MiB fixture $D/src/slow.bin"; exit 1; }
  built=$(7z a -bd -m0=PPMd -p"$CANARY" "$D/slow.7z" "$D/src/slow.bin" 2>&1) || { echo "FAIL archive-threat.sh: 7z could not build the PPMd fixture $D/slow.7z:"; printf '%s\n' "$built" | tail -5; exit 1; }
  start_backend
  send '{"c":"formats"}'
  await '"t":"formats"' || fail=1

  compress out.7z 7z "$CANARY" true a.txt b.txt
  check "a password compress wrote its .7z" "1" "$([ -s "$D/out.7z" ] && answered '"ok":true')"
  compress names.7z 7z "$CANARY" false a.txt
  check "a password compress with plain names wrote its .7z" "1" "$([ -s "$D/names.7z" ] && answered '"ok":true')"
  compress pw.zip zip "$CANARY" false a.txt
  check "a password zip wrote its archive" "1" "$([ -s "$D/pw.zip" ] && answered '"ok":true')"
  check "and it is AES-256, never ZipCrypto" "yes" "$([ "$(7z l -slt "$D/pw.zip" < /dev/null 2>/dev/null | grep -c 'AES-256')" -ge 1 ] && echo yes || echo no)"
  job "{\"c\":\"archive\",\"op\":\"compress\",\"paths\":[\"$D/src/a.txt\"],\"dest\":\"$D/plain.tar.zst\",\"format\":\"tar.zst\",\"menuId\":0}"
  check "the plain control compress wrote its archive" "1" "$([ -s "$D/plain.tar.zst" ] && answered '"ok":true')"

  compress dup.7z 7z "$CANARY" false a.txt
  check "a password compress onto an existing name is refused" "1" "$(answered 'already exists')"
  check "and leaves that file as it was" "taken" "$(cat "$D/dup.7z")"

  extract out.7z ex_none
  check "an archive with encrypted names asks for a password before any extract" "1" "$(answered '"err":"encrypted"')"
  extract names.7z ex_none
  check "an archive with encrypted entries asks for one too" "1" "$(answered '"err":"encrypted"')"
  extract out.7z ex_wrong "$WRONG"
  check "a wrong password answers wrong-password" "1" "$(answered '"err":"wrong-password"')"
  check "and names the archive and destination to retry" "1" "$(answered "\"path\":\"$D/out.7z\",\"dest\":\"$D/ex_wrong\"")"
  check "and leaves no destination behind" "0" "$([ -e "$D/ex_wrong" ] && echo 1 || echo 0)"
  extract out.7z ex_ok "$CANARY"
  check "the right password extracts" "1" "$(answered '"ok":true')"
  check "and the files read back whole" "alpha|beta" "$(cat "$D/ex_ok/a.txt" 2>/dev/null)|$(cat "$D/ex_ok/b.txt" 2>/dev/null)"
  extract out.7z ex_ok "$CANARY"
  check "an extract onto an existing folder is refused, separately" "1" "$(answered 'already exists')"
  # A zip is bsdtar's to extract plainly, so only the probe and the password path reach 7z for it.
  extract pw.zip zx_none
  check "a password zip asks for its password before any extract" "1" "$(answered '"err":"encrypted"')"
  extract pw.zip zx_wrong "$WRONG"
  check "a wrong password on a zip answers wrong-password" "1" "$(answered '"err":"wrong-password"')"
  extract pw.zip zx_ok "$CANARY"
  check "the right password extracts a zip" "1" "$(answered '"ok":true')"
  check "and the zip's file reads back whole" "alpha" "$(cat "$D/zx_ok/a.txt" 2>/dev/null)"
  extract src/locked.7z ex_locked "$CANARY"
  check "a file that is no archive fails without asking" "1" "$(answered '"ok":false')"
  check "and a password on it never loops the card" "0" "$(printf '%s\n' "$ANSWER" | grep -c -e '"err":"encrypted"' -e '"err":"wrong-password"')"
  extract out.7z ex_empty ""
  check "an empty password asks like no password at all" "1" "$(answered '"err":"encrypted"')"
  # 7z reads one stdin line, so a password with a line break is refused before it can lock an archive with half of it.
  compress broken.7z 7z 'pre\npost' true a.txt
  check "a password with a line break is refused naming the archive" "1" "$(answered 'the password for broken.7z cannot hold a line break')"
  check "and no archive is written" "0" "$([ -e "$D/broken.7z" ] && echo 1 || echo 0)"

  # The big job outlives a few samples, so a live 7z child is read while it works.
  rm -f "$D/sample.log" "$D/sample.stop"
  sample_until_stopped &
  SAMPLER=$!
  compress big.7z 7z "$CANARY" true big.bin
  touch "$D/sample.stop"; wait "$SAMPLER" 2>/dev/null
  check "the big password compress finished" "1" "$(answered '"ok":true')"
  check "a live 7z child was read" "1" "$([ "$(count_in ' 0$' "$D/sample.log")" -ge 1 ] && echo 1 || echo 0)"
  check "no live 7z child carried the secret in argv or environ" "0" "$(grep -c -v -e ' 0$' -e unreadable "$D/sample.log")"
  # The extract gets its own sampler window on the PPMd archive, which outlives many samples, so a -p leak there is read live too.
  rm -f "$D/sample.log" "$D/sample.stop"
  sample_until_stopped &
  SAMPLER=$!
  extract slow.7z ex_slow "$CANARY"
  touch "$D/sample.stop"; wait "$SAMPLER" 2>/dev/null
  check "the slow password extract finished" "1" "$(answered '"ok":true')"
  check "a live 7z child was read on extract" "1" "$([ "$(count_in ' 0$' "$D/sample.log")" -ge 1 ] && echo 1 || echo 0)"
  check "no sampled extract child carried the secret in argv or environ" "0" "$(grep -c -v -e ' 0$' -e unreadable "$D/sample.log")"

  stop_backend
  check "every job answered" "$dones" "$(count_in '"t":"archivedone"' "$D/out")"
  check "the wire never repeats the secret" "0" "$(grep -c -- "$CANARY" "$D/out" | tr -d ' ')"
  check "stderr never repeats the secret" "0" "$(grep -c -- "$CANARY" "$D/err" | tr -d ' ')"
  check "no file anywhere under the sandbox root holds the secret" "0" "$(grep -r -l -a -D skip -- "$CANARY" "$D" 2>/dev/null | wc -l | tr -d ' ')"
}
missing=""
for tool in 7z bwrap bsdtar; do command -v "$tool" >/dev/null 2>&1 || missing="${missing:+$missing, }$tool"; done
echo "--- phase A: the live backend never repeats the secret ---"
if [ -n "$missing" ]; then
  echo "FAIL archive-threat.sh phase A needs $missing, which is not installed"; fail=1
else
  phase_a
fi

echo "--- phase B: the offscreen card walk clears every terminal path ---"
command -v qs >/dev/null || { echo "archive-threat.sh: qs is not installed, cannot walk the cards"; exit 1; }
mkdir -p "$D/config" "$D/home" || exit 1
ln -s "$PWD/ui" "$D/config/flea" || exit 1
headless_commons "$D" || exit 1
ln -s "$(readlink -f ui/boot/Ui)" "$D/config/Ui" || exit 1
cp tests/archive-threat.qml "$D/config/shell.qml" || exit 1
run_timeout_s=60
output=$( ( env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
    HOME="$D/home" XDG_STATE_HOME="$D" XDG_CACHE_HOME="$D" \
    XDG_DATA_HOME="$D" XDG_CONFIG_HOME="$D/home/.config" \
    XDG_RUNTIME_DIR="$D" FLEA_BIN="$PWD/target/debug/flea" \
    QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_QUICK_BACKEND=software QT_FORCE_STDERR_LOGGING=1 \
    timeout "$run_timeout_s" qs -p "$D/config" 2>&1 ) 2>/dev/null )
# Sample input: "  INFO qml: ARCHIVETHREAT ok <check name>" once per check, and "  INFO qml: ARCHIVETHREAT DONE checks=42 failed=0" once.
printf '%s\n' "$output" | grep -q "ARCHIVETHREAT DONE checks=$EXPECTED_CHECKS failed=0" || { echo "FAIL the walk did not report $EXPECTED_CHECKS checks and 0 failed:"; printf '%s\n' "$output" | grep ARCHIVETHREAT | grep -v 'ARCHIVETHREAT ok ' | head -20; fail=1; }
check "the walk passed $EXPECTED_CHECKS checks by name" "$EXPECTED_CHECKS" "$(printf '%s\n' "$output" | grep -c 'ARCHIVETHREAT ok ')"
check "no probe log repeats the secret" "0" "$(printf '%s\n' "$output" | grep -c -- "$CANARY")"

echo "--- phase C: the static shape ---"
# IPC may read the cards' open state for a capture, never a dialog itself or any text typed into one.
check "Ipc.qml names neither password dialog" "0" "$(grep -ci 'archivepassworddialog\|archivecompressdialog' ui/Ipc.qml || true)"
check "the cards' IPC seam is the one state reader" "1" "$(grep -c 'function archiveCardsState(): string' ui/Ipc.qml)"
check "and it reads no secret field" "0" "$(sed -n '/function archiveCardsState/,/^        }$/p' ui/Ipc.qml | grep -ciE '\.(text|secret|_secret|password|passphrase)\b' || true)"
check "the cards' capture state reads a field's length, never its text" "0" "$(sed -n '/function captureState/,/^    }$/p' ui/ArchivePasswordDialog.qml ui/ArchiveCompressDialog.qml | grep -oiE '\.(text|secret|_secret|password|passphrase)\b(\.length)?' | grep -vc '\.length$' || true)"
check "and both cards expose one" "2" "$(cat ui/ArchivePasswordDialog.qml ui/ArchiveCompressDialog.qml | grep -c 'function captureState()')"
check "the host builds on first open only" "1" "$(grep -c 'Qt.createComponent("ArchiveCards.qml")' ui/WindowBody.qml)"
check "and nothing instantiates it at startup" "0" "$(count_in 'Flea\.ArchiveCards {' ui/WindowBody.qml ui/ArchiveCards.qml)"
check "no new surface logs" "0" "$(count_in 'console\.log' ui/ArchiveCompressDialog.qml ui/ArchivePasswordDialog.qml ui/ArchiveCards.qml ui/js/Archive.js ui/js/ArchiveOps.js ui/js/Kinds.js)"
check "no unkeyed extract slot survives anywhere in the ui" "0" "$(grep -r -h -E 'extractP(w|assword)Source' ui | wc -l | tr -d ' ')"
check "phase B exports the backend seam" "1" "$(count_in 'FLEA_BIN="\$PWD/target/debug/flea"' tests/archive-threat.sh)"

echo
if [ "$fail" = 0 ]; then echo "archive-threat.sh: all checks passed"; else echo "archive-threat.sh: FAILURES above"; fi
exit "$fail"
