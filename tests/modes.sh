#!/bin/bash
# Drives the real binary and asserts the mode contract, since main() is only reachable here.
set -u
# Hard rule 9's guard, which owns FIXTURE_ROOT and every create and delete below.
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1

BIN=./target/debug/flea
# Without this every case below drives a missing binary and reports the result as a product failure.
[ -x "$BIN" ] || { echo "modes.sh: $BIN is missing, run cargo build" >&2; exit 1; }
# current_exe() answers with the kernel's own resolved path, so the expectation is resolved the same way.
BIN_REAL=$(readlink -f "$BIN")
# Named, not re-derived from ui_dir's own walk, which an installed /usr/share/flea/ui outranks.
UI_REAL=$(readlink -f .)/ui
# An operator exporting any of these would answer for src/gui.rs, which is the thing under test here.
unset QSG_RHI_BACKEND FLEA_RENDERER_AUTOMATIC QT_VK_PHYSICAL_DEVICE_INDEX VK_DRIVER_FILES VK_ICD_FILENAMES QS_ICON_THEME FLEA_QT_THEME \
  FLEA_PREFETCH FLEA_PREFETCH_SHELL
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
# Trailing arguments are a usage error, the shape --default and --picker already take: a mode that
# ignored them would answer for a command line nobody wrote. It is still read before every other
# mode, so the refusal is what a second flag gets rather than that flag's own behaviour.
out=$($BIN --version --gui 2>&1 >/dev/null </dev/null); rc=$?
check "--version with a trailing flag is a usage error" "2" "$rc"
check "--version with a trailing flag says it takes nothing" "1" "$(echo "$out" | grep -c 'version takes nothing')"
out=$(env -u WAYLAND_DISPLAY -u DISPLAY $BIN --gui --version 2>&1 >/dev/null </dev/null); rc=$?
check "--version after another flag is refused too" "2" "$rc"
check "--version is still read before the other modes" "1" "$(echo "$out" | grep -c 'version takes nothing')"

# --pick refuses before any window and writes no reply, and an exported-but-empty request is absent
# the way an empty display is: a wrapper's unset variable must not open a chooser with no request.
D="$FIXTURE_ROOT/flea-pick-test-$$"
sandbox_make "$D"
out=$(env -u FLEA_PICKER $BIN --pick "$D/reply.json" 2>&1 </dev/null); rc=$?
check "--pick with no request refuses" "2" "$rc"
check "--pick with no request names FLEA_PICKER" "1" "$(echo "$out" | grep -c 'needs FLEA_PICKER')"
out=$(env FLEA_PICKER= $BIN --pick "$D/reply.json" 2>&1 </dev/null); rc=$?
check "--pick with an empty request refuses" "2" "$rc"
check "--pick with an empty request gives the same sentence" "1" "$(echo "$out" | grep -c 'needs FLEA_PICKER')"
# The request guard is read before the display one, so a headless run cannot pass this case by
# refusing for the other reason: with a display, that is the branch that would open a window.
check "--pick with an empty request refuses on the request, not the display" "0" "$(echo "$out" | grep -c 'no graphical session')"
check "a refused pick wrote no reply file" "0" "$(ls -A "$D" | grep -c '^reply.json$')"
sandbox_remove "$D"

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

# A shell PTY proves bare flea selects the window branch; only --tui reaches the terminal interface.
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
# One stub reports everything the launch has to carry across exec, huge pages and target included.
cat > "$D/qs" <<'STUB'
#!/bin/sh
grep -i "^THP_enabled" /proc/self/status
printf 'FLEA_BIN %s\n' "$FLEA_BIN"
printf 'RENDERER %s\n' "$QSG_RHI_BACKEND"
printf 'AUTOMATIC %s\n' "${FLEA_RENDERER_AUTOMATIC-unset}"
printf 'ICD %s\n' "${VK_ICD_FILENAMES-unset}"
printf 'DRIVER_FILES %s\n' "${VK_DRIVER_FILES-unset}"
printf 'ARGV %s\n' "$*"
printf 'FLEA_PATH %s\n' "${FLEA_PATH-unset}"
printf 'FLEA_SELECT %s\n' "${FLEA_SELECT-unset}"
printf 'PLATFORM_THEME %s\n' "${QT_QPA_PLATFORMTHEME-unset}"
printf 'PREFETCH %s\n' "${FLEA_PREFETCH-unset}"
printf 'PREFETCH_SHELL %s %s\n' "${FLEA_PREFETCH_SHELL-unset}" "$$"
printf 'ICON_THEME %s\n' "${QS_ICON_THEME-unset}"
printf 'THEME_MARKER %s\n' "${FLEA_QT_THEME-unset}"
STUB
chmod +x "$D/qs"
out=$(env FLEA_BIN=stale FLEA_UI="$UI_REAL" WAYLAND_DISPLAY=flea-modes-test-display PATH="$D:/usr/bin:/bin" $BIN --gui 2>&1 </dev/null)
check "the launched shell has transparent huge pages off" "1" \
  "$(echo "$out" | grep -c 'THP_enabled:[[:space:]]*0')"
check "the launched shell reported its THP state at all" "1" "$(echo "$out" | grep -c 'THP_enabled')"
# The entry is ui/boot/shell.qml and not the ui directory, because a document imports its own.
check "the launch targets the boot entry" "ARGV -p $UI_REAL/boot/shell.qml" "$(echo "$out" | grep '^ARGV ')"
# 24003ab made an explicit FLEA_BIN the operator's choice, the rule FLEA_UI and QSG_RHI_BACKEND
# already follow, and this check was left asserting the behaviour that commit replaced. The launch
# above sets one deliberately, so the operator's own value is what must reach the shell; the unset
# case further down is the one that derives the running binary.
check "an explicit FLEA_BIN is the operator's, and reaches the shell" "FLEA_BIN stale" \
  "$(echo "$out" | grep '^FLEA_BIN ')"
