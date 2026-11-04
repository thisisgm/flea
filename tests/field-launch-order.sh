#!/bin/bash
# Gates tools/flea-field-bench's launch rotation, pair balance over 2N runs and final launch_pos CSV column.
set -u
cd "$(dirname "$0")/.." || exit 1
bench="$PWD/tools/flea-field-bench"

bad=0
check() {
  local label="$1" expected="$2" actual="$3"
  if [ "$expected" != "$actual" ]; then
    echo "FAIL $label"; echo "  expected: $expected"; echo "  actual:   $actual"; bad=1
  else
    echo "ok   $label"
  fi
}

block=$(sed -n '/^# launch order begin$/,/^# launch order end$/p' "$bench")
[ -n "$block" ] || { echo "FAIL: no launch order block in $bench"; exit 1; }
# The block reads ENTRANTS and nothing else, so each case sets the list it needs.
eval "$block"

check "a rotation starts at place (run - 1) mod N" "b c a" "$(launch_rotation 2 a b c | paste -sd' ')"
check "the third run starts at the third place" "c a b" "$(launch_rotation 3 a b c | paste -sd' ')"
check "the fourth run reverses the first full cycle" "c b a" "$(launch_rotation 4 a b c | paste -sd' ')"
check "a kind with no entrant prints nothing" "" "$(launch_rotation 5)"

# Over 2N runs each entrant leads exactly twice, and each ordered pair leads exactly N times.
balance_case() {
  local n=$1 r i j k lead_ok=yes pair_ok=yes
  local -a ids=() order=()
  local -A first=() before=() pos=()
  for (( i = 0; i < n; i++ )); do ids+=("g$i"); done
  for (( r = 1; r <= 2 * n; r++ )); do
    mapfile -t order < <(launch_rotation "$r" "${ids[@]}")
    first[${order[0]}]=$(( ${first[${order[0]}]:-0} + 1 ))
    for (( k = 0; k < n; k++ )); do pos[${order[$k]}]=$k; done
    for (( i = 0; i < n; i++ )); do
      for (( j = 0; j < n; j++ )); do
        [ "$i" -eq "$j" ] && continue
        [ "${pos[g$i]}" -lt "${pos[g$j]}" ] && before[$i,$j]=$(( ${before[$i,$j]:-0} + 1 ))
      done
    done
  done
  for (( i = 0; i < n; i++ )); do
    [ "${first[g$i]:-0}" -eq 2 ] || lead_ok=no
    for (( j = 0; j < n; j++ )); do
      [ "$i" -eq "$j" ] && continue
      [ "${before[$i,$j]:-0}" -eq "$n" ] || pair_ok=no
    done
  done
  printf 'lead=%s pair=%s' "$lead_ok" "$pair_ok"
}
check "two entrants: each leads twice, each pair leads both ways twice over four runs" "lead=yes pair=yes" "$(balance_case 2)"
check "eight entrants: each leads twice, each pair leads both ways eight times over sixteen runs" "lead=yes pair=yes" "$(balance_case 8)"

# The kinds rotate on their own: gui places run 1 to 3, tui places run 1 to 3, and every row is listed once.
ENTRANTS=(
  "flea|gui|x|sh" "flea-anchor|gui|x|sh" "nautilus|gui|x|sh"
  "flea-tui|tui|x|sh" "yazi|tui|x|sh" "mc|tui|x|sh"
)
# The run order reads the bench's ONLY split, so each case sets ONLY and re-splits before ordering.
only_split=$(grep -E '^(set -f|ONLY_IDS=|set \+f)' "$bench")
only_has_src=$(grep '^only_has() {' "$bench")
with_only() { ONLY="$1"; eval "$only_split"; eval "$only_has_src"; }
# flea-tui runs the arm's own binary with --tui, so its presence is FLEA_BIN and never a flea on PATH.
FLEA_BIN=/bin/sh
with_only ""
run2=$(run_order 2 | while read -r i place; do printf '%s@%s ' "${ENTRANTS[$i]%%|*}" "$place"; done)
check "run 2 places each kind from 1 and lists every row" "flea-anchor@1 nautilus@2 flea@3 yazi@1 mc@2 flea-tui@3 " "$run2"
check "run order lists every ENTRANTS row once" 6 "$(run_order 1 | wc -l)"
# ONLY runs the real filter: excluded rows take no place and the kept rows are placed from 1.
with_only "nautilus yazi"
only_run2=$(run_order 2 | while read -r i place; do printf '%s@%s ' "${ENTRANTS[$i]%%|*}" "$place"; done)
check "ONLY keeps only the named rows, placed from 1 among the entrants that launch" "nautilus@1 yazi@1 " "$only_run2"
with_only ""
# Ghost's comm names no program, so it is retained for SKIP without taking a launch place.
ENTRANTS+=("ghost|gui|x|definitely-not-a-command")
ghost_run1=$(run_order 1 | while read -r i place; do printf '%s@%s ' "${ENTRANTS[$i]%%|*}" "$place"; done)
check "an absent entrant takes no place" "flea@1 flea-anchor@2 nautilus@3 ghost@- flea-tui@1 yazi@2 mc@3 " "$ghost_run1"

