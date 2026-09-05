#!/bin/bash
# Drives the real binary and asserts the mode contract, since main() is only reachable here.
set -u
# Hard rule 9's guard, which owns FIXTURE_ROOT and every create and delete below.
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1

BIN=./target/debug/flea
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

# A handoff that spawns returns before its child has written anything, so the suite waits for the
# child's own line instead of for a fixed time. The 0.2 s guess it replaced survived 20 runs under
# twenty-four spinners here and failed 2 of 2 once the child's first write was delayed by 0.4 s, and
# every failure that run recorded was a check reading a log the child had not written to yet. The wait is
# bounded and gives up silently: the check that follows reads the same log and fails on the missing line.
wait_for_line() {
  local file="$1" pattern="$2" waited=0
  while [ "$waited" -lt 200 ]; do
    grep -q "$pattern" "$file" 2>/dev/null && return 0
    waited=$((waited + 1))
    sleep 0.05
  done
  return 1
}

# --version answers before any other mode, prints bare, and agrees with the crate it was built
# from, so a stale binary beside a bumped Cargo.toml cannot report the new number.
want=$(grep -m1 '^version = ' Cargo.toml | cut -d'"' -f2)
out=$($BIN --version 2>&1 </dev/null); rc=$?
check "--version exits 0" "0" "$rc"
check "--version prints the crate version" "$want" "$out"
out=$($BIN --version --gui 2>&1 </dev/null)
check "--version wins over another mode" "$want" "$out"

# No tty on either handle and no display: it must refuse, not guess.
out=$(env -u WAYLAND_DISPLAY -u DISPLAY $BIN --gui 2>&1 </dev/null)
check "no display refuses" "1" "$(echo "$out" | grep -c 'no graphical session')"

# An exported-but-empty display is absent and must not reach qs.
out=$(env WAYLAND_DISPLAY= DISPLAY= PATH=/nonexistent-flea-test-path $BIN --gui 2>&1 </dev/null)
check "an empty display refuses" "1" "$(echo "$out" | grep -c 'no graphical session')"
out=$(env -u DISPLAY WAYLAND_DISPLAY= PATH=/nonexistent-flea-test-path $BIN --gui 2>&1 </dev/null)
check "an empty WAYLAND_DISPLAY alone refuses" "1" "$(echo "$out" | grep -c 'no graphical session')"
out=$(env -u WAYLAND_DISPLAY DISPLAY= PATH=/nonexistent-flea-test-path $BIN --gui 2>&1 </dev/null)
check "an empty DISPLAY alone refuses" "1" "$(echo "$out" | grep -c 'no graphical session')"

# Each display variable independently permits launch, and an empty peer must not mask it.
out=$(env -u DISPLAY WAYLAND_DISPLAY=flea-modes-test-display PATH=/nonexistent-flea-test-path $BIN --gui 2>&1 </dev/null)
check "a non-empty WAYLAND_DISPLAY is accepted" "1" "$(echo "$out" | grep -c 'could not start the shell')"
out=$(env -u WAYLAND_DISPLAY DISPLAY=:99 PATH=/nonexistent-flea-test-path $BIN --gui 2>&1 </dev/null)
check "a non-empty DISPLAY is accepted" "1" "$(echo "$out" | grep -c 'could not start the shell')"
out=$(env WAYLAND_DISPLAY= DISPLAY=:99 PATH=/nonexistent-flea-test-path $BIN --gui 2>&1 </dev/null)
check "an empty WAYLAND_DISPLAY does not mask DISPLAY" "1" "$(echo "$out" | grep -c 'could not start the shell')"
out=$(env WAYLAND_DISPLAY=flea-modes-test-display DISPLAY= PATH=/nonexistent-flea-test-path $BIN --gui 2>&1 </dev/null)
check "an empty DISPLAY does not mask WAYLAND_DISPLAY" "1" "$(echo "$out" | grep -c 'could not start the shell')"
out=$(env WAYLAND_DISPLAY=flea-modes-test-display DISPLAY=:99 PATH=/nonexistent-flea-test-path $BIN --gui 2>&1 </dev/null)
check "two non-empty displays are accepted" "1" "$(echo "$out" | grep -c 'could not start the shell')"

