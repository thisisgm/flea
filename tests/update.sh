#!/bin/bash
# Drives flea --update against stub pacman, checkupdates, vercmp and Omarchy's presenter, plus a curl that only records; nothing here touches a network or a package.
set -u
# Hard rule 9's guard, which owns FIXTURE_ROOT and every create and delete below.
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1

BIN=./target/debug/flea
[ -x "$BIN" ] || { echo "update.sh: $BIN is missing, run cargo build" >&2; exit 1; }
# current_exe() answers with the kernel's resolved path, which is what the stub pacman is asked to own.
BIN_REAL=$(readlink -f "$BIN")
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

# The presenter is spawned and not waited for, so the suite waits for its own line, bounded at ten seconds.
wait_for_line() {
  local file="$1" pattern="$2" waited=0
  while [ "$waited" -lt 200 ]; do
    grep -q "$pattern" "$file" 2>/dev/null && return 0
    waited=$((waited + 1))
    sleep 0.05
  done
  return 1
}

D=$(sandbox_root_ok && mktemp -d "$SANDBOX_ROOT/flea-update-test-XXXXXX") || exit 1
sandbox_require "$D"
: > "$D/$SANDBOX_MARKER"
mkdir -p "$D/bin" "$D/empty" "$D/home" "$D/data" "$D/config" "$D/cache"
calls="$D/calls.log"
ran="$D/presenter.log"

# Each stub records its argv, locale and exact inherited environment, then answers from the case's STUB_* variables.
stub() {
  local name="$1" body="$2"
  printf '#!/bin/sh\nLC_ALL=C.UTF-8 LANGUAGE= /usr/bin/sort -z /proc/$$/environ > %q\nexact=unexpected\n/usr/bin/cmp -s %q %q && exact=exact\nprintf "%%s %%s LC_ALL=%%s LANGUAGE=%%s NARGS=%%s ENV=%%s\\n" %q "$*" "${LC_ALL-unset}" "${LANGUAGE-unset}" "$#" "$exact" >> %q\n%s\n' \
    "$D/child.env" "$D/expected.env" "$D/child.env" "$name" "$calls" "$body" > "$D/bin/$name"
  chmod +x "$D/bin/$name"
}

# Sample input: pacman -Qqo /src/target/debug/flea, then pacman -Qi flea.
stub pacman '
case "$1" in
  -Qqo) [ -n "${STUB_OWNER-}" ] || exit 1; printf "%s\n" "$STUB_OWNER" ;;
  -Qi) printf "Name            : %s\nVersion         : %s\nValidated By    : %s\n" "$2" "$STUB_INSTALLED" "$STUB_VALIDATED" ;;
  *) exit 1 ;;
esac'
stub checkupdates 'printf "%b" "${STUB_UPDATES-}"; exit "${STUB_UPDATES_CODE:-2}"'
# No check asks the network directly any more, so curl only records the call and fails, which every called curl count below would see.
stub curl 'exit 1'
stub vercmp 'printf "%s\n" "${STUB_ORDER:-0}"'

# Only the stub directory is on PATH, so a program this suite forgot to stub fails to start rather than reaching a mirror.
run_check() {
  : > "$calls"
  env -i HOME="$D/home" XDG_DATA_HOME="$D/data" XDG_CONFIG_HOME="$D/config" XDG_CACHE_HOME="$D/cache" PATH="$D/bin" "$@" LC_ALL=C.UTF-8 /usr/bin/env -0 \
    | LC_ALL=C.UTF-8 LANGUAGE= sort -z > "$D/expected.env"
  out=$(env -i HOME="$D/home" XDG_DATA_HOME="$D/data" XDG_CONFIG_HOME="$D/config" XDG_CACHE_HOME="$D/cache" PATH="$D/bin" LC_ALL=de_DE.UTF-8 LANGUAGE=de:ar "$@" "$BIN" --update check 2>"$D/stderr")
  rc=$?
  err=$(cat "$D/stderr")
  check "every parser child received only the expected environment" "0" "$(grep -c ' ENV=unexpected$' "$calls")"
}

called() { grep -c "^$1 " "$calls"; }

# A binary no package owns is a cargo build: nothing is asked of any source.
run_check
check "an unowned binary prints unchecked unowned" "unchecked unowned - -" "$out"
check "and exits with nothing to install" "3" "$rc"
check "and asked pacman who owns the running binary" "1" "$(grep -Fxc "pacman -Qqo $BIN_REAL LC_ALL=C.UTF-8 LANGUAGE=unset NARGS=2 ENV=exact" "$calls")"
check "and asked no mirror and ran no curl" "0|0" "$(called checkupdates)|$(called curl)"

# A local makepkg build carries OPR's name and no signature.
run_check STUB_OWNER=flea STUB_INSTALLED=0.3.3-1 STUB_VALIDATED=None
check "a local build prints unchecked local with its version" "unchecked local 0.3.3-1 -" "$out"
check "and exits with nothing to install" "3" "$rc"
check "and pacman described it in the C.UTF-8 locale" "1" "$(grep -Fxc 'pacman -Qi flea LC_ALL=C.UTF-8 LANGUAGE=unset NARGS=2 ENV=exact' "$calls")"
check "and asked no source" "0|0" "$(called checkupdates)|$(called curl)"