# The real entrant loop refuses the whole field when a selected row is absent, including a missing Strata binary, and launches nothing after it.
scratch=$(mktemp -d /tmp/flea-launch-order.XXXXXX) || exit 1
: > "$scratch/.flea-test-sandbox"
trap 'case $scratch in /tmp/?*) [ -f "$scratch/.flea-test-sandbox" ] && rm -rf -- "$scratch" ;; esac' EXIT
run_text=$(sed -n '/^# entrant runs begin$/,/^# entrant runs end$/p' "$bench")
[ -n "$run_text" ] || { echo "FAIL: no entrant run block in $bench"; exit 1; }
STRATA_BIN="$scratch/absent-strata"
run_guarded() { printf '%s@%s\n' "$1" "$LAUNCH_POS"; }
fail() { echo "FAIL: $*" >&2; exit 1; }
RUNS=3
FIELD_FRONTEND=qml
absent_case() { ENTRANTS=("$1" "present|gui|x|sh"); ( eval "$run_text" ) > "$scratch/launched" 2> "$scratch/skips"; echo "$? $(paste -sd' ' "$scratch/skips")"; }
check "a missing GUI command refuses the field" "1 FAIL: required entrant is absent: ghost" "$(absent_case "ghost|gui|x|definitely-not-a-command")"
check "a missing TUI command refuses the field" "1 FAIL: required entrant is absent: ghost-tui" "$(absent_case "ghost-tui|tui|x|definitely-not-a-command")"
check "a missing Strata binary refuses the field" "1 FAIL: Strata stable is missing: $STRATA_BIN" "$(absent_case "strata-stable|gui|x|strata")"
STRATA_NIGHTLY_BIN="$scratch/absent-nightly"
check "a missing Strata nightly refuses the field" "1 FAIL: Strata nightly is missing" "$(absent_case "strata-nightly|gui|x|strata")"
ENTRANTS=("ghost|gui|x|definitely-not-a-command" "present|gui|x|sh")
( eval "$run_text" ) > "$scratch/launched" 2> "$scratch/skips"
check "a refused field never launches the absent entrant" 0 "$(grep -c '^ghost@' "$scratch/launched")"
with_only present
eval "$run_text" > "$scratch/launched-only" 2> "$scratch/skips-only"
check "ONLY excludes absent entrants without naming or launching them" 0 "$(wc -l < "$scratch/skips-only")"
check "ONLY still launches the selected entrant in each run" 3 "$(grep -cx 'present@1' "$scratch/launched-only")"
with_only ""