# qs missing from PATH is what a bad launcher or .desktop install hits; no errno may leak.
out=$(env WAYLAND_DISPLAY=flea-modes-test-display PATH=/nonexistent-flea-test-path $BIN --gui 2>&1 </dev/null)
check "missing qs is elided" "1" "$(echo "$out" | grep -c 'could not start the shell')"
check "missing qs carries no errno" "0" "$(echo "$out" | grep -c 'os error')"

# With no flag and no tty the default must be the window branch, which is what a .desktop launch takes.
out=$(env -u WAYLAND_DISPLAY -u DISPLAY $BIN . 2>&1 </dev/null)
check "no flag defaults to the window" "1" "$(echo "$out" | grep -c 'no graphical session')"

# A shell PTY proves bare flea still means the product that exists, not the reserved terminal interface.
out=$(env -u WAYLAND_DISPLAY -u DISPLAY script -qec "$BIN ." /dev/null 2>&1)
check "no flag at a terminal defaults to the window" "1" "$(echo "$out" | grep -c 'no graphical session')"

# --tui with no tty must refuse rather than write escape codes into a pipe.
out=$($BIN --tui 2>&1 </dev/null | cat)
check "--tui without a tty refuses" "1" "$(echo "$out" | grep -c 'needs a terminal')"

# Mutually exclusive flags are a usage error; the message is checked too, see AGENTS.md Modes.
out=$($BIN --tui --gui 2>&1 >/dev/null </dev/null)
rc=$?
check "--tui --gui is a usage error" "2" "$rc"
check "--tui --gui names the conflict" "1" "$(echo "$out" | grep -c 'mutually exclusive')"

# The prctl has to survive exec, so a stub qs reports the kernel's own view of the launched child.
D="$FIXTURE_ROOT/flea-thp-test-$$"
sandbox_make "$D"
printf '#!/bin/sh\ngrep -i "^THP_enabled" /proc/self/status\n' > "$D/qs"
chmod +x "$D/qs"
out=$(env WAYLAND_DISPLAY=flea-modes-test-display PATH="$D:/usr/bin:/bin" $BIN --gui 2>&1 </dev/null)
check "the launched shell has transparent huge pages off" "1" \
  "$(echo "$out" | grep -c 'THP_enabled:[[:space:]]*0')"
check "the launched shell reported its THP state at all" "1" "$(echo "$out" | grep -c 'THP_enabled')"
sandbox_remove "$D"

# Sample input: let finished = Command::new("gio")
# Each mode's stub is named from that mode's own exec target, because a stub named by hand goes stale
# the day the target is renamed, and the run then resolves the operator's real launcher instead: that
# is what left three editors running on this box.
# Not the qs stubs: a renamed qs reaches real quickshell, which their dead WAYLAND_DISPLAY kills with nothing left behind.
handoff_in() {
  grep -ho 'Command::new("[a-z0-9-]\+")' "$1" | cut -d'"' -f2 | sort -u
}
open_handoff=$(handoff_in src/open.rs)
terminal_handoff=$(handoff_in src/terminal.rs)
# Fail closed rather than name a stub from an empty or two-name derivation: that stub is one nothing
# calls, which is the fall-through this exists to prevent, and a check would report it after the fact.
for derived in "$open_handoff" "$terminal_handoff"; do
  case "$derived" in
    ''|*[!a-z0-9-]*)
      echo "FAIL modes: src/open.rs and src/terminal.rs must each name one handoff; got '$derived'"
      exit 1 ;;
  esac
done

