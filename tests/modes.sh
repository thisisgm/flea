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

# The prctl and renderer choice have to survive exec, so a stub qs reports the launched child.
D="$FIXTURE_ROOT/flea-thp-test-$$"
sandbox_make "$D"
# One stub reports everything the launch has to carry across exec: the kernel's own view of huge
# pages, the binary the fallback would relaunch, the renderer chosen for it, and the opened target.
cat > "$D/qs" <<'STUB'
#!/bin/sh
grep -i "^THP_enabled" /proc/self/status
printf 'FLEA_BIN %s\n' "$FLEA_BIN"
printf 'RENDERER %s\n' "$QSG_RHI_BACKEND"
printf 'AUTOMATIC %s\n' "${FLEA_RENDERER_AUTOMATIC-unset}"
printf 'ARGV %s\n' "$*"
printf 'FLEA_PATH %s\n' "${FLEA_PATH-unset}"
printf 'FLEA_SELECT %s\n' "${FLEA_SELECT-unset}"
STUB
chmod +x "$D/qs"
# The loader variables are scrubbed because this suite sets them itself below: an operator shell
# already exporting a broken pair would redden the positive control as if the launcher were wrong.
out=$(env -u QSG_RHI_BACKEND -u FLEA_RENDERER_AUTOMATIC -u VK_DRIVER_FILES -u VK_ICD_FILENAMES FLEA_BIN=stale WAYLAND_DISPLAY=flea-modes-test-display PATH="$D:/usr/bin:/bin" $BIN --gui 2>&1 </dev/null)
check "the launched shell has transparent huge pages off" "1" \
  "$(echo "$out" | grep -c 'THP_enabled:[[:space:]]*0')"
check "the launched shell reported its THP state at all" "1" "$(echo "$out" | grep -c 'THP_enabled')"
check "the launched shell uses this Flea binary" "FLEA_BIN $PWD/target/debug/flea" \
  "$(echo "$out" | grep '^FLEA_BIN ')"
check "the automatic renderer starts with Vulkan" "1" "$(echo "$out" | grep -c '^RENDERER vulkan$')"
check "the automatic renderer permits one fallback" "1" "$(echo "$out" | grep -c '^AUTOMATIC 1$')"
out=$(env QSG_RHI_BACKEND=opengl FLEA_RENDERER_AUTOMATIC=stale WAYLAND_DISPLAY=flea-modes-test-display PATH="$D:/usr/bin:/bin" $BIN --gui 2>&1 </dev/null)
check "an explicit renderer is preserved" "1" "$(echo "$out" | grep -c '^RENDERER opengl$')"
check "an explicit renderer cannot trigger fallback" "1" "$(echo "$out" | grep -c '^AUTOMATIC unset$')"
# An exported-but-empty renderer is what a wrapper script's unset variable produces, and the tree
# already rules that shape absent for WAYLAND_DISPLAY and DISPLAY above.
out=$(env -u FLEA_RENDERER_AUTOMATIC -u VK_DRIVER_FILES -u VK_ICD_FILENAMES QSG_RHI_BACKEND= WAYLAND_DISPLAY=flea-modes-test-display PATH="$D:/usr/bin:/bin" $BIN --gui 2>&1 </dev/null)
check "an empty renderer is absent, not a choice" "1" "$(echo "$out" | grep -c '^RENDERER vulkan$')"
check "and an empty renderer still permits the one fallback" "1" "$(echo "$out" | grep -c '^AUTOMATIC 1$')"

# Issue #14: a loader that cannot deliver an instance SIGSEGVs Quickshell inside QRhi::create before
# any scene-graph error can be raised, so the launcher must not name Vulkan to a shell it will crash.
out=$(env VK_DRIVER_FILES=/nonexistent-flea-icd VK_ICD_FILENAMES=/nonexistent-flea-icd \
  WAYLAND_DISPLAY=flea-modes-test-display PATH="$D:/usr/bin:/bin" $BIN --gui 2>&1 </dev/null)
check "an unusable Vulkan loader launches the shell on OpenGL" "1" "$(echo "$out" | grep -c '^RENDERER opengl$')"
check "and OpenGL is marked as final, since it has nowhere left to fall" "1" "$(echo "$out" | grep -c '^AUTOMATIC unset$')"

# The operator's own choice is not a guess to be corrected, even when the loader cannot honour it.
out=$(env VK_DRIVER_FILES=/nonexistent-flea-icd VK_ICD_FILENAMES=/nonexistent-flea-icd \
  QSG_RHI_BACKEND=vulkan WAYLAND_DISPLAY=flea-modes-test-display PATH="$D:/usr/bin:/bin" $BIN --gui 2>&1 </dev/null)
check "an explicit Vulkan survives an unusable loader" "1" "$(echo "$out" | grep -c '^RENDERER vulkan$')"
check "and an explicit choice still marks no fallback" "1" "$(echo "$out" | grep -c '^AUTOMATIC unset$')"

# Nothing but the renderer may differ between the two arms, so both are launched on the same target.
good=$(env -u VK_DRIVER_FILES -u VK_ICD_FILENAMES WAYLAND_DISPLAY=flea-modes-test-display PATH="$D:/usr/bin:/bin" \
  $BIN --gui --select /etc/hostname 2>&1 </dev/null)
broken=$(env VK_DRIVER_FILES=/nonexistent-flea-icd VK_ICD_FILENAMES=/nonexistent-flea-icd \
  WAYLAND_DISPLAY=flea-modes-test-display PATH="$D:/usr/bin:/bin" \
  $BIN --gui --select /etc/hostname 2>&1 </dev/null)