# The startup check runs before any launch: only_has must exist by then, or every row is skipped unchecked.
first_call=$(grep -n '^require_selected_entrants$' "$bench" | head -1 | cut -d: -f1)
has_line=$(grep -n '^only_has() {' "$bench" | head -1 | cut -d: -f1)
check "only_has is defined before the startup check first runs" yes "$([ -n "$first_call" ] && [ -n "$has_line" ] && [ "$has_line" -lt "$first_call" ] && echo yes || echo no)"
pre_src=$(sed -n '/^require_selected_entrants() {$/,/^}$/p' "$bench")
preflight_case() { ( ENTRANTS=("$@"); eval "$pre_src"; require_selected_entrants ) 2>&1; echo "rc=$?"; }
check "the startup check refuses a missing command" "FAIL: required entrant is absent: ghost rc=1" "$(preflight_case "ghost|gui|x|definitely-not-a-command" | paste -sd' ')"
# A TUI arm whose binary exists, on a box with no flea on PATH, passes the startup check, takes a place and launches.
printf '#!/bin/sh\n' > "$scratch/flea-arm"; chmod +x "$scratch/flea-arm"
FLEA_BIN=$scratch/flea-arm
saved_path=$PATH
# Keep every other command when an installed flea shares its directory with the system tools.
PATH=$(python3 - "$scratch" <<'PY'
import os
from pathlib import Path
import sys

paths = []
for i, directory in enumerate(os.environ['PATH'].split(':')):
    source = Path(directory or '.')
    if os.access(source / 'flea', os.X_OK):
        shim = Path(sys.argv[1]) / ('no-flea-%d' % i)
        shim.mkdir()
        for entry in source.iterdir():
            if entry.name != 'flea':
                (shim / entry.name).symlink_to(entry.absolute())
        directory = str(shim)
    paths.append(directory)
print(':'.join(paths))
PY
)
check "the startup check finds flea-tui by its arm binary" "rc=0" "$(preflight_case "flea-tui|tui|x|flea")"
ENTRANTS=("flea-tui|tui|x|flea")
check "the run order places flea-tui by its arm binary" "flea-tui@1 " "$(run_order 1 | while read -r i place; do printf '%s@%s ' "${ENTRANTS[$i]%%|*}" "$place"; done)"
( eval "$run_text" ) > "$scratch/launched-tui" 2> "$scratch/skips-tui"
PATH=$saved_path
check "the run loop launches flea-tui by its arm binary" 3 "$(grep -cx 'flea-tui@1' "$scratch/launched-tui")"
FLEA_BIN=/bin/sh

# Scope the real roster and kill rows, so a valid word cannot hide a typo beside it.
entrant_src=$(sed -n '/^build_entrants() {$/,/^}$/p' "$bench")
kill_src=$(sed -n '/^declare -a KILL_TARGETS=(/,/^)/p' "$bench")
kill_comms_src=$(sed -n '/^kill_comms_for() {$/,/^}$/p' "$bench")
validate_src=$(sed -n '/^validate_only() {$/,/^}$/p' "$bench")
scope_src=$(sed -n '/^# only scope begin$/,/^# only scope end$/p' "$bench")
scope_case() {
  (
    fail() { printf 'refused: %s\n' "$*"; exit 3; }
    FLEA_UI=$scratch/ui; FLEA_BIN=$scratch/flea; FLEA_QMLCACHE=$scratch/qml; FIXTURE=$scratch
    FIELD_FRONTEND=${2:-qml}; FLEA_FRONTEND=$FIELD_FRONTEND
    # The selection check needs each selected entrant to exist, so stand-ins take their names.
    mkdir -p "$scratch/bin"; for stand_in in "$scratch/flea" "$scratch/flea-gui" "$scratch/bin/thunar"; do printf '#!/bin/sh\n' > "$stand_in"; chmod +x "$stand_in"; done
    PATH="$scratch/bin:$PATH"
    FLEA_STATE=""; TUI_CONF=""; TUI_CLASS=flea-test-kitty; TUI_TERM=kitty; STRATA_BIN=$scratch/strata; WITH_ANCHOR=no
    eval "$entrant_src"; build_entrants
    FLEA_STATE=$scratch/state; TUI_CONF=$scratch/tui
    build_entrants
    eval "$kill_src"; eval "$kill_comms_src"; eval "$validate_src"
    with_only "$1"; eval "$scope_src"
    selected=""
    for row in "${ENTRANTS[@]}"; do only_has "${row%%|*}" && selected+="${row%%|*} "; done
    printf 'targets=%s selected=%s\n' "$(printf '%s\n' "${KILL_TARGETS[@]}" | paste -sd' ')" "${selected% }"
    [[ "${ENTRANTS[0]}" = *"XDG_STATE_HOME=$FLEA_STATE "* ]] && printf 'fresh state\n'
    for row in "${ENTRANTS[@]}"; do
      if [ "${row%%|*}" = flea-tui ] && [[ "$row" = *"XDG_CONFIG_HOME=$TUI_CONF "* ]]; then printf 'fresh config\n'; fi
    done
  )
}
out=$(scope_case flea-anchor); rc=$?
check "ONLY=flea-anchor without an anchor tree is refused" 3 "$rc"
check "the missing anchor keeps its exact refusal message" "refused: ONLY names flea-anchor but ANCHOR_REPO is not a separate tree, so that arm would never launch" "$out"
out=$(scope_case "flea thunr"); rc=$?
check "ONLY=flea thunr refuses the whole selection" 3 "$rc"
check "the ONLY refusal names the unknown word" "refused: ONLY names unknown entrant 'thunr'" "$out"
out=$(scope_case "flea thunar"); rc=$?
check "ONLY=flea thunar scopes both QML entrants and thunar's helper" "targets=qs|$scratch/ui flea|--backend thunar| tumblerd| selected=flea thunar
fresh state
fresh config" "$out"
check "a valid multi-word ONLY exits zero" 0 "$rc"
out=$(scope_case "flea thunar" gpui); rc=$?
check "ONLY=flea thunar scopes the GPUI child and thunar's helper" "targets=flea-gui|$scratch/flea-gui flea|--backend thunar| tumblerd| selected=flea thunar
fresh state
fresh config" "$out"
check "a valid GPUI multi-word ONLY exits zero" 0 "$rc"