check "the automatic renderer starts with Vulkan" "1" "$(echo "$out" | grep -c '^RENDERER vulkan$')"
check "the automatic renderer permits one fallback" "1" "$(echo "$out" | grep -c '^AUTOMATIC 1$')"
# The downgrade below says why, so its silence here is what proves this arm took the probe's other branch.
check "a loader that can deliver Vulkan says nothing" "0" "$(echo "$out" | grep -c 'Vulkan is unusable')"
# A DRM card directory has no dash in its name; card1-DP-1 is one of its connectors.
card_count=0
for card in /sys/class/drm/card[0-9]*; do
  # An unmatched glob arrives as its own literal, which is not a card and must not be counted.
  [ -e "$card" ] || continue
  case "${card##*/}" in *-*) continue ;; esac
  card_count=$((card_count + 1))
done
# corner: a box with one DRM card cannot be hybrid, so the pin must stay silent on it.
if [ "$card_count" = 1 ]; then
  check "a single-GPU box leaves the loader's ICD list alone" "1" "$(echo "$out" | grep -c '^ICD unset$')"
  check "a single-GPU box leaves the driver file list alone" "1" "$(echo "$out" | grep -c '^DRIVER_FILES unset$')"
  check "a single-GPU box announces no display-GPU pin" "0" "$(echo "$out" | grep -c 'GPU with no display')"
else
  # corner: a multi-card box cannot be judged from here, so only the pin and the variables are held to agree.
  if echo "$out" | grep -q 'GPU with no display'; then
    check "an announced pin names an ICD file" "1" "$(echo "$out" | grep -c '^ICD /.*\.json')"
    check "and an announced pin names a driver file" "1" "$(echo "$out" | grep -c '^DRIVER_FILES /.*\.json')"
  else
    check "an unannounced pin leaves the loader's ICD list alone" "1" "$(echo "$out" | grep -c '^ICD unset$')"
    check "and leaves the driver file list alone" "1" "$(echo "$out" | grep -c '^DRIVER_FILES unset$')"
  fi
fi
out=$(env VK_ICD_FILENAMES=/tmp/flea-operator-icd.json WAYLAND_DISPLAY=flea-modes-test-display PATH="$D:/usr/bin:/bin" $BIN --gui 2>&1 </dev/null)
check "an explicit ICD list is preserved" "1" "$(echo "$out" | grep -c '^ICD /tmp/flea-operator-icd.json$')"
check "and an explicit ICD list is not announced as a pin" "0" "$(echo "$out" | grep -c 'GPU with no display')"
out=$(env VK_DRIVER_FILES=/tmp/flea-operator-driver.json WAYLAND_DISPLAY=flea-modes-test-display PATH="$D:/usr/bin:/bin" $BIN --gui 2>&1 </dev/null)
check "an explicit driver file list is preserved" "1" "$(echo "$out" | grep -c '^DRIVER_FILES /tmp/flea-operator-driver.json$')"
check "and an explicit driver file list is not announced as a pin" "0" "$(echo "$out" | grep -c 'GPU with no display')"
out=$(env QSG_RHI_BACKEND=opengl FLEA_RENDERER_AUTOMATIC=stale WAYLAND_DISPLAY=flea-modes-test-display PATH="$D:/usr/bin:/bin" $BIN --gui 2>&1 </dev/null)
check "an explicit renderer is preserved" "1" "$(echo "$out" | grep -c '^RENDERER opengl$')"
check "an explicit renderer cannot trigger fallback" "1" "$(echo "$out" | grep -c '^AUTOMATIC unset$')"
# An exported-but-empty renderer is a wrapper script's unset variable, absent as WAYLAND_DISPLAY is.
out=$(env QSG_RHI_BACKEND= WAYLAND_DISPLAY=flea-modes-test-display PATH="$D:/usr/bin:/bin" $BIN --gui 2>&1 </dev/null)
check "an empty renderer is absent, not a choice" "1" "$(echo "$out" | grep -c '^RENDERER vulkan$')"
check "and an empty renderer still permits the one fallback" "1" "$(echo "$out" | grep -c '^AUTOMATIC 1$')"

# Issue #14: a loader that cannot build an instance kills the shell before it can raise a scene-graph error.
out=$(env VK_DRIVER_FILES=/nonexistent-flea-icd VK_ICD_FILENAMES=/nonexistent-flea-icd \
  WAYLAND_DISPLAY=flea-modes-test-display PATH="$D:/usr/bin:/bin" $BIN --gui 2>&1 </dev/null)
check "an unusable Vulkan loader launches the shell on OpenGL" "1" "$(echo "$out" | grep -c '^RENDERER opengl$')"
check "and OpenGL is marked as final, since it has nowhere left to fall" "1" "$(echo "$out" | grep -c '^AUTOMATIC unset$')"
# Only the probe's own branch prints this, so the pair above cannot come from an explicit renderer.
check "and the operator is told which call refused" "1" \
  "$(echo "$out" | grep -c 'flea: Vulkan is unusable, vkCreateInstance answered ')"
check "and the sentence names the extensions it asked for" "1" "$(echo "$out" | grep -c 'VK_KHR_surface')"
check "and it is said once, not dumped" "1" "$(echo "$out" | grep -c 'Vulkan is unusable')"