# flea-bin is the prebuilt release, and other names are local; signed or not, none asks a release source.
for owner in flea-bin flea-git flea-custom; do
  kind=local
  if [ "$owner" = flea-bin ]; then kind=release; fi
  for validated in None Signature; do
    run_check STUB_OWNER="$owner" STUB_INSTALLED=0.3.3-1 STUB_VALIDATED="$validated"
    check "$owner ($validated) is $kind" "unchecked $kind 0.3.3-1 -" "$out"
    check "and exits with nothing to install" "3" "$rc"
    check "and asked no source or comparison" "0|0|0" "$(called checkupdates)|$(called curl)|$(called vercmp)"
    check "and has no failure sentence" "" "$err"
  done
done

run_check STUB_OWNER=flea-git STUB_INSTALLED=0.3.1.r0.g6433131-2 STUB_VALIDATED=None
check "a describe-style local version is not compared" "unchecked local - -" "$out"
check "and asked no source" "0|0" "$(called checkupdates)|$(called curl)"

# OPR's signed package asks checkupdates, which syncs its own database copy and is never given --nosync.
opr=(STUB_OWNER=flea STUB_INSTALLED=0.3.2-1 STUB_VALIDATED=Signature)
run_check "${opr[@]}" STUB_UPDATES_CODE=0 STUB_ORDER=1 \
  STUB_UPDATES='linux 6.16.8.arch1-1 -> 6.16.9.arch1-1\nflea 0.3.2-1 -> 0.3.3-1\n'
check "an OPR update prints available with both versions" "available opr 0.3.2-1 0.3.3-1" "$out"
check "and exits 0" "0" "$rc"
check "checkupdates ran with no arguments" "1" "$(grep -Fxc 'checkupdates  LC_ALL=C.UTF-8 LANGUAGE=unset NARGS=0 ENV=exact' "$calls")"
check "vercmp was asked whether the offer is newer than the installed" "1" "$(grep -Fxc 'vercmp 0.3.3-1 0.3.2-1 LC_ALL=C.UTF-8 LANGUAGE=unset NARGS=2 ENV=exact' "$calls")"
check "and never ran curl" "0" "$(called curl)"

run_check "${opr[@]}" STUB_UPDATES_CODE=0 STUB_UPDATES='linux 6.16.8.arch1-1 -> 6.16.9.arch1-1\n'
check "other packages' updates leave OPR Flea current" "current opr 0.3.2-1 -" "$out"
check "and exit with nothing to install" "3" "$rc"

run_check "${opr[@]}" STUB_UPDATES_CODE=2
check "checkupdates' no-updates status is current" "current opr 0.3.2-1 -" "$out"
check "and needs no vercmp" "0" "$(called vercmp)"

run_check "${opr[@]}" STUB_UPDATES_CODE=1
check "checkupdates' error status prints failed" "failed opr 0.3.2-1 -" "$out"
check "and exits 2" "2" "$rc"
check "with one sentence naming the mirrors" "flea: the package mirrors could not be asked for a newer Flea" "$err"

run_check "${opr[@]}" STUB_UPDATES_CODE=0 STUB_ORDER=0 STUB_UPDATES='flea 0.3.2-1 -> 0.3.2-1\n'
check "the installed version is current" "current opr 0.3.2-1 0.3.2-1" "$out"
run_check "${opr[@]}" STUB_UPDATES_CODE=0 STUB_ORDER=-1 STUB_UPDATES='flea 0.3.2-1 -> 0.3.1-1\n'
check "an older mirror is current, never an offer to downgrade" "current opr 0.3.2-1 0.3.1-1" "$out"
check "and exits with nothing to install" "3" "$rc"

run_check "${opr[@]}" STUB_UPDATES_CODE=0 STUB_UPDATES='flea 0.3.2-1 -> 0.3.4-1$(reboot)\n'
check "a version that fails the strict pattern prints failed" "failed opr 0.3.2-1 -" "$out"
check "and never reaches vercmp" "0" "$(called vercmp)"
check "and names the version problem" "flea: the package source answered with a version this check cannot compare" "$err"

# The two usage shapes a malformed --update takes, on an empty PATH so a parse that ran either could launch nothing real.
out=$(env -i HOME="$D/home" XDG_DATA_HOME="$D/data" XDG_CONFIG_HOME="$D/config" XDG_CACHE_HOME="$D/cache" PATH="$D/empty" "$BIN" --update now 2>&1 >/dev/null); rc=$?
check "--update with an unknown word is a usage error" "2" "$rc"
check "and says what it takes" "1" "$(echo "$out" | grep -c 'update takes nothing, or check')"
out=$(env -i HOME="$D/home" XDG_DATA_HOME="$D/data" XDG_CONFIG_HOME="$D/config" XDG_CACHE_HOME="$D/cache" PATH="$D/empty" "$BIN" --update check twice 2>&1 >/dev/null); rc=$?
check "--update check with a trailing word is a usage error" "2" "$rc"
check "and says what it takes, which a check that ran and failed would not" "1" "$(echo "$out" | grep -c 'update takes nothing, or check')"