# --open resolves the target, refuses a directory, and hands anything else to gio open, which is
# the route that reads the desktop database and so honours Terminal=true; see "Opening a file".
D="$FIXTURE_ROOT/flea-open-test-$$"
sandbox_make "$D"
mkdir -p "$D/dir" "$D/bin" "$D/failbin"
printf 'hello' > "$D/file.txt"
ln -s "$D/file.txt" "$D/linkfile"
ln -s "$D/dir" "$D/linkdir"
ln -s "$D/nowhere" "$D/broken"
# The two names an argv bug shows up on, as real files, so what is checked is what the child was
# handed rather than what a quoting rule promises.
printf 'hello' > "$D/-dash.txt"
newline_name=$(printf 'two\nlines.txt')
printf 'hello' > "$D/$newline_name"
# Its stdio is detached, so everything it has to say goes to this log rather than to our stdout.
opened="$D/opened.log"
# The last argument on its own, because a name with a newline in it cannot be read back off a line.
last_arg="$D/last-arg"
# Sample input: gio open /home/flea-sandbox/flea-open-test-123/file.txt
# No strip-to-paren here: cut reads its OWN stat, comm is bare "cut", and its pgid is the stub's by fork.
{
  printf '#!/bin/sh\n'
  printf 'printf "FD1 %%s\\n" "$(readlink /proc/$$/fd/1)" >> %q\n' "$opened"
  printf 'exec >> %q 2>&1\n' "$opened"
  printf 'printf "PID %%s\\n" "$$"\n'
  printf 'printf "NARGS %%s\\n" "$#"\n'
  printf 'printf "ARGV %%s\\n" "$*"\n'
  printf 'shift $(($# - 1)); printf "%%s" "$1" > %q\n' "$last_arg"
  printf 'P=$(cut -d" " -f5 /proc/self/stat)\n'
  printf '[ "$$" = "$P" ] && printf "PGID MATCH pid=%%s pgid=%%s\\n" "$$" "$P" || printf "PGID MISMATCH pid=%%s pgid=%%s\\n" "$$" "$P"\n'
  printf 'grep -i "^THP_enabled" /proc/self/status\n'
} > "$D/bin/$open_handoff"
chmod +x "$D/bin/$open_handoff"
# The same handoff, refusing. gio open answers nonzero when it cannot reach a handler, and --open
# has to report that rather than the 0 a fire-and-forget spawn reports whatever happens next.
printf '#!/bin/sh\nexit 3\n' > "$D/failbin/$open_handoff"
chmod +x "$D/failbin/$open_handoff"

: > "$opened"
# Quickshell hands flea --open a pipe and closes it, so a pipe is exactly what the handler must not inherit.
PATH="$D/bin:/usr/bin:/bin" $BIN --open "$D/file.txt" 2>&1 | cat >/dev/null
# THP_enabled is the stub's last line, so waiting for it is waiting for the whole record.
wait_for_line "$opened" '^THP_enabled'
out=$(cat "$opened")
check "--open hands the file to gio open" "1" "$(echo "$out" | grep -c "^ARGV open $D/file.txt$")"
check "and gio is given the subcommand and the path and nothing else" "1" "$(echo "$out" | grep -c '^NARGS 2$')"
# A pipe here dies with the flea that made it, and the handler dies with it on its first write.
check "the opened program got no inherited pipe" "1" "$(echo "$out" | grep -c '^FD1 /dev/null$')"
check "and the stub reported its first descriptor at all" "1" "$(echo "$out" | grep -c '^FD1 ')"
# Field five of /proc/self/stat is the process group; it equals the pid only after setpgid(0, 0).
check "the opened program leads its own process group" "1" "$(echo "$out" | grep -c '^PGID MATCH')"
check "and the stub reported its process group at all" "1" "$(echo "$out" | grep -c '^PGID ')"
# Nothing disabled huge pages in this process, so 1 is the untouched state and a stray disable would show.
check "a plain --open leaves huge pages on" "1" "$(echo "$out" | grep -c '^THP_enabled:[[:space:]]*1')"
check "and the stub reported its THP state at all" "1" "$(echo "$out" | grep -c 'THP_enabled')"
# --open waits for the launcher, so by the time it returns the launcher has been reaped; a pid that
# is still signalable is a gio left running for the life of the application it started.
launcher_pid=$(echo "$out" | sed -n 's/^PID //p' | head -1)
check "the launcher reported a pid at all" "1" "$([ -n "$launcher_pid" ] && echo 1 || echo 0)"
check "and --open left no launcher behind" "1" "$(kill -0 "$launcher_pid" 2>/dev/null && echo 0 || echo 1)"

: > "$opened"
PATH="$D/bin:/usr/bin:/bin" $BIN --open "$D/linkfile" >/dev/null 2>&1
wait_for_line "$opened" '^THP_enabled'
check "a symlink to a file is resolved to its target" "1" "$(grep -c "^ARGV open $D/file.txt$" "$opened")"

