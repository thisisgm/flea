#!/bin/bash
# Gates tools/flea-field-bench's TUI first rows: a probe launch's poll stamps the first time kitty's dump of the child's output names a fixture entry.
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

# The scratch root is a mktemp -d of its own, and its one delete is checked absolute and two deep before the trap exists.
scratch=$(mktemp -d) || exit 1
case $scratch in
  /*/*) ;;
  *) echo "FAIL: mktemp -d gave '$scratch', which is not an absolute path two components deep"; exit 1 ;;
esac
: > "$scratch/.flea-test-sandbox"
writer=""
trap '[ -n "$writer" ] && kill "$writer" 2>/dev/null; case $scratch in /?*) [ -f "$scratch/.flea-test-sandbox" ] && rm -rf -- "$scratch" ;; esac' EXIT

block=$(sed -n '/^# tui frame begin$/,/^# tui frame end$/p' "$bench")
[ -n "$block" ] || { echo "FAIL: no tui frame block in $bench"; exit 1; }
eval "$(sed -n '/^now_ms() {$/,/^}$/p' "$bench")"
eval "$block"
TUI_DUMP=$scratch/kitty.dump
TUI_NAMES=$scratch/fixture-names
printf '%s\n' some_file_name_000001.txt some_file_name_000002.txt > "$TUI_NAMES"

# A terminal setup and a chrome frame first, then the listing: only the listing's arrival may stamp the rows.
: > "$TUI_DUMP"
{ sleep 0.2; printf '\033[?1049h\033[1;1H Flea  /fixture\033[0m' >> "$TUI_DUMP"
  sleep 0.3; printf '\033[3;1H\033[38;2;1;2;3msome_file_name_000001.txt\033[0m' >> "$TUI_DUMP"; } &
writer=$!
now_ms; t0=$NOW_MS
tui_first_rows "$t0" $(( t0 + 3000 )); rc=$?
wait "$writer"; writer=""
check "the listing's arrival stamps the rows" 0 "$rc"
check "chrome before the listing does not count" yes "$([ "$FIRST_ROWS_MS" -ge 450 ] && [ "$FIRST_ROWS_MS" -lt 1500 ] && echo yes || echo no)"

# A screen that never lists: no rows by the deadline.
printf '\033[?1049h Flea  loading' > "$TUI_DUMP"
now_ms; t0=$NOW_MS
tui_first_rows "$t0" $(( t0 + 300 )); rc=$?
check "output that names no entry gives no rows by the deadline" "1 -1" "$rc $FIRST_ROWS_MS"

# The wiring: only a rows probe launch runs kitty with its dump and the poll, a measured TUI run takes the probe's rows, and the map clamps.
run_one=$(sed -n '/^run_one() {$/,/^}$/p' "$bench")
run_guarded=$(sed -n '/^run_guarded() {$/,/^}$/p' "$bench")
poll_line=$(printf '%s\n' "$run_one" | grep -n 'tui_first_rows "\$t0"' | cut -d: -f1)
map_line=$(printf '%s\n' "$run_one" | grep -n 'wait_for_window "\$class"' | cut -d: -f1)
check "the poll starts before the map wait" yes "$([ -n "$poll_line" ] && [ -n "$map_line" ] && [ "$poll_line" -lt "$map_line" ] && echo yes || echo no)"
off_line=$(printf '%s\n' "$run_one" | grep -n '^  TUI_DUMP_FLAG=""$' | cut -d: -f1)
dump_line=$(printf '%s\n' "$run_one" | grep -n 'if \[ "\$kind" = tui \] && \[ "\$RUN_MODE" = rows \]; then : > "\$TUI_DUMP"; TUI_DUMP_FLAG="--dump-bytes \$TUI_DUMP"; fi' | cut -d: -f1)
clock_line=$(printf '%s\n' "$run_one" | grep -n 'now_ms; t0=\$NOW_MS' | cut -d: -f1)
check "only a rows probe turns the dump on, emptied, before the clock" yes "$([ -n "$off_line" ] && [ -n "$dump_line" ] && [ -n "$clock_line" ] && [ "$off_line" -lt "$dump_line" ] && [ "$dump_line" -lt "$clock_line" ] && echo yes || echo no)"
check "the poll runs only in a rows probe" 1 "$(printf '%s\n' "$run_one" | grep -c '^  if \[ "\$kind" = tui \] && \[ "\$RUN_MODE" = rows \]; then$')"
check "a measured TUI run takes the probe's rows" 1 "$(printf '%s\n' "$run_one" | grep -c '^    first_rows=\$FIRST_ROWS_PROBE$')"
ret_line=$(printf '%s\n' "$run_one" | grep -n 'FIRST_ROWS_PROBE=\$first_rows$' | cut -d: -f1)
pid_line=$(printf '%s\n' "$run_one" | grep -n 'wait_for_pid "\$comm"' | cut -d: -f1)
check "a rows probe returns before anything is measured" yes "$([ -n "$ret_line" ] && [ -n "$pid_line" ] && [ "$ret_line" -lt "$pid_line" ] && echo yes || echo no)"
probe_line=$(printf '%s\n' "$run_guarded" | grep -n 'if \[ "\$2" = tui \]; then FIRST_ROWS_PROBE=-1; RUN_MODE=rows; run_one "\$@" > /dev/null; RUN_MODE=measure; fi' | cut -d: -f1)
measure_line=$(printf '%s\n' "$run_guarded" | grep -n 'run_one "\$@" >> "\$OUT"' | cut -d: -f1)
check "each TUI entrant is probed before it is measured" yes "$([ -n "$probe_line" ] && [ -n "$measure_line" ] && [ "$probe_line" -lt "$measure_line" ] && echo yes || echo no)"
probe_block=$(printf '%s\n' "$run_one" | sed -n '/^  if \[ "\$RUN_MODE" = rows \]; then$/,/^  fi$/p')
check "a rows probe ends its launch and returns" 11 "$(printf '%s\n' "$probe_block" | grep -c '^    watch_kill; clean_bench; release_run_cg$')$(printf '%s\n' "$probe_block" | grep -c '^    return 0$')"
check "a probe has a control group of its own" 1 "$(printf '%s\n' "$run_one" | grep -c '\[ "\$RUN_MODE" = rows \] && leaf="\$leaf-rows"')"
tui_lines=$(grep -c '^    "[a-z-]*|tui|' "$bench")
check "the roster reader finds TUI entrants" yes "$([ "$tui_lines" -gt 0 ] && echo yes || echo no)"
check "every TUI entrant's kitty takes the dump flag, escaped for the launch's eval" "$tui_lines" "$(grep '^    "[a-z-]*|tui|' "$bench" | grep -c -- '--class \$TUI_CLASS \\\$TUI_DUMP_FLAG ')"
check "no TUI entrant's kitty names a dump of its own" 0 "$(grep '^    "[a-z-]*|tui|' "$bench" | grep -c -- '--dump-bytes')"
check "a window that never mapped shows no rows, and rows before the map are stamped at it" 1 "$(printf '%s\n' "$run_one" | grep -c 'if \[ "\$mapped" -lt 0 \]; then first_rows=-1; elif \[ "\$first_rows" -ge 0 \] && \[ "\$mapped" -gt "\$first_rows" \]; then first_rows=\$mapped; fi')"
check "a window that mapped but listed nothing says so on stderr" 1 "$(printf '%s\n' "$run_one" | grep -c '\[ "\$first_rows" -lt 0 \] && \[ "\$mapped" -ge 0 \] && echo .*no fixture entry name reached kitty.s output.* >&2')"
check "a probe refuses a dump on disk" 1 "$(printf '%s\n' "$run_one" | grep -c '\[ -n "\$TUI_DUMP_FLAG" \] && \[ "\$TUI_CONF_FS" != tmpfs \] && fail ')"
# The launch itself: every TUI entrant's command, built and evaluated as the bench does, hands kitty the dump flag only when it is set.
launched() {
  (
    set +u
    TUI_TERM=fake_term TUI_CLASS=flea-bench-kitty TUI_CONF=$scratch FLEA_STATE=$scratch/state FLEA_BIN=/bin/flea-stub FIXTURE=/fixture
    TUI_DUMP=$scratch/kitty.dump TUI_DUMP_FLAG=""
    eval "$(sed -n '/^build_entrants() {$/,/^}$/p' "$bench")"
    WITH_ANCHOR=no; build_entrants
    setsid() { "$@"; }; nohup() { "$@"; }
    fake_term() { printf '%s\n' "$*" >> "$scratch/argv"; }
    TUI_DUMP_FLAG=$1
    for row in "${ENTRANTS[@]}"; do
      IFS='|' read -r id kind class comm token helper_comm helper_token shift_cmd <<< "$row"
      [ "$kind" = tui ] || continue
      { eval "$launch"; } &
      wait $!
    done
  )
}
# Sample input, the two run_one lines read here: '    export FLEA_FIELD_BENCH=$BENCH_TOKEN', then the launch '    eval "$shift_cmd" >/dev/null 2>&1; } &'.
launch="" next=no
while IFS= read -r line; do
  if [ "$next" = yes ]; then line=${line#"${line%%[! ]*}"}; launch=${line%"; } &"}; break; fi
  [ "$line" = '    export FLEA_FIELD_BENCH=$BENCH_TOKEN' ] && next=yes
done <<< "$run_one"
check "run_one launches the entrant from a background group of this shell" 'eval "$shift_cmd" >/dev/null 2>&1' "$launch"
: > "$scratch/argv"; launched "--dump-bytes $scratch/kitty.dump"
check "a probe's launch hands every TUI kitty the dump" "$tui_lines" "$(grep -c -- "--class flea-bench-kitty --dump-bytes $scratch/kitty.dump " "$scratch/argv")"
: > "$scratch/argv"; launched ""
check "a measured launch hands no TUI kitty a dump" "$tui_lines 0" "$(grep -c -- '--class flea-bench-kitty ' "$scratch/argv") $(grep -c -- '--dump-bytes' "$scratch/argv")"
check "the header carries first_rows_ms after rerun, then the cold memory columns and launch_pos" 1 "$(grep -c '^echo "id,run,.*,rerun,first_rows_ms,cpu_main_s,cold_pss_kb,cold_drm_kb,cold_child_pss_kb,cold_sample_after_map_ms,launch_pos" >> "\$OUT"$' "$bench")"
check "the row carries the first rows after the guard, then the cold memory values and the launch place" 1 "$(grep -c '"\$WG_CLEAN" "\$RERUN_N" "\$first_rows" "\$cpu_main_s" "\$cold_pss" "\$cold_drm" "\$cold_child_pss" "\$cold_sample_ms" "\$LAUNCH_POS"$' "$bench")"

[ "$bad" = 0 ] && echo "field-tui-frame: all passed" || echo "field-tui-frame: failures above"
exit "$bad"