# Named from src/update.rs, the same derivation tests/modes.sh uses, so a renamed presenter cannot fall through to a real one.
# Sample input: src/update.rs `Command::new("omarchy-launch-floating-terminal-with-presentation")`.
presenter=$(grep -ho 'Command::new("[a-z0-9-]\+")' src/update.rs | cut -d'"' -f2 | sort -u)
case "$presenter" in
  ''|*[!a-z0-9-]*) echo "FAIL update: src/update.rs must name one presenter; got '$presenter'"; sandbox_remove "$D"; exit 1 ;;
esac
check "the presenter is Omarchy's own floating terminal launcher" "omarchy-launch-floating-terminal-with-presentation" "$presenter"

# Sample input: omarchy-launch-floating-terminal-with-presentation omarchy-update
{
  printf '#!/bin/sh\n'
  printf 'printf "FD1 %%s\\n" "$(/usr/bin/readlink /proc/$$/fd/1)" >> %q\n' "$ran"
  printf 'printf "FD2 %%s\\n" "$(/usr/bin/readlink /proc/$$/fd/2)" >> %q\n' "$ran"
  printf 'exec >> %q 2>&1\n' "$ran"
  printf 'printf "NARGS %%s\\n" "$#"\n'
  printf 'printf "ARGV %%s\\n" "$*"\n'
  printf 'P=$(/usr/bin/cut -d" " -f5 /proc/self/stat)\n'
  printf '[ "$$" = "$P" ] && printf "PGID MATCH\\n" || printf "PGID MISMATCH\\n"\n'
  printf '/usr/bin/grep -i "^THP_enabled" /proc/self/status\n'
} > "$D/bin/$presenter"
chmod +x "$D/bin/$presenter"

: > "$ran"
# Quickshell hands flea --update a pipe and closes it, so a pipe is what the updater must not inherit.
env -i HOME="$D/home" XDG_DATA_HOME="$D/data" XDG_CONFIG_HOME="$D/config" XDG_CACHE_HOME="$D/cache" PATH="$D/bin" "$BIN" --update 2>&1 | cat >/dev/null
check "--update returns success once the presenter is started" "0" "${PIPESTATUS[0]}"
wait_for_line "$ran" '^THP_enabled'
out=$(cat "$ran")
check "the presenter is handed omarchy-update and nothing else" "ARGV omarchy-update|NARGS 1" \
  "$(echo "$out" | grep '^ARGV ')|$(echo "$out" | grep '^NARGS ')"
check "the updater got no inherited pipe, on stdout or stderr" "1|1" \
  "$(echo "$out" | grep -c '^FD1 /dev/null$')|$(echo "$out" | grep -c '^FD2 /dev/null$')"
check "the updater leads its own process group" "1" "$(echo "$out" | grep -c '^PGID MATCH$')"
check "the updater runs with huge pages on" "1" "$(echo "$out" | grep -c '^THP_enabled:[[:space:]]*1')"

# The window inherits huge pages off, so the updater it starts must get them back.
: > "$ran"
mkdir -p "$D/gui"
cp "$BIN_REAL" "$D/gui/flea"
printf '#!/bin/sh\n/usr/bin/grep -i "^THP_enabled" /proc/self/status | /usr/bin/sed "s/^/GUI /" >> %q\nexec %q --update\n' "$ran" "$BIN_REAL" > "$D/gui/flea-gui"
chmod +x "$D/gui/flea-gui"
env -i HOME="$D/home" XDG_DATA_HOME="$D/data" XDG_CONFIG_HOME="$D/config" XDG_CACHE_HOME="$D/cache" XDG_STATE_HOME="$D/home/state" WAYLAND_DISPLAY=flea-update-test-display PATH="$D/bin" \
  "$D/gui/flea" --gui >"$D/gui.log" 2>&1 </dev/null
wait_for_line "$ran" '^THP_enabled'
check "the window inherited huge pages off" "1" "$(grep -c '^GUI THP_enabled:[[:space:]]*0' "$ran")"
check "and the updater it started got them back" "1" "$(grep -c '^THP_enabled:[[:space:]]*1' "$ran")"

out=$(env -i HOME="$D/home" XDG_DATA_HOME="$D/data" XDG_CONFIG_HOME="$D/config" XDG_CACHE_HOME="$D/cache" PATH="$D/empty" "$BIN" --update 2>&1); rc=$?
check "a box with no presenter is the failure status" "2" "$rc"
check "and one sentence saying nothing was updated" "flea: Omarchy's updater could not be started, so nothing was updated" "$out"

# The presenter ships in the omarchy package, which PKGBUILD must depend on or the row opens nothing.
check "PKGBUILD depends on omarchy, which ships $presenter" "1" "$( (. ./PKGBUILD && printf '%s\n' "${depends[@]}") | grep -cxF omarchy)"

sandbox_remove "$D"
[ "$fail" -eq 0 ] && echo "update: all checks passed"
exit "$fail"