# A leading dash and an embedded newline both survive canonicalization as one absolute argument.
: > "$opened"; : > "$last_arg"
PATH="$D/bin:/usr/bin:/bin" $BIN --open "$D/-dash.txt" >/dev/null 2>&1
wait_for_line "$opened" '^THP_enabled'
check "a name starting with a dash is handed over absolute, so it is never read as a flag" \
  "$D/-dash.txt" "$(cat "$last_arg")"
check "and it is still exactly two arguments" "1" "$(grep -c '^NARGS 2$' "$opened")"
: > "$opened"; : > "$last_arg"
PATH="$D/bin:/usr/bin:/bin" $BIN --open "$D/$newline_name" >/dev/null 2>&1
wait_for_line "$opened" '^THP_enabled'
check "a name with a newline in it arrives whole and unsplit" \
  "$D/$newline_name" "$(cat "$last_arg")"
check "and it is still exactly two arguments too" "1" "$(grep -c '^NARGS 2$' "$opened")"

PATH="$D/bin:/usr/bin:/bin" $BIN --open "$D/dir" >/dev/null 2>&1
check "a directory is refused with its own status" "3" "$?"
PATH="$D/bin:/usr/bin:/bin" $BIN --open "$D/linkdir" >/dev/null 2>&1
check "and so is a symlink to one" "3" "$?"

out=$(PATH="$D/bin:/usr/bin:/bin" $BIN --open "$D/broken" 2>&1)
rc=$?
check "a broken symlink is an error status" "2" "$rc"
check "and one sentence, with no errno" "0" "$(echo "$out" | grep -c 'os error')"
check "and that sentence names the file" "1" "$(echo "$out" | grep -c 'could not be opened')"

out=$(env PATH=/nonexistent-flea-test-path $BIN --open "$D/file.txt" 2>&1)
rc=$?
check "a missing gio is an error status" "2" "$rc"
check "and is elided too" "0" "$(echo "$out" | grep -c 'os error')"
# Without this the pair cannot tell a failed spawn from a --open that was never implemented.
check "and that sentence names the handler" "1" "$(echo "$out" | grep -c 'nothing on this system could be asked')"

# The launcher's own refusal. A spawn that is never waited on reports 0 here, which is what put a
# green status on an open that never happened.
out=$(env PATH="$D/failbin:/usr/bin:/bin" $BIN --open "$D/file.txt" 2>&1)
rc=$?
check "a launcher that refuses is an error status, not a green handoff" "2" "$rc"
check "and its refusal is elided to one sentence" "1" "$(echo "$out" | grep -c 'could not be opened')"
check "with no errno in it" "0" "$(echo "$out" | grep -c 'os error')"

out=$($BIN --open 2>&1 </dev/null)
check "--open with no path is a usage error" "1" "$(echo "$out" | grep -c -- '--open')"

# Every program the openers hand off to by name must be shipped by a PKGBUILD dependency, or the
# package installs and the button it belongs to does nothing at all. The table is here rather than
# from pacman so the check runs off the box too, and a handoff with no row in it is itself a failure.
handoff_package() {
  case "$1" in
    gio) printf 'glib2' ;;
    xdg-terminal-exec) printf 'xdg-terminal-exec' ;;
    *) printf '' ;;
  esac
}
handoffs=$(printf '%s\n%s\n' "$open_handoff" "$terminal_handoff" | sort -u)
# The denominator, because a derived loop over nothing reports green having checked nothing: a
# renamed file or a handoff name this grep cannot match would otherwise pass in silence.
check "the two openers hand off to two programs by name" "2" "$(printf '%s\n' "$handoffs" | grep -c .)"
for handoff in $handoffs; do
  package=$(handoff_package "$handoff")
  check "$handoff is a handoff this suite knows the package for" "1" "$([ -n "$package" ] && echo 1 || echo 0)"
  check "PKGBUILD depends on $package, which ships $handoff" "1" "$(grep -c "^depends=.*'$package'" PKGBUILD)"
done

