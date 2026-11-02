#!/bin/bash
# Drives the real binary and asserts the mode contract, since main() is only reachable here.
set -u
# Hard rule 9's guard, which owns FIXTURE_ROOT and every create and delete below.
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1

BIN=${FLEA_BIN:-./target/debug/flea}
# Without this every case below drives a missing binary and reports the result as a product failure.
[ -x "$BIN" ] || { echo "modes.sh: $BIN is missing, run cargo build" >&2; exit 1; }
# current_exe() answers with the kernel's own resolved path, so the expectation is resolved the same way.
BIN_REAL=$(readlink -f "$BIN")
# Isolate the executable from a real sibling flea-gui so only the recording child can run.
MODE_BIN=$(mktemp -d "$FIXTURE_ROOT/flea-modes-bin.XXXXXXXX") || exit 1
: > "$MODE_BIN/.flea-test-sandbox"
cp "$BIN_REAL" "$MODE_BIN/flea" || exit 1
BIN="$MODE_BIN/flea"
BIN_REAL=$(readlink -f "$BIN")
trap 'sandbox_remove "$MODE_BIN"' EXIT
# Named, not re-derived from ui_dir's own walk, which an installed /usr/share/flea/ui outranks.
UI_REAL=$(readlink -f .)/ui
# An operator exporting any of these would answer for src/gui.rs, which is the thing under test here.
unset FLEA_FRONTEND QSG_RHI_BACKEND FLEA_RENDERER_AUTOMATIC QT_VK_PHYSICAL_DEVICE_INDEX VK_DRIVER_FILES VK_ICD_FILENAMES QS_ICON_THEME FLEA_QT_THEME \
  FLEA_PREFETCH FLEA_PREFETCH_SHELL VK_ADD_DRIVER_FILES VK_LOADER_DRIVERS_SELECT VK_LOADER_DRIVERS_DISABLE FLEA_VK_DISABLE
fail=0
# Every --gui launch below may cache its Vulkan probe under XDG_CACHE_HOME or ~/.cache, so none of them reaches the operator's own.
CACHE_ROOT="$FIXTURE_ROOT/flea-modes-cache-$$"
sandbox_make "$CACHE_ROOT"
export XDG_CACHE_HOME="$CACHE_ROOT"

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
check "a non-empty WAYLAND_DISPLAY is accepted" "1" "$(echo "$out" | grep -c 'the window could not start')"
out=$(env -u WAYLAND_DISPLAY DISPLAY=:99 PATH=/nonexistent-flea-test-path $BIN --gui 2>&1 </dev/null)
check "a non-empty DISPLAY is accepted" "1" "$(echo "$out" | grep -c 'the window could not start')"
out=$(env WAYLAND_DISPLAY= DISPLAY=:99 PATH=/nonexistent-flea-test-path $BIN --gui 2>&1 </dev/null)
check "an empty WAYLAND_DISPLAY does not mask DISPLAY" "1" "$(echo "$out" | grep -c 'the window could not start')"
out=$(env WAYLAND_DISPLAY=flea-modes-test-display DISPLAY= PATH=/nonexistent-flea-test-path $BIN --gui 2>&1 </dev/null)
check "an empty DISPLAY does not mask WAYLAND_DISPLAY" "1" "$(echo "$out" | grep -c 'the window could not start')"
out=$(env WAYLAND_DISPLAY=flea-modes-test-display DISPLAY=:99 PATH=/nonexistent-flea-test-path $BIN --gui 2>&1 </dev/null)
check "two non-empty displays are accepted" "1" "$(echo "$out" | grep -c 'the window could not start')"

# A flea-gui found neither beside flea nor on PATH is a broken install, so the error names the binary and its cause.
out=$(env WAYLAND_DISPLAY=flea-modes-test-display PATH=/nonexistent-flea-test-path $BIN --gui 2>&1 </dev/null)
check "missing GPUI binary is named" "1" "$(echo "$out" | grep -c 'the window could not start')"
check "missing GPUI binary carries its cause" "1" "$(echo "$out" | grep -c 'os error')"

# With no flag and no tty the default must be the window branch, which is what a .desktop launch takes.
out=$(env -u WAYLAND_DISPLAY -u DISPLAY $BIN . 2>&1 </dev/null)
check "no flag defaults to the window" "1" "$(echo "$out" | grep -c 'no graphical session')"

# A shell PTY proves bare flea still means the product that exists, not the reserved terminal interface.
out=$(env -u WAYLAND_DISPLAY -u DISPLAY script -qec "$BIN ." /dev/null 2>&1)
check "no flag at a terminal defaults to the window" "1" "$(echo "$out" | grep -c 'no graphical session')"

# --tui with no tty must refuse rather than write escape codes into a pipe.
out=$($BIN --tui 2>&1 </dev/null | cat)
check "--tui without a tty refuses" "1" "$(echo "$out" | grep -c 'needs a terminal')"

# Flags that cannot be used together are a usage error; the message is checked too, see AGENTS.md Modes.
out=$($BIN --tui --gui 2>&1 >/dev/null </dev/null)
rc=$?
check "--tui --gui is a usage error" "2" "$rc"
check "--tui --gui names the conflict" "1" "$(echo "$out" | grep -c 'cannot be used together')"