# The operator's own choice is not a guess to be corrected, even when the loader cannot honour it.
out=$(env VK_DRIVER_FILES=/nonexistent-flea-icd VK_ICD_FILENAMES=/nonexistent-flea-icd \
  QSG_RHI_BACKEND=vulkan WAYLAND_DISPLAY=flea-modes-test-display PATH="$D:/usr/bin:/bin" $BIN --gui 2>&1 </dev/null)
check "an explicit Vulkan survives an unusable loader" "1" "$(echo "$out" | grep -c '^RENDERER vulkan$')"
check "and an explicit choice still marks no fallback" "1" "$(echo "$out" | grep -c '^AUTOMATIC unset$')"
check "and the probe never ran, so nothing was said about it" "0" "$(echo "$out" | grep -c 'Vulkan is unusable')"

# Nothing but the renderer may differ between the two arms, so both are launched on the same target.
good=$(env WAYLAND_DISPLAY=flea-modes-test-display PATH="$D:/usr/bin:/bin" \
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
check "and it is the only arm that reported a downgrade" "0" "$(echo "$good" | grep -c 'Vulkan is unusable')"


# A launch with no FLEA_BIN in the environment is the ordinary one, and it must still name this binary.
out=$(env -u FLEA_BIN WAYLAND_DISPLAY=flea-modes-test-display PATH="$D:/usr/bin:/bin" $BIN --gui 2>&1 </dev/null)
check "an unset FLEA_BIN is derived from the running binary" "FLEA_BIN $BIN_REAL" \
  "$(echo "$out" | grep '^FLEA_BIN ')"

# The icon theme name is the one thing Flea took from gtk3, and Omarchy writes it here too.
theme_home="$D/home"
mkdir -p "$theme_home/.local/state/omarchy/current/theme"
printf 'Yaru-blue\n' > "$theme_home/.local/state/omarchy/current/theme/icons.theme"
out=$(env -u XDG_CACHE_HOME HOME="$theme_home" QT_QPA_PLATFORMTHEME=gtk3 WAYLAND_DISPLAY=flea-modes-test-display \
  PATH="$D:/usr/bin:/bin" $BIN --gui 2>&1 </dev/null)
check "the icon theme name reaches the shell" "ICON_THEME Yaru-blue" "$(echo "$out" | grep '^ICON_THEME ')"
check "the prefetch list is named for the backend" "PREFETCH $theme_home/.cache/flea/prefetch" "$(echo "$out" | grep '^PREFETCH ')"
# Sample line: "PREFETCH_SHELL 4242 4242", the pid the launcher named and the pid the stub runs as.
own=$(echo "$out" | grep '^PREFETCH_SHELL ' | cut -d' ' -f3)
check "and the shell is named by the pid exec kept" "PREFETCH_SHELL $own $own" "$(echo "$out" | grep '^PREFETCH_SHELL ')"
check "and gtk3 does not" "PLATFORM_THEME unset" "$(echo "$out" | grep '^PLATFORM_THEME ')"

out=$(env HOME="$theme_home" XDG_CACHE_HOME="$D/xdg-cache" WAYLAND_DISPLAY=flea-modes-test-display \
  PATH="$D:/usr/bin:/bin" $BIN --gui 2>&1 </dev/null)
check "an operator's XDG_CACHE_HOME holds the prefetch list" "PREFETCH $D/xdg-cache/flea/prefetch" "$(echo "$out" | grep '^PREFETCH ')"

# An operator who named an icon theme keeps whatever platform theme they chose with it.
out=$(env HOME="$theme_home" QT_QPA_PLATFORMTHEME=gtk3 QS_ICON_THEME=Papirus \
  WAYLAND_DISPLAY=flea-modes-test-display PATH="$D:/usr/bin:/bin" $BIN --gui 2>&1 </dev/null)
check "an operator's own icon theme is untouched" "ICON_THEME Papirus" "$(echo "$out" | grep '^ICON_THEME ')"
check "and their platform theme survives with it" "PLATFORM_THEME gtk3" "$(echo "$out" | grep '^PLATFORM_THEME ')"
check "and no trade was marked over it" "THEME_MARKER unset" "$(echo "$out" | grep '^THEME_MARKER ')"

# Only gtk3 is traded. Another engine is the operator's own choice and must survive untouched.
out=$(env HOME="$theme_home" QT_QPA_PLATFORMTHEME=qt6ct WAYLAND_DISPLAY=flea-modes-test-display \
  PATH="$D:/usr/bin:/bin" $BIN --gui 2>&1 </dev/null)
check "another platform theme is the operator's and survives" "PLATFORM_THEME qt6ct" "$(echo "$out" | grep '^PLATFORM_THEME ')"
check "and no icon theme is named over it" "ICON_THEME unset" "$(echo "$out" | grep '^ICON_THEME ')"
check "and nothing is marked as traded" "THEME_MARKER unset" "$(echo "$out" | grep '^THEME_MARKER ')"

# The trade marks itself, which is what open.rs and terminal.rs read to hand the theme back.
out=$(env HOME="$theme_home" QT_QPA_PLATFORMTHEME=gtk3 WAYLAND_DISPLAY=flea-modes-test-display \
  PATH="$D:/usr/bin:/bin" $BIN --gui 2>&1 </dev/null)
check "a traded theme says what it traded" "THEME_MARKER gtk3" "$(echo "$out" | grep '^THEME_MARKER ')"

# An unreadable icons.theme is a read that failed, so the launch must be exactly today's.
chmod 000 "$theme_home/.local/state/omarchy/current/theme/icons.theme"
out=$(env HOME="$theme_home" QT_QPA_PLATFORMTHEME=gtk3 WAYLAND_DISPLAY=flea-modes-test-display \
  PATH="$D:/usr/bin:/bin" $BIN --gui 2>&1 </dev/null)
check "an unreadable icons.theme keeps the platform theme" "PLATFORM_THEME gtk3" "$(echo "$out" | grep '^PLATFORM_THEME ')"
check "and names no icon theme from it" "ICON_THEME unset" "$(echo "$out" | grep '^ICON_THEME ')"
check "and marks no trade it did not make" "THEME_MARKER unset" "$(echo "$out" | grep '^THEME_MARKER ')"
chmod 644 "$theme_home/.local/state/omarchy/current/theme/icons.theme"

# No HOME: no icons.theme, no cache for a list, and the prefetch pair a Flea terminal passes on never reaches the shell.
out=$(env -u HOME -u XDG_CACHE_HOME FLEA_PREFETCH="$D/stale-list" FLEA_PREFETCH_SHELL=1 QT_QPA_PLATFORMTHEME=gtk3 WAYLAND_DISPLAY=flea-modes-test-display \
  PATH="$D:/usr/bin:/bin" $BIN --gui 2>&1 </dev/null)
check "no HOME keeps the platform theme" "PLATFORM_THEME gtk3" "$(echo "$out" | grep '^PLATFORM_THEME ')"
check "and names no prefetch list" "PREFETCH unset" "$(echo "$out" | grep '^PREFETCH ')"
check "and no shell to record it" "1" "$(echo "$out" | grep -c '^PREFETCH_SHELL unset ')"

# A chooser started from a Flea terminal inherits both variables and must not record over the main window's list.
out=$(env FLEA_PICKER='{"stub":true}' FLEA_PREFETCH="$D/stale-list" FLEA_PREFETCH_SHELL=1 FLEA_UI="$UI_REAL" \
  WAYLAND_DISPLAY=flea-modes-test-display PATH="$D:/usr/bin:/bin" $BIN --pick "$D/reply.json" 2>&1 </dev/null)
check "a chooser drops the prefetch list" "PREFETCH unset" "$(echo "$out" | grep '^PREFETCH ')"
check "and the shell pid with it" "1" "$(echo "$out" | grep -c '^PREFETCH_SHELL unset ')"

# No icons.theme is not an invitation to guess: the launch must be exactly today's.
rm -f "$theme_home/.local/state/omarchy/current/theme/icons.theme"
out=$(env HOME="$theme_home" QT_QPA_PLATFORMTHEME=gtk3 WAYLAND_DISPLAY=flea-modes-test-display \
  PATH="$D:/usr/bin:/bin" $BIN --gui 2>&1 </dev/null)
check "a box with no icons.theme keeps its platform theme" "PLATFORM_THEME gtk3" "$(echo "$out" | grep '^PLATFORM_THEME ')"
check "and gets no icon theme of its own" "ICON_THEME unset" "$(echo "$out" | grep '^ICON_THEME ')"
check "and marks no trade it did not make" "THEME_MARKER unset" "$(echo "$out" | grep '^THEME_MARKER ')"

# An empty icons.theme is absent, the rule every other empty variable follows here.
printf '\n' > "$theme_home/.local/state/omarchy/current/theme/icons.theme"
out=$(env HOME="$theme_home" QT_QPA_PLATFORMTHEME=gtk3 WAYLAND_DISPLAY=flea-modes-test-display \
  PATH="$D:/usr/bin:/bin" $BIN --gui 2>&1 </dev/null)
check "an empty icons.theme keeps the platform theme" "PLATFORM_THEME gtk3" "$(echo "$out" | grep '^PLATFORM_THEME ')"
check "and names no icon theme" "ICON_THEME unset" "$(echo "$out" | grep '^ICON_THEME ')"
check "and marks no trade it did not make" "THEME_MARKER unset" "$(echo "$out" | grep '^THEME_MARKER ')"

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
# Sample input: copier.command = ["sh", "-c", "printf '%s' \"$1\" | wl-copy", "_", text]
# The window has a handoff of its own and it is a pipeline inside an sh -c string, not a Command::new,
# so it is derived from the pipeline's last word instead; the grep above cannot see a QML caller at all.
qml_handoff_in() {
  grep -ho '| [a-z0-9-]\+"' "$1" | cut -d' ' -f2 | tr -d '"' | sort -u
}
opener_qml_handoff=$(qml_handoff_in ui/Opener.qml)
# Fail closed rather than name a stub from an empty or two-name derivation: that stub is one nothing
# calls, which is the fall-through this exists to prevent, and a check would report it after the fact.
for derived in "$open_handoff" "$terminal_handoff" "$opener_qml_handoff"; do
  case "$derived" in
    ''|*[!a-z0-9-]*)
      echo "FAIL modes: src/open.rs, src/terminal.rs and ui/Opener.qml must each name one handoff; got '$derived'"
      exit 1 ;;
  esac