# The stub qs is what exec_qs launched, so it inherits huge pages off; --open must hand them back.
: > "$last_arg"
printf '#!/bin/sh\ngrep -i "^THP_enabled" /proc/self/status | sed "s/^/QS /"\nexec %s --open %s\n' "$PWD/$BIN" "$D/file.txt" > "$D/bin/qs"
chmod +x "$D/bin/qs"
: > "$opened"
out=$(env WAYLAND_DISPLAY=flea-modes-test-display PATH="$D/bin:/usr/bin:/bin" $BIN --gui 2>&1 </dev/null)
# src/open.rs:43 waits for the launcher, so the stub's record is whole when the chain returns and this wait returns at once.
wait_for_line "$opened" '^THP_enabled'
check "the shell inherited huge pages off" "1" "$(echo "$out" | grep -c '^QS THP_enabled:[[:space:]]*0')"
check "and the opened program got them back" "1" "$(grep -c '^THP_enabled:[[:space:]]*1' "$opened")"
sandbox_remove "$D"

# --terminal resolves the directory, refuses anything that is not one, and hands the canonical path
# to xdg-terminal-exec as one --dir= argument. src/terminal.rs is its own copy of the stdio, process
# group and huge page guards --open carries, so each one is pinned here rather than assumed to have
# travelled with the code; see "Opening a file".
D="$FIXTURE_ROOT/flea-terminal-test-$$"
sandbox_make "$D"
mkdir -p "$D/dir" "$D/bin"
printf 'hello' > "$D/file.txt"
ln -s "$D/dir" "$D/linkdir"
ln -s "$D/nowhere" "$D/broken"
# Its stdio is detached, so everything the terminal has to say goes to this log, not to our stdout.
ran="$D/ran.log"
# Sample input: xdg-terminal-exec --dir=/home/flea-sandbox/flea-terminal-test-123/dir
# No strip-to-paren here: cut reads its OWN stat, comm is bare "cut", and its pgid is the stub's by fork.
{
  printf '#!/bin/sh\n'
  printf 'printf "FD1 %%s\\n" "$(readlink /proc/$$/fd/1)" >> %q\n' "$ran"
  printf 'exec >> %q 2>&1\n' "$ran"
  printf 'printf "NARGS %%s\\n" "$#"\n'
  printf 'printf "ARGV %%s\\n" "$*"\n'
  printf 'P=$(cut -d" " -f5 /proc/self/stat)\n'
  printf '[ "$$" = "$P" ] && printf "PGID MATCH pid=%%s pgid=%%s\\n" "$$" "$P" || printf "PGID MISMATCH pid=%%s pgid=%%s\\n" "$$" "$P"\n'
  printf 'grep -i "^THP_enabled" /proc/self/status\n'
} > "$D/bin/$terminal_handoff"
chmod +x "$D/bin/$terminal_handoff"

: > "$ran"
# Quickshell hands flea --terminal a pipe and closes it, so a pipe is what the terminal must not inherit.
PATH="$D/bin:/usr/bin:/bin" $BIN --terminal "$D/dir" 2>&1 | cat >/dev/null
# --terminal spawns and returns without waiting: against a stub that slept half a second before its
# first write it returned with the log still empty, so every line below arrives after it has exited.
wait_for_line "$ran" '^THP_enabled'
out=$(cat "$ran")
check "--terminal hands the directory to xdg-terminal-exec" "1" "$(echo "$out" | grep -c -- "^ARGV --dir=$D/dir$")"
check "and it is given that one argument and nothing else" "1" "$(echo "$out" | grep -c '^NARGS 1$')"
# A pipe here dies with the flea that made it, and the terminal dies with it on its first write.
check "the terminal got no inherited pipe" "1" "$(echo "$out" | grep -c '^FD1 /dev/null$')"
check "and the stub reported its first descriptor at all" "1" "$(echo "$out" | grep -c '^FD1 ')"
# Field five of /proc/self/stat is the process group; it equals the pid only after setpgid(0, 0).
check "the terminal leads its own process group" "1" "$(echo "$out" | grep -c '^PGID MATCH')"
check "and the stub reported its process group at all" "1" "$(echo "$out" | grep -c '^PGID ')"
# Nothing disabled huge pages in this process, so 1 is the untouched state and a stray disable would show.
check "a plain --terminal leaves huge pages on" "1" "$(echo "$out" | grep -c '^THP_enabled:[[:space:]]*1')"
check "and the stub reported its THP state at all" "1" "$(echo "$out" | grep -c 'THP_enabled')"