# The prctl and renderer choice have to survive exec, so a stub qs reports the launched child.
D="$FIXTURE_ROOT/flea-thp-test-$$"
sandbox_make "$D"
# One stub reports everything the launch has to carry across exec, huge pages and target included.
cat > "$D/flea-gui" <<'STUB'
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
printf 'DRIVERS_DISABLE %s\n' "${VK_LOADER_DRIVERS_DISABLE-unset}"
printf 'DISABLE_MARKER %s\n' "${FLEA_VK_DISABLE-unset}"
STUB
chmod +x "$D/flea-gui"
out=$(env FLEA_BIN=stale FLEA_UI="$UI_REAL" WAYLAND_DISPLAY=flea-modes-test-display PATH="$D:/usr/bin:/bin" $BIN --gui 2>&1 </dev/null)
check "the launched shell has transparent huge pages off" "1" \
  "$(echo "$out" | grep -c 'THP_enabled:[[:space:]]*0')"
check "the launched shell reported its THP state at all" "1" "$(echo "$out" | grep -c 'THP_enabled')"
# GPUI opens its own window, so the default launch hands the binary no QML entry.
check "the default launch targets GPUI without a QML entry" "ARGV " "$(echo "$out" | grep '^ARGV ')"
# 24003ab made an explicit FLEA_BIN the operator's choice, the rule FLEA_UI and QSG_RHI_BACKEND
# already follow, and this check was left asserting the behaviour that commit replaced. The launch
# above sets one deliberately, so the operator's own value is what must reach the shell; the unset
# case further down is the one that derives the running binary.
check "an explicit FLEA_BIN is the operator's, and reaches the shell" "FLEA_BIN stale" \
  "$(echo "$out" | grep '^FLEA_BIN ')"
check "GPUI chooses its own renderer" "1" "$(echo "$out" | grep -c '^RENDERER $')"
check "GPUI inherits no QML retry" "1" "$(echo "$out" | grep -c '^AUTOMATIC unset$')"
# Which drivers are excluded depends on this box's GPUs, but the marker always names the exact filter Flea wrote.
disabled=$(echo "$out" | sed -n 's/^DRIVERS_DISABLE //p')
check "a driver exclusion and its marker travel together" "$disabled" "$(echo "$out" | sed -n 's/^DISABLE_MARKER //p')"
out=$(env VK_ICD_FILENAMES=/tmp/flea-operator-icd.json WAYLAND_DISPLAY=flea-modes-test-display PATH="$D:/usr/bin:/bin" $BIN --gui 2>&1 </dev/null)
check "an explicit ICD list is preserved" "1" "$(echo "$out" | grep -c '^ICD /tmp/flea-operator-icd.json$')"
out=$(env VK_DRIVER_FILES=/tmp/flea-operator-driver.json WAYLAND_DISPLAY=flea-modes-test-display PATH="$D:/usr/bin:/bin" $BIN --gui 2>&1 </dev/null)
check "an explicit driver file list is preserved" "1" "$(echo "$out" | grep -c '^DRIVER_FILES /tmp/flea-operator-driver.json$')"
check "and an explicit driver file list excludes no driver" "1" "$(echo "$out" | grep -c '^DRIVERS_DISABLE unset$')"
# The loader's own filter is the operator's too, and Flea neither replaces it nor marks it as its own.
out=$(env VK_LOADER_DRIVERS_DISABLE=flea-operator_icd.json WAYLAND_DISPLAY=flea-modes-test-display PATH="$D:/usr/bin:/bin" $BIN --gui 2>&1 </dev/null)
check "an operator's own driver filter is preserved" "1" "$(echo "$out" | grep -c '^DRIVERS_DISABLE flea-operator_icd.json$')"
check "and it is not marked as Flea's" "1" "$(echo "$out" | grep -c '^DISABLE_MARKER unset$')"
# The system roots src/icdexclude.rs searches beyond XDG_*: a loader settings file there excludes nothing on its own.
system_settings=""
for root in /etc/xdg /etc /usr/local/share /usr/share; do
  [ -e "$root/vulkan/loader_settings.d/vk_loader_settings.json" ] && system_settings="$root"
done
# The planted-settings run below inherits HOME and the XDG home and data variables, so those roots count too; its XDG_CONFIG_DIRS is the planted one.
settings_roots=("${XDG_CONFIG_HOME:-${HOME:+$HOME/.config}}" "${XDG_DATA_HOME:-${HOME:+$HOME/.local/share}}")
IFS=: read -r -a data_dirs <<<"${XDG_DATA_DIRS:-}"
settings_roots+=("${data_dirs[@]}")
inherited_settings="$system_settings"
for root in "${settings_roots[@]}"; do
  [ -n "$root" ] && [ -e "$root/vulkan/loader_settings.d/vk_loader_settings.json" ] && inherited_settings="$root"
done
# A loader settings file can add or choose drivers itself, so its presence is the operator's choice as well.
mkdir -p "$D/xdg/vulkan/loader_settings.d" && printf '{}\n' > "$D/xdg/vulkan/loader_settings.d/vk_loader_settings.json"
out=$(env XDG_CONFIG_DIRS="$D/xdg" WAYLAND_DISPLAY=flea-modes-test-display PATH="$D:/usr/bin:/bin" $BIN --gui 2>&1 </dev/null)
if [ -z "$inherited_settings" ]; then
  check "a loader settings file excludes no driver" "1" "$(echo "$out" | grep -c '^DRIVERS_DISABLE unset$')"
else
  echo "skip: a loader settings file stands under $inherited_settings, so the planted one proves nothing"