done

# --open resolves the target, refuses a directory, and hands anything else to gio open, which is
# the route that reads the desktop database and so honours Terminal=true; see "Opening a file".
D="$FIXTURE_ROOT/flea-open-test-$$"
sandbox_make "$D"
mkdir -p "$D/dir" "$D/bin" "$D/failbin" "$D/lingerbin"
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
  printf 'printf "ICD %%s\\n" "${VK_ICD_FILENAMES-unset}"\n'
  printf 'printf "DRIVER_FILES %%s\\n" "${VK_DRIVER_FILES-unset}"\n'
  printf 'printf "PIN %%s\\n" "${FLEA_VK_PIN-unset}"\n'
  printf 'printf "THEME %%s\\n" "${QT_QPA_PLATFORMTHEME-unset}"\n'
  printf 'printf "ICON_THEME %%s\\n" "${QS_ICON_THEME-unset}"\n'
  printf 'printf "THEME_MARKER %%s\\n" "${FLEA_QT_THEME-unset}"\n'
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

# That pair cannot go red on its own: the wait above is for the stub's LAST write, so the stub is
# exiting whatever --open did. gio open really does outlive its own last write while a DBusActivatable
# handler starts, measured at 0.32 to 0.75 s on this box, and this stub is that case in miniature.
# Half a second, because a --open that did not wait reaches the check below in milliseconds.
linger_s=0.5
lingered="$D/lingered.log"
printf '#!/bin/sh\nprintf "PID %%s\\n" "$$" >> %q\nsleep %s\n' "$lingered" "$linger_s" > "$D/lingerbin/$open_handoff"
chmod +x "$D/lingerbin/$open_handoff"
PATH="$D/lingerbin:/usr/bin:/bin" $BIN --open "$D/file.txt" >/dev/null 2>&1
lingering_pid=$(sed -n 's/^PID //p' "$lingered" | head -1)
check "the lingering launcher reported a pid at all" "1" "$([ -n "$lingering_pid" ] && echo 1 || echo 0)"
check "and --open waited for a launcher that outlived its own last write" "1" \
  "$([ -n "$lingering_pid" ] && ! kill -0 "$lingering_pid" 2>/dev/null && echo 1 || echo 0)"