: > "$ran"
PATH="$D/bin:/usr/bin:/bin" $BIN --terminal "$D/linkdir" >/dev/null 2>&1
wait_for_line "$ran" '^THP_enabled'
check "a symlink to a directory is resolved to its target" "1" "$(grep -c -- "^ARGV --dir=$D/dir$" "$ran")"

: > "$ran"
out=$(PATH="$D/bin:/usr/bin:/bin" $BIN --terminal "$D/file.txt" 2>&1)
rc=$?
check "a file is refused with the failure status" "2" "$rc"
check "and that sentence names the directory, which is the thing that was not one" "1" \
  "$(echo "$out" | grep -c 'that directory could not be opened')"
# Best effort: a spawned stub may not have written yet, so the two checks above are the strict ones.
check "and no terminal was started over it" "0" "$(grep -c '^ARGV ' "$ran")"

out=$(PATH="$D/bin:/usr/bin:/bin" $BIN --terminal "$D/broken" 2>&1)
rc=$?
check "a path that resolves to nothing is an error status" "2" "$rc"
check "and one sentence, with no errno" "0" "$(echo "$out" | grep -c 'os error')"
check "and that sentence names the directory too" "1" "$(echo "$out" | grep -c 'that directory could not be opened')"

out=$(env PATH=/nonexistent-flea-test-path $BIN --terminal "$D/dir" 2>&1)
rc=$?
check "a missing xdg-terminal-exec is an error status" "2" "$rc"
check "and is elided too" "0" "$(echo "$out" | grep -c 'os error')"
# Without this the pair cannot tell a failed spawn from a --terminal that was never implemented.
check "and that sentence names the handler" "1" "$(echo "$out" | grep -c 'nothing on this system could be asked')"

out=$($BIN --terminal 2>&1 </dev/null)
check "--terminal with no path is a usage error" "1" "$(echo "$out" | grep -c -- '--terminal')"

# The stub qs is what exec_qs launched, so it inherits huge pages off; --terminal must hand them
# back. This is the arm with teeth: the plain call above runs with them already on.
printf '#!/bin/sh\nexec %s --terminal %s\n' "$PWD/$BIN" "$D/dir" > "$D/bin/qs"
chmod +x "$D/bin/qs"
: > "$ran"
env WAYLAND_DISPLAY=flea-modes-test-display PATH="$D/bin:/usr/bin:/bin" $BIN --gui >/dev/null 2>&1 </dev/null
# The sandbox is removed below and src/terminal.rs:40 is a spawn, so this wait is what keeps the stub
# from being deleted out from under the chain that is still starting it.
wait_for_line "$ran" '^THP_enabled'
check "the terminal got its huge pages back through the shell" "1" "$(grep -c '^THP_enabled:[[:space:]]*1' "$ran")"
sandbox_remove "$D"

# Issue 41. A handler declaring Terminal=true has to be run inside a terminal or it maps no window
# at all, and which programs need one is the desktop database's judgement, never Flea's. This drives
# the real gio against an isolated XDG_DATA_HOME and XDG_CONFIG_HOME, so the operator's own MIME
# state is neither read nor written, and a stub xdg-terminal-exec records whether it was reached.
T="$FIXTURE_ROOT/flea-terminal-entry-$$"
sandbox_make "$T"
mkdir -p "$T/data/applications" "$T/config" "$T/bin"
terminal_log="$T/ran.log"
{ printf '#!/bin/sh\n'; printf 'printf "HANDLER %%s\\n" "$*" >> %q\n' "$terminal_log"; } > "$T/bin/flea-t41-handler"
chmod +x "$T/bin/flea-t41-handler"
# What glib runs a Terminal=true entry inside; glib names this one, not src/terminal.rs, so it is
# not derived, and it records the call and then runs the command itself.
{ printf '#!/bin/sh\n'; printf 'printf "TERMINAL %%s\\n" "$*" >> %q\n' "$terminal_log"; printf 'exec "$@"\n'; } > "$T/bin/xdg-terminal-exec"
chmod +x "$T/bin/xdg-terminal-exec"
{
  printf '[Desktop Entry]\n'
  printf 'Type=Application\n'
  printf 'Name=Flea issue 41 handler\n'
  printf 'Exec=flea-t41-handler %%f\n'
  printf 'Terminal=true\n'
  printf 'NoDisplay=true\n'
  printf 'MimeType=text/plain;\n'
} > "$T/data/applications/flea-t41.desktop"
printf '[Default Applications]\ntext/plain=flea-t41.desktop\n' > "$T/config/mimeapps.list"
printf 'hello\n' > "$T/note.txt"
# Built if the tool is here and skipped if it is not; the checks below read the run, not this.
update-desktop-database "$T/data/applications" >/dev/null 2>&1
: > "$terminal_log"
env XDG_DATA_HOME="$T/data" XDG_CONFIG_HOME="$T/config" XDG_DATA_DIRS=/usr/share \
  PATH="$T/bin:/usr/bin:/bin" $BIN --open "$T/note.txt" >/dev/null 2>&1