fi
# A planted absent-vendor manifest, so an empty filter cannot pass; its run is the control for the two unset ones.
probe="$D/xdg-probe"
mkdir -p "$probe/config" "$probe/data/vulkan/icd.d" "$probe/home"
# Mirrors present_devices in src/icdexclude.rs: decidable exactly when the code can decide.
nvidia_id=0x10de; amd_id=0x1002; intel_id=0x8086
nvidia_lib=libGLX_nvidia.so.0; amd_lib=libvulkan_radeon.so; intel_lib=libvulkan_intel.so
hasvk_lib=libvulkan_intel_hasvk.so; hasvk_ceiling=0x1800; broxton_below=0x0a84; cherryview_ids="0x22b0 0x22b1 0x22b2 0x22b3"
decidable=1; skip_reason=""; present=""; present_pairs=""; plant_lib=""
for dev in /sys/bus/pci/devices/*; do
  [ -r "$dev/class" ] || { decidable=0; skip_reason="a PCI device class is unreadable"; break; }
  case "$(cat "$dev/class" 2>/dev/null)" in 0x03*|0X03*) ;; *) continue ;; esac
  [ -r "$dev/vendor" ] || { decidable=0; skip_reason="a display controller vendor is unreadable"; break; }
  [ -r "$dev/device" ] || { decidable=0; skip_reason="a display controller device id is unreadable"; break; }
  present="$present $(cat "$dev/vendor" 2>/dev/null)"
  present_pairs="$present_pairs $(cat "$dev/vendor" 2>/dev/null):$(cat "$dev/device" 2>/dev/null)"
done
if [ "$decidable" = 1 ]; then
  for card in /sys/class/drm/card[0-9]*; do
    [ -e "$card" ] || continue
    case "${card##*/}" in *-*) continue ;; esac
    sub=$(readlink "$card/device/subsystem" 2>/dev/null) || { decidable=0; skip_reason="a DRM card subsystem is unreadable"; break; }
    [ "${sub##*/}" = pci ] || { decidable=0; skip_reason="a DRM card is off the PCI bus"; break; }
    [ -r "$card/device/vendor" ] || { decidable=0; skip_reason="a DRM card vendor is unreadable"; break; }
    [ -r "$card/device/device" ] || { decidable=0; skip_reason="a DRM card device id is unreadable"; break; }
    present="$present $(cat "$card/device/vendor" 2>/dev/null)"
    present_pairs="$present_pairs $(cat "$card/device/vendor" 2>/dev/null):$(cat "$card/device/device" 2>/dev/null)"
  done
fi
[ "$decidable" = 1 ] && [ -z "$present" ] && { decidable=0; skip_reason="no present vendor"; }
[ "$decidable" = 1 ] && [ -n "$system_settings" ] && { decidable=0; skip_reason="a system loader settings file stands under $system_settings"; }
if [ "$decidable" = 1 ]; then
  lower=$(printf '%s' "$present" | tr 'A-FX' 'a-fx')
  case "$lower " in *" $nvidia_id "*) ;; *) plant_lib="$nvidia_lib" ;; esac
  if [ -z "$plant_lib" ]; then case "$lower " in *" $amd_id "*) ;; *) plant_lib="$amd_lib" ;; esac; fi
  if [ -z "$plant_lib" ]; then case "$lower " in *" $intel_id "*) ;; *) plant_lib="$intel_lib" ;; esac; fi
  [ -n "$plant_lib" ] || { decidable=0; skip_reason="every known vendor is present"; }