: > "$opened"
# Flea's own pin carries a marker, and only a marked pin is taken back off a program Flea opens.
VK_ICD_FILENAMES=/tmp/flea-pin-icd.json VK_DRIVER_FILES=/tmp/flea-pin-driver.json FLEA_VK_PIN=1 \
  PATH="$D/bin:/usr/bin:/bin" $BIN --open "$D/file.txt" 2>&1 | cat >/dev/null
# The status wanted is the launcher's own, not cat's, which is 0 whatever happened upstream.
check "the marked-pin open returned success" "0" "${PIPESTATUS[0]}"
wait_for_line "$opened" '^THP_enabled'
out=$(cat "$opened")
check "a marked pin is dropped from an opened program" "1" "$(echo "$out" | grep -c '^ICD unset$')"
check "and its driver file list goes with it" "1" "$(echo "$out" | grep -c '^DRIVER_FILES unset$')"
check "and the marker itself does not leak onward" "1" "$(echo "$out" | grep -c '^PIN unset$')"

: > "$opened"
# An operator's own list carries no marker, so it must survive into the program they open.
VK_ICD_FILENAMES=/tmp/flea-operator-icd.json VK_DRIVER_FILES=/tmp/flea-operator-driver.json \
  PATH="$D/bin:/usr/bin:/bin" $BIN --open "$D/file.txt" 2>&1 | cat >/dev/null
check "the operator-list open returned success" "0" "${PIPESTATUS[0]}"
wait_for_line "$opened" '^THP_enabled'
out=$(cat "$opened")
check "an operator's own ICD list reaches the opened program" "1" "$(echo "$out" | grep -c '^ICD /tmp/flea-operator-icd.json$')"
check "and so does their own driver file list" "1" "$(echo "$out" | grep -c '^DRIVER_FILES /tmp/flea-operator-driver.json$')"

: > "$opened"
# An exported but empty marker is absent, so it must not turn an operator's own list into a pin.
VK_ICD_FILENAMES=/tmp/flea-operator-icd.json VK_DRIVER_FILES=/tmp/flea-operator-driver.json FLEA_VK_PIN= \
  PATH="$D/bin:/usr/bin:/bin" $BIN --open "$D/file.txt" 2>&1 | cat >/dev/null
check "the empty-marker open returned success" "0" "${PIPESTATUS[0]}"
wait_for_line "$opened" '^THP_enabled'
out=$(cat "$opened")
check "an empty marker leaves an operator's ICD list alone" "1" "$(echo "$out" | grep -c '^ICD /tmp/flea-operator-icd.json$')"
check "and leaves their driver file list alone" "1" "$(echo "$out" | grep -c '^DRIVER_FILES /tmp/flea-operator-driver.json$')"

: > "$opened"
# What the launcher traded for its own startup, the program it opens gets back, and nothing else.
env -u QT_QPA_PLATFORMTHEME QS_ICON_THEME=Yaru-blue FLEA_QT_THEME=gtk3 \
  PATH="$D/bin:/usr/bin:/bin" $BIN --open "$D/file.txt" 2>&1 | cat >/dev/null
check "the traded-theme open returned success" "0" "${PIPESTATUS[0]}"
wait_for_line "$opened" '^THP_enabled'
out=$(cat "$opened")
check "an opened program gets the traded platform theme back" "THEME gtk3" "$(echo "$out" | grep '^THEME ')"
check "and not the icon theme Flea named for Quickshell" "ICON_THEME unset" "$(echo "$out" | grep '^ICON_THEME ')"
check "and not the marker that said so" "THEME_MARKER unset" "$(echo "$out" | grep '^THEME_MARKER ')"

: > "$opened"
# No marker is no trade, so an operator's own platform theme reaches the program they opened.
env QT_QPA_PLATFORMTHEME=qt6ct QS_ICON_THEME=Papirus \
  PATH="$D/bin:/usr/bin:/bin" $BIN --open "$D/file.txt" 2>&1 | cat >/dev/null
check "the untraded open returned success" "0" "${PIPESTATUS[0]}"
wait_for_line "$opened" '^THP_enabled'
out=$(cat "$opened")
check "an unmarked launch leaves the platform theme alone" "THEME qt6ct" "$(echo "$out" | grep '^THEME ')"
check "and leaves an operator's own icon theme alone" "ICON_THEME Papirus" "$(echo "$out" | grep '^ICON_THEME ')"