check "the working arm opens the selected file's directory" "FLEA_PATH /etc" "$(echo "$good" | grep '^FLEA_PATH ')"
check "the working arm selects the file itself" "FLEA_SELECT /etc/hostname" "$(echo "$good" | grep '^FLEA_SELECT ')"
check "the fallback arm opens the same directory" "$(echo "$good" | grep '^FLEA_PATH ')" "$(echo "$broken" | grep '^FLEA_PATH ')"
check "the fallback arm selects the same file" "$(echo "$good" | grep '^FLEA_SELECT ')" "$(echo "$broken" | grep '^FLEA_SELECT ')"
check "the fallback arm passes the same UI root" "$(echo "$good" | grep '^ARGV ')" "$(echo "$broken" | grep '^ARGV ')"
check "and the fallback arm is the one that changed renderer" "1" "$(echo "$broken" | grep -c '^RENDERER opengl$')"

# A launch with no FLEA_BIN in the environment is the ordinary one, and it must still name this binary.
out=$(env -u FLEA_BIN WAYLAND_DISPLAY=flea-modes-test-display PATH="$D:/usr/bin:/bin" $BIN --gui 2>&1 </dev/null)
check "an unset FLEA_BIN is derived from the running binary" "FLEA_BIN $PWD/target/debug/flea" \
  "$(echo "$out" | grep '^FLEA_BIN ')"

sandbox_remove "$D"

# --open resolves the target, refuses a directory, and hands anything else to xdg-open.
D="$FIXTURE_ROOT/flea-open-test-$$"
sandbox_make "$D"
mkdir -p "$D/dir" "$D/bin"
printf 'hello' > "$D/file.txt"
ln -s "$D/file.txt" "$D/linkfile"
ln -s "$D/dir" "$D/linkdir"
ln -s "$D/nowhere" "$D/broken"
# Its stdio is detached, so everything it has to say goes to this log rather than to our stdout.
opened="$D/opened.log"
# No strip-to-paren here: cut reads its OWN stat, comm is bare "cut", and its pgid is the stub's by fork.
printf '#!/bin/sh\nprintf "FD1 %%s\\n" "$(readlink /proc/$$/fd/1)" >> %q\nexec >> %q 2>&1\nprintf "ARGV %%s\\n" "$@"\nP=$(cut -d" " -f5 /proc/self/stat)\n[ "$$" = "$P" ] && printf "PGID MATCH pid=%%s pgid=%%s\\n" "$$" "$P" || printf "PGID MISMATCH pid=%%s pgid=%%s\\n" "$$" "$P"\ngrep -i "^THP_enabled" /proc/self/status\n' "$opened" "$opened" > "$D/bin/xdg-open"
chmod +x "$D/bin/xdg-open"

: > "$opened"
# Quickshell hands flea --open a pipe and closes it, so a pipe is exactly what the handler must not inherit.
PATH="$D/bin:/usr/bin:/bin" $BIN --open "$D/file.txt" 2>&1 | cat >/dev/null; sleep 0.2
out=$(cat "$opened")
check "--open hands the file to xdg-open" "1" "$(echo "$out" | grep -c "^ARGV $D/file.txt$")"
# A pipe here dies with the flea that made it, and the handler dies with it on its first write.
check "the opened program got no inherited pipe" "1" "$(echo "$out" | grep -c '^FD1 /dev/null$')"
check "and the stub reported its first descriptor at all" "1" "$(echo "$out" | grep -c '^FD1 ')"
# Field five of /proc/self/stat is the process group; it equals the pid only after setpgid(0, 0).
check "the opened program leads its own process group" "1" "$(echo "$out" | grep -c '^PGID MATCH')"
check "and the stub reported its process group at all" "1" "$(echo "$out" | grep -c '^PGID ')"
# Nothing disabled huge pages in this process, so 1 is the untouched state and a stray disable would show.
check "a plain --open leaves huge pages on" "1" "$(echo "$out" | grep -c '^THP_enabled:[[:space:]]*1')"
check "and the stub reported its THP state at all" "1" "$(echo "$out" | grep -c 'THP_enabled')"

: > "$opened"
PATH="$D/bin:/usr/bin:/bin" $BIN --open "$D/linkfile" >/dev/null 2>&1; sleep 0.2
check "a symlink to a file is resolved to its target" "1" "$(grep -c "^ARGV $D/file.txt$" "$opened")"

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
check "a missing xdg-open is an error status" "2" "$rc"
check "and is elided too" "0" "$(echo "$out" | grep -c 'os error')"
# Without this the pair cannot tell a failed spawn from a --open that was never implemented.
check "and that sentence names the handler" "1" "$(echo "$out" | grep -c 'nothing on this system could be asked')"

out=$($BIN --open 2>&1 </dev/null)
check "--open with no path is a usage error" "1" "$(echo "$out" | grep -c -- '--open')"

# The stub qs is what exec_qs launched, so it inherits huge pages off; --open must hand them back.
printf '#!/bin/sh\ngrep -i "^THP_enabled" /proc/self/status | sed "s/^/QS /"\nexec %s --open %s\n' "$PWD/$BIN" "$D/file.txt" > "$D/bin/qs"
chmod +x "$D/bin/qs"
: > "$opened"
out=$(env WAYLAND_DISPLAY=flea-modes-test-display PATH="$D/bin:/usr/bin:/bin" $BIN --gui 2>&1 </dev/null; sleep 0.2)
check "the shell inherited huge pages off" "1" "$(echo "$out" | grep -c '^QS THP_enabled:[[:space:]]*0')"
check "and the opened program got them back" "1" "$(grep -c '^THP_enabled:[[:space:]]*1' "$opened")"
sandbox_remove "$D"

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