fi
[ "$decidable" = 1 ] && printf '{"ICD":{"library_path":"%s"}}\n' "$plant_lib" > "$probe/data/vulkan/icd.d/flea_probe_absent_icd.json"
# Mirrors is_hasvk_intel in src/vulkan.rs: a hasvk-class Intel device keeps the hasvk manifest, any other present set excludes it.
hasvk_here=0
if [ "$decidable" = 1 ]; then
  for pair in $present_pairs; do
    v=${pair%%:*}; d=${pair##*:}
    [ "$((v))" -eq "$((intel_id))" ] || continue
    { [ "$((d))" -lt "$((hasvk_ceiling))" ] && [ "$((d))" -ne "$((broxton_below))" ]; } && hasvk_here=1
    for c in $cherryview_ids; do [ "$((d))" -eq "$((c))" ] && hasvk_here=1; done
  done
  printf '{"ICD":{"library_path":"%s"}}\n' "$hasvk_lib" > "$probe/data/vulkan/icd.d/flea_probe_hasvk_icd.json"
fi
if [ "$decidable" = 1 ]; then
  out=$(env XDG_CONFIG_HOME="$probe/config" XDG_CONFIG_DIRS="" XDG_DATA_HOME="$probe/data" XDG_DATA_DIRS="" HOME="$probe/home" \
    WAYLAND_DISPLAY=flea-modes-test-display PATH="$D:/usr/bin:/bin" $BIN --gui 2>&1 </dev/null)
  disabled=$(echo "$out" | sed -n 's/^DRIVERS_DISABLE //p')
  check "a planted absent-vendor manifest is excluded" "1" "$(echo "$disabled" | grep -c 'flea_probe_absent_icd.json')"
  check "and the marker names that same filter" "$disabled" "$(echo "$out" | sed -n 's/^DISABLE_MARKER //p')"
  if [ "$hasvk_here" = 0 ]; then
    check "a planted hasvk manifest is excluded without a hasvk-class GPU" "1" "$(echo "$disabled" | grep -c 'flea_probe_hasvk_icd.json')"
  else
    check "a planted hasvk manifest is kept beside a hasvk-class GPU" "0" "$(echo "$disabled" | grep -c 'flea_probe_hasvk_icd.json')"
  fi
  mkdir -p "$probe/config/vulkan/loader_settings.d" && printf '{}\n' > "$probe/config/vulkan/loader_settings.d/vk_loader_settings.json"
  out=$(env XDG_CONFIG_HOME="$probe/config" XDG_CONFIG_DIRS="" XDG_DATA_HOME="$probe/data" XDG_DATA_DIRS="" HOME="$probe/home" \
    WAYLAND_DISPLAY=flea-modes-test-display PATH="$D:/usr/bin:/bin" $BIN --gui 2>&1 </dev/null)
  check "a loader settings file excludes nothing, planted manifest included" "1" "$(echo "$out" | grep -c '^DRIVERS_DISABLE unset$')"
  rm "$probe/config/vulkan/loader_settings.d/vk_loader_settings.json"
  out=$(env XDG_CONFIG_HOME="$probe/config" XDG_CONFIG_DIRS="" XDG_DATA_HOME="$probe/data" XDG_DATA_DIRS="" HOME="$probe/home" \
    VK_DRIVER_FILES=/tmp/flea-operator-driver.json WAYLAND_DISPLAY=flea-modes-test-display PATH="$D:/usr/bin:/bin" $BIN --gui 2>&1 </dev/null)
  check "an operator driver choice excludes nothing, planted manifest included" "1" "$(echo "$out" | grep -c '^DRIVERS_DISABLE unset$')"
else
  echo "skip: $skip_reason, leaving the decision to the unit tests"
fi
# A new window or the OpenGL retry inherits the shell's own filter, which the launcher decides again in every arm.
for renderer in "" opengl; do
  out=$(env QSG_RHI_BACKEND="$renderer" VK_LOADER_DRIVERS_DISABLE=flea-stale_icd.json FLEA_VK_DISABLE=flea-stale_icd.json \
    WAYLAND_DISPLAY=flea-modes-test-display PATH="$D:/usr/bin:/bin" $BIN --gui 2>&1 </dev/null)
  check "an inherited Flea exclusion is decided again${renderer:+ on $renderer}, not passed on" "0" "$(echo "$out" | grep -c 'flea-stale_icd.json')"
  disabled=$(echo "$out" | sed -n 's/^DRIVERS_DISABLE //p')
  check "and the fresh decision${renderer:+ on $renderer} carries its own marker" "$disabled" "$(echo "$out" | sed -n 's/^DISABLE_MARKER //p')"
done
# A marker that leaked into a shell the operator then changed the filter in names another filter, which stays theirs.
out=$(env VK_LOADER_DRIVERS_DISABLE=flea-operator_icd.json FLEA_VK_DISABLE=flea-stale_icd.json \
  WAYLAND_DISPLAY=flea-modes-test-display PATH="$D:/usr/bin:/bin" $BIN --gui 2>&1 </dev/null)
check "a filter its leaked marker does not name is the operator's" "1" "$(echo "$out" | grep -c '^DRIVERS_DISABLE flea-operator_icd.json$')"
check "and the stale marker is not handed on" "1" "$(echo "$out" | grep -c '^DISABLE_MARKER unset$')"
# A main window strips obsolete renderer choices and replaces inherited prefetch ownership.
out=$(env QSG_RHI_BACKEND=opengl FLEA_RENDERER_AUTOMATIC=stale FLEA_PREFETCH=stale FLEA_PREFETCH_SHELL=1 \
  WAYLAND_DISPLAY=flea-modes-test-display PATH="$D:/usr/bin:/bin" $BIN --gui 2>&1 </dev/null)
check "GPUI clears the Qt renderer choice" "RENDERER " "$(echo "$out" | grep '^RENDERER ')"
check "GPUI clears the QML retry marker" "AUTOMATIC unset" "$(echo "$out" | grep '^AUTOMATIC ')"
check "GPUI replaces inherited prefetch ownership with its current list" "PREFETCH $CACHE_ROOT/flea/prefetch" "$(echo "$out" | grep '^PREFETCH ')"
out=$(env -u FLEA_BIN WAYLAND_DISPLAY=flea-modes-test-display PATH="$D:/usr/bin:/bin" $BIN --gui 2>&1 </dev/null)
check "an unset FLEA_BIN is derived from the running binary" "FLEA_BIN $BIN_REAL" "$(echo "$out" | grep '^FLEA_BIN ')"
out=$(env FLEA_FRONTEND=qml WAYLAND_DISPLAY=flea-modes-test-display PATH="$D:/usr/bin:/bin" $BIN --gui 2>&1 </dev/null)
rc=$?
check "the legacy GUI route refuses" "2" "$rc"
check "the legacy GUI route gives one sentence" "flea: the QML frontend is no longer available; unset FLEA_FRONTEND to open the GPUI window" "$out"

# A chooser started from a Flea terminal inherits both variables and must not record over the main window's list.
out=$(env FLEA_PICKER='{"stub":true}' FLEA_PREFETCH="$D/stale-list" FLEA_PREFETCH_SHELL=1 FLEA_UI="$UI_REAL" \
  WAYLAND_DISPLAY=flea-modes-test-display PATH="$D:/usr/bin:/bin" $BIN --pick "$D/reply.json" 2>&1 </dev/null)
check "a chooser drops the prefetch list" "PREFETCH unset" "$(echo "$out" | grep '^PREFETCH ')"
check "and the shell pid with it" "1" "$(echo "$out" | grep -c '^PREFETCH_SHELL unset ')"

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
  printf 'printf "DRIVERS_DISABLE %%s\\n" "${VK_LOADER_DRIVERS_DISABLE-unset}"\n'
  printf 'printf "DISABLE_MARKER %%s\\n" "${FLEA_VK_DISABLE-unset}"\n'
  printf 'printf "THEME %%s\\n" "${QT_QPA_PLATFORMTHEME-unset}"\n'
  printf 'printf "ICON_THEME %%s\\n" "${QS_ICON_THEME-unset}"\n'
  printf 'printf "THEME_MARKER %%s\\n" "${FLEA_QT_THEME-unset}"\n'
  printf 'printf "TAB_PID %%s\\n" "${FLEA_TAB_SOURCE_PID-unset}"\n'
  printf 'printf "TAB_CURSOR %%s\\n" "${FLEA_TAB_CURSOR-unset}"\n'
  printf 'printf "TAB_TOKEN %%s\\n" "${FLEA_TAB_TOKEN-unset}"\n'
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
# Flea's own driver exclusion is marked with its exact value, and is taken back off a program Flea opens.
VK_LOADER_DRIVERS_DISABLE=flea-absent_icd.json FLEA_VK_DISABLE=flea-absent_icd.json \
  PATH="$D/bin:/usr/bin:/bin" $BIN --open "$D/file.txt" 2>&1 | cat >/dev/null
check "the marked-exclusion open returned success" "0" "${PIPESTATUS[0]}"
wait_for_line "$opened" '^THP_enabled'
out=$(cat "$opened")
check "a marked driver exclusion is dropped from an opened program" "1" "$(echo "$out" | grep -c '^DRIVERS_DISABLE unset$')"
check "and its marker does not leak onward" "1" "$(echo "$out" | grep -c '^DISABLE_MARKER unset$')"

: > "$opened"
# A leaked marker naming another filter makes the filter beside it the operator's, so only the marker goes.
VK_LOADER_DRIVERS_DISABLE=flea-operator_icd.json FLEA_VK_DISABLE=flea-absent_icd.json \
  PATH="$D/bin:/usr/bin:/bin" $BIN --open "$D/file.txt" 2>&1 | cat >/dev/null
check "the leaked-marker open returned success" "0" "${PIPESTATUS[0]}"
wait_for_line "$opened" '^THP_enabled'
out=$(cat "$opened")
check "an operator's filter beside a leaked marker reaches the opened program" "1" "$(echo "$out" | grep -c '^DRIVERS_DISABLE flea-operator_icd.json$')"
check "and the leaked marker does not" "1" "$(echo "$out" | grep -c '^DISABLE_MARKER unset$')"

: > "$opened"
# An operator's own list carries no marker, so it must survive into the program they open.
VK_ICD_FILENAMES=/tmp/flea-operator-icd.json VK_DRIVER_FILES=/tmp/flea-operator-driver.json \
  VK_LOADER_DRIVERS_DISABLE=flea-operator_icd.json PATH="$D/bin:/usr/bin:/bin" $BIN --open "$D/file.txt" 2>&1 | cat >/dev/null
check "the operator-list open returned success" "0" "${PIPESTATUS[0]}"
wait_for_line "$opened" '^THP_enabled'
out=$(cat "$opened")
check "an operator's own ICD list reaches the opened program" "1" "$(echo "$out" | grep -c '^ICD /tmp/flea-operator-icd.json$')"
check "and so does their own driver file list" "1" "$(echo "$out" | grep -c '^DRIVER_FILES /tmp/flea-operator-driver.json$')"
check "and so does their own driver filter" "1" "$(echo "$out" | grep -c '^DRIVERS_DISABLE flea-operator_icd.json$')"

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
# The tear-off hand-off belongs to the one window ui/TabBar.qml starts, so nothing this window launches replays it.
FLEA_TAB_SOURCE_PID=111 FLEA_TAB_CURSOR=same.txt FLEA_TAB_TOKEN=old-lift \
  PATH="$D/bin:/usr/bin:/bin" $BIN --open "$D/file.txt" 2>&1 | cat >/dev/null
check "the tear-off open returned success" "0" "${PIPESTATUS[0]}"
wait_for_line "$opened" '^THP_enabled'
out=$(cat "$opened")
check "an opened program does not inherit the tab source pid" "TAB_PID unset" "$(echo "$out" | grep '^TAB_PID ')"
check "and not the tab cursor name" "TAB_CURSOR unset" "$(echo "$out" | grep '^TAB_CURSOR ')"
check "and not the tab lift token" "TAB_TOKEN unset" "$(echo "$out" | grep '^TAB_TOKEN ')"

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
# canonicalize already proved the file is there, so the refusal sentence says no application took it.
check "and its refusal is one sentence saying no application opened it" "1" \
  "$(echo "$out" | grep -c 'no application on this system opened that file')"
check "with no errno in it" "0" "$(echo "$out" | grep -c 'os error')"

out=$($BIN --open 2>&1 </dev/null)
rc=$?
check "--open with no path exits 2" "2" "$rc"
check "--open usage names its path and optional gallery" "1" "$(echo "$out" | grep -F -c -- '--open <path> [images...]')"

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
  check "PKGBUILD depends on $package, which ships $handoff" "1" "$( (. ./PKGBUILD && printf '%s\n' "${depends[@]}") | grep -cxF "$package")"
done

# The stub flea-gui is what exec_window launched, so it inherits huge pages off; --open must hand them back.
: > "$last_arg"
printf '#!/bin/sh\ngrep -i "^THP_enabled" /proc/self/status | sed "s/^/QS /"\nexec %s --open %s\n' "$BIN_REAL" "$D/file.txt" > "$D/bin/flea-gui"
chmod +x "$D/bin/flea-gui"
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
  printf '[ "${1-}" = "--print-cmd=\\0" ] && exit 1\n'
  printf 'printf "FD1 %%s\\n" "$(readlink /proc/$$/fd/1)" >> %q\n' "$ran"
  printf 'exec >> %q 2>&1\n' "$ran"
  printf 'printf "NARGS %%s\\n" "$#"\n'
  printf 'printf "ICD %%s\\n" "${VK_ICD_FILENAMES-unset}"\n'
  printf 'printf "DRIVER_FILES %%s\\n" "${VK_DRIVER_FILES-unset}"\n'
  printf 'printf "PIN %%s\\n" "${FLEA_VK_PIN-unset}"\n'
  printf 'printf "DRIVERS_DISABLE %%s\\n" "${VK_LOADER_DRIVERS_DISABLE-unset}"\n'
  printf 'printf "DISABLE_MARKER %%s\\n" "${FLEA_VK_DISABLE-unset}"\n'
  printf 'printf "THEME %%s\\n" "${QT_QPA_PLATFORMTHEME-unset}"\n'
  printf 'printf "ICON_THEME %%s\\n" "${QS_ICON_THEME-unset}"\n'
  printf 'printf "THEME_MARKER %%s\\n" "${FLEA_QT_THEME-unset}"\n'
  printf 'printf "TAB_PID %%s\\n" "${FLEA_TAB_SOURCE_PID-unset}"\n'
  printf 'printf "TAB_CURSOR %%s\\n" "${FLEA_TAB_CURSOR-unset}"\n'
  printf 'printf "TAB_TOKEN %%s\\n" "${FLEA_TAB_TOKEN-unset}"\n'
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
# Flea's own driver exclusion is taken back off a terminal Flea opens, the same as its pin.
VK_LOADER_DRIVERS_DISABLE=flea-absent_icd.json FLEA_VK_DISABLE=flea-absent_icd.json \
  PATH="$D/bin:/usr/bin:/bin" $BIN --terminal "$D/dir" 2>&1 | cat >/dev/null
check "the marked-exclusion terminal returned success" "0" "${PIPESTATUS[0]}"
wait_for_line "$ran" '^THP_enabled'
out=$(cat "$ran")
check "a marked driver exclusion is dropped from a terminal" "1" "$(echo "$out" | grep -c '^DRIVERS_DISABLE unset$')"
check "and its marker does not leak into the terminal" "1" "$(echo "$out" | grep -c '^DISABLE_MARKER unset$')"

: > "$ran"
# The terminal path reaches the same ownership test, so it needs the leaked-marker arm of its own.
VK_LOADER_DRIVERS_DISABLE=flea-operator_icd.json FLEA_VK_DISABLE=flea-absent_icd.json \
  PATH="$D/bin:/usr/bin:/bin" $BIN --terminal "$D/dir" 2>&1 | cat >/dev/null
check "the leaked-marker terminal returned success" "0" "${PIPESTATUS[0]}"
wait_for_line "$ran" '^THP_enabled'
out=$(cat "$ran")
check "an operator's filter beside a leaked marker reaches the terminal" "1" "$(echo "$out" | grep -c '^DRIVERS_DISABLE flea-operator_icd.json$')"
check "and the leaked marker does not reach the terminal" "1" "$(echo "$out" | grep -c '^DISABLE_MARKER unset$')"

: > "$ran"
# An operator's own list carries no marker, so it must survive into the terminal they open.
VK_ICD_FILENAMES=/tmp/flea-operator-icd.json VK_DRIVER_FILES=/tmp/flea-operator-driver.json \
  VK_LOADER_DRIVERS_DISABLE=flea-operator_icd.json PATH="$D/bin:/usr/bin:/bin" $BIN --terminal "$D/dir" 2>&1 | cat >/dev/null
check "the operator-list terminal returned success" "0" "${PIPESTATUS[0]}"
wait_for_line "$ran" '^THP_enabled'
out=$(cat "$ran")
check "an operator's own ICD list reaches the terminal" "1" "$(echo "$out" | grep -c '^ICD /tmp/flea-operator-icd.json$')"
check "and so does their own driver file list" "1" "$(echo "$out" | grep -c '^DRIVER_FILES /tmp/flea-operator-driver.json$')"
check "and so does their own driver filter in a terminal" "1" "$(echo "$out" | grep -c '^DRIVERS_DISABLE flea-operator_icd.json$')"

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
# The terminal path takes the same refusal: a tab lift replayed in a shell Flea started would move a cursor it never asked for.
FLEA_TAB_SOURCE_PID=111 FLEA_TAB_CURSOR=same.txt FLEA_TAB_TOKEN=old-lift \
  PATH="$D/bin:/usr/bin:/bin" $BIN --terminal "$D/dir" 2>&1 | cat >/dev/null
check "the tear-off terminal returned success" "0" "${PIPESTATUS[0]}"
wait_for_line "$ran" '^THP_enabled'
out=$(cat "$ran")
check "a terminal does not inherit the tab source pid" "TAB_PID unset" "$(echo "$out" | grep '^TAB_PID ')"
check "and not the tab cursor name" "TAB_CURSOR unset" "$(echo "$out" | grep '^TAB_CURSOR ')"
check "and not the tab lift token" "TAB_TOKEN unset" "$(echo "$out" | grep '^TAB_TOKEN ')"

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
check "a file opens its listing folder in a terminal" "0" "$rc"
wait_for_line "$ran" '^THP_enabled'
check "the terminal receives the file parent" "1" "$(grep -c -- "^ARGV --dir=$D$" "$ran")"

: > "$ran"
out=$(PATH="$D/bin:/usr/bin:/bin" $BIN --terminal "$D/broken" 2>&1)
rc=$?
check "a dangling link opens its listing folder in a terminal" "0" "$rc"
wait_for_line "$ran" '^THP_enabled'
check "the terminal receives the dangling link parent" "1" "$(grep -c -- "^ARGV --dir=$D$" "$ran")"

out=$(env PATH=/nonexistent-flea-test-path $BIN --terminal "$D/dir" 2>&1)
rc=$?
check "a missing xdg-terminal-exec is an error status" "2" "$rc"
check "and is elided too" "0" "$(echo "$out" | grep -c 'os error')"
# Without this the pair cannot tell a failed spawn from a --terminal that was never implemented.
check "and that sentence names the handler" "1" "$(echo "$out" | grep -c 'nothing on this system could be asked')"

out=$($BIN --terminal 2>&1 </dev/null)
check "--terminal with no path is a usage error" "1" "$(echo "$out" | grep -c -- '--terminal')"

# exec_window starts the stub flea-gui with huge pages off, so only this arm proves --terminal hands them back.
printf '#!/bin/sh\nexec %s --terminal %s\n' "$BIN_REAL" "$D/dir" > "$D/bin/flea-gui"
chmod +x "$D/bin/flea-gui"
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
check "--default refuses when its desktop entry is not installed, with the status the Settings row reads" "3" "$rc"
check "and names what is missing" "1" "$(echo "$out" | grep -c 'is not installed')"
check "and mimeapps.list is never created" "0" "$([ -e "$D/config/mimeapps.list" ] && echo 1 || echo 0)"
check "and bindings.lua is left untouched" "-- stock omarchy bindings" "$(cat "$D/config/hypr/bindings.lua")"
sandbox_remove "$D"

# At a terminal --picker restarts the portal as Settings does; piped, it leaves that to its caller.
sandbox_root_ok
D=$(mktemp -d "$SANDBOX_ROOT/flea-portal-restart-test.XXXXXX") || exit 1
: > "$D/$SANDBOX_MARKER"
mkdir -p "$D/data/xdg-desktop-portal/portals" "$D/config/hypr" "$D/bin"
: > "$D/data/xdg-desktop-portal/portals/flea.portal"
printf -- '-- stock omarchy bindings\n' > "$D/config/hypr/bindings.lua"
# Sample input: systemctl --user try-restart xdg-desktop-portal.service
printf '#!/bin/sh\nprintf "%%s\\n" "$*" >> "%s/restarts"\nexit "$(cat "%s/restart-status")"\n' "$D" "$D" > "$D/bin/systemctl"
# Only the real readiness command is accepted, and the reply and exit status belong to this fixture.
printf '#!/bin/sh\ncase "$*" in\n  "--user --auto-start=no --timeout=5s get-property org.freedesktop.portal.Desktop /org/freedesktop/portal/desktop org.freedesktop.portal.FileChooser version") ;;\n  *) exit 9 ;;\nesac\ncat "%s/probe-reply"\nexit "$(cat "%s/probe-status")"\n' "$D" "$D" > "$D/bin/busctl"
# hyprctl unreachable, so the float block is written but the operator's own Hyprland is never reloaded.
printf '#!/bin/sh\nexit 1\n' > "$D/bin/hyprctl"
chmod 755 "$D/bin/systemctl" "$D/bin/busctl" "$D/bin/hyprctl"
echo 0 > "$D/restart-status"
echo 0 > "$D/probe-status"
printf 'u 4\n' > "$D/probe-reply"
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
check "a claim with no desktop named beside a desktop file refuses" "1" "$(grep -c 'The desktop is unknown, so Flea cannot tell whether this desktop reads .*hyprland-portals.conf' <<<"$out")"
check "and exits 1" "1" "$(grep -cx 'rc=1' <<<"$out")"
check "and leaves that file as it was" "${nautilus}x" "$(cat "$shadow"; printf x)"
check "and writes no picker window rule into bindings.lua" "$binds_before" "$(cat "$D/config/hypr/bindings.lua"; printf x)"
check "and writes no portals.conf behind it" "no" "$([ -e "$D/config/xdg-desktop-portal/portals.conf" ] && echo yes || echo no)"
check "and restarts nothing" "5" "$(restarts)"
# With no desktop named and no desktop file, portals.conf is the only user file the portal can read.
[ -n "$D" ] && [[ "$D" = /* ]] && [ -f "$D/$SANDBOX_MARKER" ] && [[ "$shadow" = "$D/"* ]] && rm -- "$shadow"
out=$(script -qec "$(printf '%q ' "${no_desktop[@]}" "$BIN_REAL" --picker); echo rc=\$?" /dev/null </dev/null | tr -d '\r')
check "a claim with no desktop named and no desktop file writes portals.conf" "1" "$(grep -c 'flea;gtk, written to .*/xdg-desktop-portal/portals.conf' <<<"$out")"
check "and exits 0" "1" "$(grep -cx 'rc=0' <<<"$out")"
check "and restarts the portal" "6" "$(restarts)"
# The routing alone decides: a claim whose float block fails (no bindings.lua) still exits 1 but restarts.
[ -n "$D" ] && [[ "$D" = /* ]] && [ -f "$D/$SANDBOX_MARKER" ] && rm -- "$D/config/hypr/bindings.lua"
out=$(at_terminal "" --picker)
check "a routed claim whose window half failed names that failure" "1" "$(grep -c 'bindings.lua could not be read' <<<"$out")"
check "and still restarts the portal" "7" "$(restarts)"
check "and says file dialogs follow now" "$restarted" "$(grep '^xdg-desktop-portal ' <<<"$out")"
# An accepted restart still needs a working frontend, and a missing or unready one gets recovery commands.
printf -- '-- stock omarchy bindings\n' > "$D/config/hypr/bindings.lua"
echo 1 > "$D/probe-status"
out=$(at_terminal "" --picker)
check "a successful restart with a failed readiness call is tried once" "8" "$(restarts)"
check "and gives the restart command instead of claiming success" "$at_startup" "$(grep '^xdg-desktop-portal ' <<<"$out")"
check "and gives the journal command for the failed frontend" "1" "$(grep -c 'journalctl --user -u xdg-desktop-portal.service -b --no-pager' <<<"$out")"
echo 0 > "$D/probe-status"
printf 'u 0\n' > "$D/probe-reply"
out=$(at_terminal "" --picker)
check "a zero interface version still restarts only once" "9" "$(restarts)"
check "and is not reported as a ready frontend" "$at_startup" "$(grep '^xdg-desktop-portal ' <<<"$out")"
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

# A relative --select names a child of the launch directory, so the pair is absolute before the window sees it.
D="$FIXTURE_ROOT/flea-relselect-$$"
sandbox_make "$D"
mkdir -p "$D/deep/er"
D_REAL=$(cd "$D" && pwd -P)
out=$(cd "$D" && "$BIN_REAL" --select note.txt --print-target 2>&1)
check "select: a bare relative name resolves against the launch directory" "$D_REAL $D_REAL/note.txt" "$out"
out=$(cd "$D/deep/er" && "$BIN_REAL" --select ../x --print-target 2>&1)
check "select: a dot dot name folds into an absolute pair" "$D_REAL/deep $D_REAL/deep/x" "$out"
out=$(cd "$D/deep/er" && "$BIN_REAL" --select ./sub/note.txt --print-target 2>&1)
check "select: a dot slash name drops the dot" "$D_REAL/deep/er/sub $D_REAL/deep/er/sub/note.txt" "$out"
# A stand-in flea-gui prints the start and select arguments the window is handed.
mkdir -p "$D/bin" "$D/home" "$D/data" "$D/cache"
cat > "$D/bin/flea-gui" <<'START_STUB'
#!/bin/sh
start=""; select=unset
while [ "$#" -gt 0 ]; do
  case "$1" in
    --select) select=$2; shift 2 ;;
    --) shift; start=$1; break ;;
    *) exit 2 ;;
  esac
done
printf 'START %s\nSELECT %s\n' "$start" "$select"
START_STUB
chmod +x "$D/bin/flea-gui"
relstart_launch() {
  local cwd="$1"; shift
  (cd "$cwd" && env -u FLEA_PATH -u FLEA_SELECT HOME="$D/home" XDG_DATA_HOME="$D/data" XDG_CACHE_HOME="$D/cache" \
    WAYLAND_DISPLAY=flea-modes-test-display PATH="$D/bin:/usr/bin:/bin" "$BIN_REAL" --gui "$@" 2>"$D/relstart.err" </dev/null)
}
relstart_check() {
  check "$1" "$2" "$3"
  [ "$2" = "$3" ] || sed 's/^/  launcher stderr: /' "$D/relstart.err"
}
out=$(relstart_launch "$D/deep" .)
relstart_check "start: a dot is handed over as the launch directory" "START $D_REAL/deep
SELECT unset" "$out"
out=$(relstart_launch "$D/deep" er/../er)
relstart_check "start: a relative folder is absolute and folded" "START $D_REAL/deep/er
SELECT unset" "$out"
out=$(relstart_launch "$D/deep" "$D_REAL/deep//er/")
relstart_check "start: an absolute folder is handed over byte for byte" "START $D_REAL/deep//er/
SELECT unset" "$out"
out=$(relstart_launch "$D/deep/er" --select ../x)
relstart_check "start: a relative reveal hands over an absolute pair" "START $D_REAL/deep
SELECT $D_REAL/deep/x" "$out"
sandbox_remove "$D"

# u1-nopin: a GPUI launch leaves adapter choice to GPUI, so the launcher hands its child no driver pin.
VKD="$FIXTURE_ROOT/flea-nopin-$$"
sandbox_make "$VKD"
mkdir -p "$VKD/bin" "$VKD/cache" "$VKD/data" "$VKD/home"
cat > "$VKD/bin/flea-gui" <<'STUB'
#!/bin/sh
printf 'DRIVER_FILES %s\n' "${VK_DRIVER_FILES-unset}"
STUB
chmod +x "$VKD/bin/flea-gui"
out=$(env XDG_CACHE_HOME="$VKD/cache" XDG_DATA_HOME="$VKD/data" HOME="$VKD/home" \
  WAYLAND_DISPLAY=flea-modes-test-display PATH="$VKD/bin:/usr/bin:/bin" $BIN --gui 2>&1 </dev/null)
check "a GPUI launch reaches the GPUI child with no driver pin" "1" "$(echo "$out" | grep -c '^DRIVER_FILES unset$')"
sandbox_remove "$VKD"
sandbox_remove "$CACHE_ROOT"

[ "$fail" -eq 0 ] && echo "modes: all checks passed"
exit $fail