: > "$opened"
# An exported but empty marker is absent, the rule the pin marker beside it already follows.
env QT_QPA_PLATFORMTHEME=qt6ct QS_ICON_THEME=Papirus FLEA_QT_THEME= \
  PATH="$D/bin:/usr/bin:/bin" $BIN --open "$D/file.txt" 2>&1 | cat >/dev/null
check "the empty-theme-marker open returned success" "0" "${PIPESTATUS[0]}"
wait_for_line "$opened" '^THP_enabled'
out=$(cat "$opened")
check "an empty theme marker hands nothing back" "THEME qt6ct" "$(echo "$out" | grep '^THEME ')"
check "and leaves the icon theme it found alone" "ICON_THEME Papirus" "$(echo "$out" | grep '^ICON_THEME ')"

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
# canonicalize already proved the file is there, so the refusal sentence names the launcher instead.
check "and its refusal is one sentence naming $open_handoff" "1" \
  "$(echo "$out" | grep -c "$open_handoff open refused")"
check "with no errno in it" "0" "$(echo "$out" | grep -c 'os error')"

out=$($BIN --open 2>&1 </dev/null)
check "--open with no path is a usage error" "1" "$(echo "$out" | grep -c -- '--open')"

# Every program the openers hand off to by name must be shipped by a PKGBUILD dependency, or the
# package installs and the button it belongs to does nothing at all. The three sources are src/open.rs,
# src/terminal.rs and ui/Opener.qml, derived above; wl-copy shipped undeclared until this check saw it.
# The table is here rather than from pacman so the check runs off the box too, and a handoff with no
# row in it is itself a failure.
handoff_package() {
  case "$1" in
    gio) printf 'glib2' ;;
    xdg-terminal-exec) printf 'xdg-terminal-exec' ;;
    wl-copy) printf 'wl-clipboard' ;;
    *) printf '' ;;
  esac
}
handoffs=$(printf '%s\n%s\n%s\n' "$open_handoff" "$terminal_handoff" "$opener_qml_handoff" | sort -u)
# The denominator, because a derived loop over nothing reports green having checked nothing: a
# renamed file or a handoff name this grep cannot match would otherwise pass in silence.
check "the three openers hand off to three programs by name" "3" "$(printf '%s\n' "$handoffs" | grep -c .)"
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
  printf 'printf "ICD %%s\\n" "${VK_ICD_FILENAMES-unset}"\n'
  printf 'printf "DRIVER_FILES %%s\\n" "${VK_DRIVER_FILES-unset}"\n'
  printf 'printf "PIN %%s\\n" "${FLEA_VK_PIN-unset}"\n'
  printf 'printf "THEME %%s\\n" "${QT_QPA_PLATFORMTHEME-unset}"\n'
  printf 'printf "ICON_THEME %%s\\n" "${QS_ICON_THEME-unset}"\n'
  printf 'printf "THEME_MARKER %%s\\n" "${FLEA_QT_THEME-unset}"\n'
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
# Flea's own pin carries a marker, and only a marked pin is taken back off a terminal Flea opens.
VK_ICD_FILENAMES=/tmp/flea-pin-icd.json VK_DRIVER_FILES=/tmp/flea-pin-driver.json FLEA_VK_PIN=1 \
  PATH="$D/bin:/usr/bin:/bin" $BIN --terminal "$D/dir" 2>&1 | cat >/dev/null
# Same reason as the open path's marker arms: cat's status says nothing about the launcher.
check "the marked-pin terminal returned success" "0" "${PIPESTATUS[0]}"
wait_for_line "$ran" '^THP_enabled'
out=$(cat "$ran")
check "a marked pin is dropped from a terminal" "1" "$(echo "$out" | grep -c '^ICD unset$')"
check "and its driver file list goes with it" "1" "$(echo "$out" | grep -c '^DRIVER_FILES unset$')"
check "and the marker does not leak into the terminal" "1" "$(echo "$out" | grep -c '^PIN unset$')"

: > "$ran"
# An operator's own list carries no marker, so it must survive into the terminal they open.
VK_ICD_FILENAMES=/tmp/flea-operator-icd.json VK_DRIVER_FILES=/tmp/flea-operator-driver.json \
  PATH="$D/bin:/usr/bin:/bin" $BIN --terminal "$D/dir" 2>&1 | cat >/dev/null
check "the operator-list terminal returned success" "0" "${PIPESTATUS[0]}"
wait_for_line "$ran" '^THP_enabled'
out=$(cat "$ran")
check "an operator's own ICD list reaches the terminal" "1" "$(echo "$out" | grep -c '^ICD /tmp/flea-operator-icd.json$')"
check "and so does their own driver file list" "1" "$(echo "$out" | grep -c '^DRIVER_FILES /tmp/flea-operator-driver.json$')"

: > "$ran"
# Both spawn sites reach one shared guard, pin_is_marked, so each needs this arm of its own.
VK_ICD_FILENAMES=/tmp/flea-operator-icd.json VK_DRIVER_FILES=/tmp/flea-operator-driver.json FLEA_VK_PIN= \
  PATH="$D/bin:/usr/bin:/bin" $BIN --terminal "$D/dir" 2>&1 | cat >/dev/null
check "the empty-marker terminal returned success" "0" "${PIPESTATUS[0]}"
wait_for_line "$ran" '^THP_enabled'
out=$(cat "$ran")
check "an empty marker leaves an operator's ICD list alone in a terminal" "1" "$(echo "$out" | grep -c '^ICD /tmp/flea-operator-icd.json$')"
check "and leaves their driver file list alone in a terminal" "1" "$(echo "$out" | grep -c '^DRIVER_FILES /tmp/flea-operator-driver.json$')"