rc=$?
# The handler runs inside the terminal wrapper, so its own line arrives after gio has been reaped.
wait_for_line "$terminal_log" '^HANDLER '
check "a Terminal=true handler is reached at all" "1" "$(grep -c '^HANDLER ' "$terminal_log")"
check "and it is run inside a terminal, which is the window the operator never saw" "1" \
  "$(grep -c '^TERMINAL ' "$terminal_log")"
check "and --open reports the launcher's own success" "0" "$rc"
sandbox_remove "$T"

# The existing modes must not have moved.
out=$(printf '{"c":"quit"}\n' | $BIN --backend)
check "--backend still runs" "0" "$?"

D="$FIXTURE_ROOT/flea-modes-test-$$"
sandbox_make "$D"
mkdir -p "$D"; : > "$D/a.txt"
$BIN --prewarm "$D" 1 "$D/out.json" >/dev/null 2>&1
check "--prewarm still runs" "0" "$?"
sandbox_remove "$D"

# A bad second argument to --default is a usage error, not a silent no-op.
out=$($BIN --default bogus 2>&1 >/dev/null </dev/null)
rc=$?
check "--default bogus is a usage error" "2" "$rc"
check "and names the accepted shape" "1" "$(echo "$out" | grep -c -- '--default takes nothing, or off')"

# With no desktop entry installed, --default must refuse and touch nothing: pointing
# xdg-mime or Hyprland's bindings at an uninstalled binary would be a claim on nothing.
D="$FIXTURE_ROOT/flea-default-missing-test-$$"
sandbox_make "$D"
mkdir -p "$D/data" "$D/config/hypr"
printf -- '-- stock omarchy bindings\n' > "$D/config/hypr/bindings.lua"
out=$(env XDG_DATA_HOME="$D/data" XDG_DATA_DIRS="$D/no-such-data-dir" XDG_CONFIG_HOME="$D/config" \
  $BIN --default 2>&1)
rc=$?
check "--default refuses when its desktop entry is not installed" "1" "$rc"
check "and names what is missing" "1" "$(echo "$out" | grep -c 'is not installed')"
check "and mimeapps.list is never created" "0" "$([ -e "$D/config/mimeapps.list" ] && echo 1 || echo 0)"
check "and bindings.lua is left untouched" "-- stock omarchy bindings" "$(cat "$D/config/hypr/bindings.lua")"
sandbox_remove "$D"

# An unknown flag is a usage error naming the flag, never a silent fallthrough.
out=$($BIN --nonsense 2>&1 </dev/null)
check "unknown flag names itself" "1" "$(echo "$out" | grep -c -- '--nonsense')"

# --select takes a file and opens its directory; --print-target is test-only, resolving the pair without a window.
out=$($BIN --select "file:///etc/hostname" --print-target 2>&1)
check "select: a file uri resolves to its parent and target" "/etc /etc/hostname" "$out"

out=$($BIN --select "/etc/hostname" --print-target 2>&1)
check "select: a bare path is accepted too" "/etc /etc/hostname" "$out"

out=$($BIN --select "file:///etc/does-not-exist" --print-target 2>&1)
check "select: a missing target still opens its directory" "/etc /etc/does-not-exist" "$out"

out=$($BIN --select "file:///home/gm/My%20Files/note.txt" --print-target 2>&1)
check "select: a percent-encoded uri decodes" "/home/gm/My Files /home/gm/My Files/note.txt" "$out"

[ "$fail" -eq 0 ] && echo "modes: all checks passed"
exit $fail