# Run the capability tool's own roster read, so the earlier builder remains visible to its caller.
cap_src=$(sed -n '/^# capability roster begin$/,/^# capability roster end$/p' tools/flea-bench-capability)
[ -n "$cap_src" ] || { echo "FAIL: no capability roster block"; exit 1; }
cap_case() {
  (
    FLEA_REPO=$PWD; FLEA_UI=$scratch/ui; FLEA_BIN=$scratch/flea; STRATA_BIN=$scratch/strata; FIXTURE=$scratch; FLEA_STATE=$scratch/state
    FLEA_QMLCACHE=$scratch/qmlcache
    eval "$cap_src"
    gui=0
    for row in "${CAP_ROWS[@]}"; do [[ "$row" = *'|gui|'* ]] && gui=$(( gui + 1 )); done
    printf 'gui=%s' "$gui"
    [[ "${CAP_ROWS[0]:-}" = *"FLEA_UI=$FLEA_UI "* ]] && printf ' ui expanded'
    [[ "${CAP_ROWS[0]:-}" = *"XDG_STATE_HOME=$FLEA_STATE "* ]] && printf ' state expanded'
    [[ "${CAP_ROWS[0]:-}" = *"QML_DISK_CACHE_PATH=$FLEA_QMLCACHE "* ]] && printf ' cache expanded'
    for row in "${CAP_ROWS[@]}"; do
      if [[ "$row" = *'|gui|'* && "$row" = *'$'* ]]; then printf ' unresolved variable'; fi
    done
  )
}
check "the capability reader retains every GUI row and expands its paths" "gui=8 ui expanded state expanded cache expanded" "$(cap_case)"

# The CSV: the header ends with launch_pos, and the row format writes one value per header column.
header=$(grep -m1 '^echo "id,run,' "$bench" | sed 's/^echo "//; s/" >> .*//')
check "the header ends with launch_pos after the cold sample columns" "cold_sample_after_map_ms,launch_pos" "$(printf '%s' "$header" | rev | cut -d, -f1-2 | rev)"
check "the header retains the TUI first rows column" 1 "$(printf '%s' "$header" | tr ',' '\n' | grep -cx first_rows_ms)"
cols=$(printf '%s\n' "$header" | tr ',' '\n' | grep -c .)
fmt=$(sed -n "s/^  printf '\([^']*\)' \"\\\$id\".*/\1/p" "$bench" | head -1)
fmt_cols=$(printf '%s' "$fmt" | grep -o '%s' | grep -c .)
check "the row format writes one field per header column" "$cols" "$fmt_cols"
check "the row format passes launch_pos as its last value" 1 "$(grep -cF '"$cold_sample_ms" "$LAUNCH_POS"' "$bench")"
check "each run reads launch_pos from the run order" 1 "$(grep -cF 'read -r i LAUNCH_POS <<< "$line"' "$bench")"

# Printed for the report: the place each entrant holds, two and eight entrants over five and ten runs.
show() {
  local n=$1 runs=$2 r i
  local -a ids=()
  for (( i = 0; i < n; i++ )); do ids+=("e$i"); done
  for (( r = 1; r <= runs; r++ )); do
    printf '  run %2d:' "$r"
    launch_rotation "$r" "${ids[@]}" | while read -r id; do printf ' %s' "$id"; done
    printf '\n'
  done
}
ENTRANTS=()
echo "order example, 2 entrants, 5 runs:"; show 2 5
echo "order example, 2 entrants, 10 runs:"; show 2 10
echo "order example, 8 entrants, 5 runs:"; show 8 5
echo "order example, 8 entrants, 10 runs:"; show 8 10

[ "$bad" = 0 ] && echo "field-launch-order: all passed" || echo "field-launch-order: failures above"
exit "$bad"