: > "$ran"
# The same hand-back on the terminal path, which carries its own copy of these guards.
env -u QT_QPA_PLATFORMTHEME QS_ICON_THEME=Yaru-blue FLEA_QT_THEME=gtk3 \
  PATH="$D/bin:/usr/bin:/bin" $BIN --terminal "$D/dir" 2>&1 | cat >/dev/null
check "the traded-theme terminal returned success" "0" "${PIPESTATUS[0]}"
wait_for_line "$ran" '^THP_enabled'
out=$(cat "$ran")
check "a terminal gets the traded platform theme back" "THEME gtk3" "$(echo "$out" | grep '^THEME ')"
check "and not the icon theme Flea named for Quickshell" "ICON_THEME unset" "$(echo "$out" | grep '^ICON_THEME ')"
check "and not the marker that said so" "THEME_MARKER unset" "$(echo "$out" | grep '^THEME_MARKER ')"

: > "$ran"
# No marker is no trade here either, and an exported empty one is absent.
env QT_QPA_PLATFORMTHEME=qt6ct QS_ICON_THEME=Papirus \
  PATH="$D/bin:/usr/bin:/bin" $BIN --terminal "$D/dir" 2>&1 | cat >/dev/null
check "the untraded terminal returned success" "0" "${PIPESTATUS[0]}"
wait_for_line "$ran" '^THP_enabled'
out=$(cat "$ran")
check "an unmarked terminal keeps the platform theme" "THEME qt6ct" "$(echo "$out" | grep '^THEME ')"
check "and keeps an operator's own icon theme" "ICON_THEME Papirus" "$(echo "$out" | grep '^ICON_THEME ')"

: > "$ran"
env QT_QPA_PLATFORMTHEME=qt6ct QS_ICON_THEME=Papirus FLEA_QT_THEME= \
  PATH="$D/bin:/usr/bin:/bin" $BIN --terminal "$D/dir" 2>&1 | cat >/dev/null
check "the empty-theme-marker terminal returned success" "0" "${PIPESTATUS[0]}"
wait_for_line "$ran" '^THP_enabled'
out=$(cat "$ran")
check "an empty theme marker hands nothing back to a terminal" "THEME qt6ct" "$(echo "$out" | grep '^THEME ')"
check "and leaves the icon theme it found alone" "ICON_THEME Papirus" "$(echo "$out" | grep '^ICON_THEME ')"

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

# At a terminal --picker restarts the portal as Settings does; piped, it leaves that to its caller.
D="$FIXTURE_ROOT/flea-portal-restart-test-$$"
sandbox_make "$D"
mkdir -p "$D/data/xdg-desktop-portal/portals" "$D/config/hypr" "$D/bin"
: > "$D/data/xdg-desktop-portal/portals/flea.portal"
printf -- '-- stock omarchy bindings\n' > "$D/config/hypr/bindings.lua"
# Sample input: systemctl --user try-restart xdg-desktop-portal.service
printf '#!/bin/sh\nprintf "%%s\\n" "$*" >> "%s/restarts"\nexit "$(cat "%s/restart-status")"\n' "$D" "$D" > "$D/bin/systemctl"
# hyprctl unreachable, so the float block is written but the operator's own Hyprland is never reloaded.
printf '#!/bin/sh\nexit 1\n' > "$D/bin/hyprctl"
chmod 755 "$D/bin/systemctl" "$D/bin/hyprctl"
echo 0 > "$D/restart-status"
picker_env=(env XDG_DATA_HOME="$D/data" XDG_DATA_DIRS="$D/no-such-data-dir" XDG_CONFIG_HOME="$D/config" XDG_CURRENT_DESKTOP=Hyprland
  PATH="$D/bin:/usr/bin:/bin")
# script(1) runs the command on a pseudo-terminal; the first argument is shell redirection applied inside it.
at_terminal() { local redirect=$1; shift; script -qec "$(printf '%q ' "${picker_env[@]}" "$BIN_REAL" "$@") $redirect" /dev/null </dev/null | tr -d '\r'; }
restarts() { cat "$D/restarts" 2>/dev/null | wc -l | tr -d ' '; }
restarted="xdg-desktop-portal restarted if it was running, so file dialogs follow now"
at_startup="xdg-desktop-portal reads this at startup: systemctl --user restart xdg-desktop-portal"
command -v script >/dev/null || check "script(1) is installed for the terminal cases" "yes" "no"
out=$("${picker_env[@]}" "$BIN_REAL" --picker 2>&1 </dev/null)
check "a piped --picker claims" "1" "$(grep -c 'flea;gtk, written to' <<<"$out")"
check "a piped --picker restarts nothing" "0" "$(restarts)"
check "and ends on the restart command instead" "$at_startup" "$(grep '^xdg-desktop-portal ' <<<"$out")"
out=$(at_terminal "" --picker off)
check "--picker off at a terminal restarts the portal once, with the switch's exact command" \
  "--user try-restart xdg-desktop-portal.service" "$(cat "$D/restarts" 2>/dev/null)"
check "and says file dialogs follow now" "$restarted" "$(grep '^xdg-desktop-portal ' <<<"$out")"
# Only stdout decides: a terminal on stdin and stderr with stdout in a file is a script capturing it.
at_terminal "> $(printf '%q' "$D/captured")" --picker >/dev/null
check "a terminal run whose stdout is redirected restarts nothing" "1" "$(restarts)"
check "and its captured output names the restart command" "$at_startup" "$(grep '^xdg-desktop-portal ' "$D/captured")"
out=$(at_terminal "</dev/null 2>/dev/null" --picker off)
check "stdout on a terminal restarts, whatever stdin and stderr are" "2" "$(restarts)"
check "and says so" "$restarted" "$(grep '^xdg-desktop-portal ' <<<"$out")"
echo 1 > "$D/restart-status"
out=$(at_terminal "" --picker)
check "a refused restart is still tried once" "3" "$(restarts)"
check "and ends on the restart command, not a claim it happened" "$at_startup" "$(grep '^xdg-desktop-portal ' <<<"$out")"
# A desktop-specific portals.conf is the one user file the portal reads, so the claim edits it and off restores it.
echo 0 > "$D/restart-status"
shadow="$D/config/xdg-desktop-portal/hyprland-portals.conf"
nautilus=$'[preferred]\ndefault=hyprland;gtk\norg.freedesktop.impl.portal.FileChooser=gnome;gtk\n'
printf '%s' "$nautilus" > "$shadow"
out=$(at_terminal "" --picker)
check "the claim names the desktop file it wrote" "1" "$(grep -c 'written to .*hyprland-portals.conf, the file xdg-desktop-portal reads on this desktop' <<<"$out")"
check "and the backend it replaced" "1" "$(grep -c 'it named gnome;gtk before, and the undo below puts that back' <<<"$out")"
check "the desktop file now routes the chooser to flea" "1" "$(grep -cx 'org.freedesktop.impl.portal.FileChooser=flea;gtk' "$shadow")"
check "and keeps the note naming what it replaced" "1" "$(grep -cx '# flea replaced: org.freedesktop.impl.portal.FileChooser=gnome;gtk' "$shadow")"
check "and keeps its default line" "1" "$(grep -cx 'default=hyprland;gtk' "$shadow")"
check "a routed claim at a terminal restarts the portal" "4" "$(restarts)"
check "and says file dialogs follow now" "$restarted" "$(grep '^xdg-desktop-portal ' <<<"$out")"
check "an older Flea's portals.conf line is still there before the undo" "1" "$(grep -cx 'org.freedesktop.impl.portal.FileChooser=flea;gtk' "$D/config/xdg-desktop-portal/portals.conf")"
# The undo reads the directory, not the session: an off run over ssh has no XDG_CURRENT_DESKTOP.
no_desktop=(env -u XDG_CURRENT_DESKTOP)
for setting in "${picker_env[@]:1}"; do [ "$setting" = XDG_CURRENT_DESKTOP=Hyprland ] || no_desktop+=("$setting"); done
out=$(script -qec "$(printf '%q ' "${no_desktop[@]}" "$BIN_REAL" --picker off)" /dev/null </dev/null | tr -d '\r')
check "--picker off puts the replaced backend back byte for byte" "${nautilus}x" "$(cat "$shadow"; printf x)"
check "and says so, without the session's desktop to name the file" "1" "$(grep -c 'gnome;gtk put back in .*hyprland-portals.conf' <<<"$out")"
check "and removes the portals.conf an older Flea left, naming it" "1" "$(grep -c 'portals.conf held nothing else, so it is gone' <<<"$out")"
check "which is gone" "no" "$([ -e "$D/config/xdg-desktop-portal/portals.conf" ] && echo yes || echo no)"
check "and restarts the portal once more" "5" "$(restarts)"
# Without the session's desktop a claim cannot tell which file the portal reads, so it refuses before either half writes.
binds_before=$(cat "$D/config/hypr/bindings.lua"; printf x)
out=$(script -qec "$(printf '%q ' "${no_desktop[@]}" "$BIN_REAL" --picker); echo rc=\$?" /dev/null </dev/null | tr -d '\r')
check "a claim with no desktop named beside a desktop file refuses" "1" "$(grep -c 'XDG_CURRENT_DESKTOP is not set, so it is unknown whether .*hyprland-portals.conf is the file this desktop reads' <<<"$out")"
check "and exits 1" "1" "$(grep -cx 'rc=1' <<<"$out")"
check "and leaves that file as it was" "${nautilus}x" "$(cat "$shadow"; printf x)"
check "and writes no picker window rule into bindings.lua" "$binds_before" "$(cat "$D/config/hypr/bindings.lua"; printf x)"
check "and writes no portals.conf behind it" "no" "$([ -e "$D/config/xdg-desktop-portal/portals.conf" ] && echo yes || echo no)"
check "and restarts nothing" "5" "$(restarts)"
# With no desktop named and no desktop file, portals.conf is the only user file the portal can read.
rm "$shadow"
out=$(script -qec "$(printf '%q ' "${no_desktop[@]}" "$BIN_REAL" --picker); echo rc=\$?" /dev/null </dev/null | tr -d '\r')
check "a claim with no desktop named and no desktop file writes portals.conf" "1" "$(grep -c 'flea;gtk, written to .*/xdg-desktop-portal/portals.conf' <<<"$out")"
check "and exits 0" "1" "$(grep -cx 'rc=0' <<<"$out")"
check "and restarts the portal" "6" "$(restarts)"
# The routing alone decides: a claim whose float block fails (no bindings.lua) still exits 1 but restarts.
rm "$D/config/hypr/bindings.lua"
out=$(at_terminal "" --picker)
check "a routed claim whose window half failed names that failure" "1" "$(grep -c 'bindings.lua could not be read' <<<"$out")"
check "and still restarts the portal" "7" "$(restarts)"
check "and says file dialogs follow now" "$restarted" "$(grep '^xdg-desktop-portal ' <<<"$out")"
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
