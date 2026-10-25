#!/bin/bash
# Gates the write guard in tools/flea-field-bench on fixture /proc files with sync and sleep stubbed; NIGHT_LIB=<path> adds the night kit's copy.
set -u
cd "$(dirname "$0")/.." || exit 1
repo=$PWD
bench="$repo/tools/flea-field-bench"

# The fixture root is a mktemp -d of its own, and its one delete is checked absolute and two deep before the trap exists.
scratch=$(mktemp -d) || exit 1
case $scratch in
  /*/*) ;;
  *) echo "FAIL: mktemp -d gave '$scratch', which is not an absolute path two components deep"; exit 1 ;;
esac
trap 'rm -rf "$scratch"' EXIT

fail=0
check() {
  local label="$1" expected="$2" actual="$3"
  if [ "$expected" != "$actual" ]; then
    echo "FAIL $label"; echo "  expected: $expected"; echo "  actual:   $actual"; fail=1
  else
    echo "ok   $label"
  fi
}

block=$(sed -n '/^# write guard begin$/,/^# write guard end$/p' "$bench")
[ -n "$block" ] || { echo "FAIL: no write guard block in $bench"; exit 1; }

export FIELD_MEMINFO="$scratch/meminfo" FIELD_VMSTAT="$scratch/vmstat"
BASE=1000000
SYNCS=0
ORDER=""
SLEPT=""
WIN_ADD=0
# Stubs: the window is where the case's writes land, and sync is counted, so the guard is shown to call both.
sync() { SYNCS=$(( SYNCS + 1 )); ORDER="$ORDER sync"; }
sleep() { SLEPT=$1; ORDER="$ORDER sleep"; set_vmstat $(( BASE + WIN_ADD )); }

set_meminfo() {
  printf 'MemTotal:       32768000 kB\nDirty:  %12s kB\nWriteback:  %8s kB\nWritebackTmp:          0 kB\n' "$1" "$2" > "$FIELD_MEMINFO"
}
set_vmstat() {
  printf 'pgpgin 77\npgpgout %s\npswpin 0\n' "$1" > "$FIELD_VMSTAT"
}

# case_row DIRTY WRITEBACK WINDOW_KB LAUNCH_KB: drives the guard the way run_one does, in this shell, and prints its cells.
case_row() {
  WIN_ADD=$3; SYNCS=0; SLEPT=""; ORDER=""
  set_meminfo "$1" "$2"; set_vmstat "$BASE"
  write_guard_pre || { echo "pre-unreadable"; return 1; }
  set_vmstat $(( BASE + $3 + $4 ))
  write_guard_launch_end || { echo "end-unreadable"; return 1; }
  printf 'dirty_wb=%s pre=%s launch=%s write_clean=%s' "$WG_DIRTY_WB_KB" "$WG_PGPGOUT_PRE_KB" "$WG_PGPGOUT_LAUNCH_KB" "$WG_CLEAN"
}

eval "$block"

# name|dirty|writeback|window kB|launch kB|expected write_clean. The first three are the brief's; the rest pin each limit.
cases='clean|300|12|100|512|1
dirty|90000|4000|0|0|0
writing|100|0|65536|0|0
launch-writing|100|0|0|20000|1
dirty at the limit|16000|384|0|0|1
dirty over the limit|16000|385|0|0|0
window at the limit|0|0|1024|0|1
window over the limit|0|0|1025|0|0
launch writes are kept, not judged|0|0|0|8193|1'
population=0
while IFS='|' read -r name dirty wb win launch want; do
  population=$(( population + 1 ))
  got=$(case_row "$dirty" "$wb" "$win" "$launch")
  # The classification is printed with its numbers, so a reader sees what was judged.
  check "$name: $got" "$want" "${got##*write_clean=}"
done <<EOF
$cases
EOF
echo "population: $population cases classified"

case_row 300 12 100 512 > /dev/null
check "the guard syncs once, then opens the window" " sync sleep" "$ORDER"
check "the window is 0.2 s" 0.2 "$SLEPT"

set_meminfo 300 12; set_vmstat "$BASE"
write_guard_pre
check "an unreadable meminfo is refused, not read as clean" 1 "$(FIELD_MEMINFO=$scratch/missing write_guard_pre; echo $?)"
printf 'MemTotal: 1 kB\n' > "$FIELD_MEMINFO"
check "a meminfo with no Dirty line is refused" 1 "$(write_guard_pre; echo $?)"
set_meminfo 300 12; printf 'pgpgin 1\n' > "$FIELD_VMSTAT"
check "a vmstat with no pgpgout line is refused" 1 "$(write_guard_pre; echo $?)"

# Positive control: a mutated threshold must turn the clean case red, one threshold at a time.
mutants=0
for pair in WRITE_DIRTY_WB_MAX_KB=100 WRITE_PRE_PGPGOUT_MAX_KB=50; do
  mutants=$(( mutants + 1 ))
  got=$(
    eval "$(printf '%s\n' "$block" | sed "s/^${pair%%=*}=[0-9]*\$/$pair/")"
    case_row 300 12 100 512
  )
  check "control, $pair: $got" 0 "${got##*write_clean=}"
done
echo "population: $mutants mutants, each run on the clean case"

# The columns: the CSV header, the row printf and the guard's cells agree, so a column cannot drift.
header=$(sed -n 's/^echo "\(id,run,[^"]*\)".*/\1/p' "$bench")
check "the guard columns precede the TUI, cold memory and launch place columns" "dirty_wb_kb,pgpgout_pre_kb,pgpgout_launch_kb,write_clean,rerun,first_rows_ms,cpu_main_s,cold_pss_kb,cold_drm_kb,cold_child_pss_kb,cold_sample_after_map_ms,launch_pos" "${header##*,cpu_sys_s,}"
row_line=$(grep -F '"$id" "$RUN_INDEX" "$mapped"' "$bench")
fields=$(printf '%s\n' "$header" | tr ',' '\n' | wc -l)
conversions=$(printf '%s\n' "${row_line%%\\n\'*}" | grep -o '%s' | wc -l)
args=$(printf '%s\n' "${row_line#*\\n\' }" | grep -o '"\$[A-Za-z_]*"' | wc -l)
check "the row printf has one conversion per header column" "$fields" "$conversions"
check "and one argument per header column" "$fields" "$args"
run_one_src=$(sed -n '/^run_one() {$/,/^}$/p' "$bench")
wiring=$(printf '%s\n' "$run_one_src" | grep -nF -e 'write_guard_pre ||' -e 'write_guard_launch_end ||' -e 'ranked=$(write_guard_ranked "$ranked")' -e '"$id" "$RUN_INDEX" "$mapped"' | cut -d: -f2- | sed 's/^ *//' | cut -c1-24 | tr '\n' '|')
check "run_one calls the guard, then ranks, then prints the row, in that order" 'write_guard_pre || fail |write_guard_launch_end ||ranked=$(write_guard_ran|printf '"'"'%s,%s,%s,%s,%s,%|' "$wiring"
guard_args='"$WG_DIRTY_WB_KB" "$WG_PGPGOUT_PRE_KB" "$WG_PGPGOUT_LAUNCH_KB" "$WG_CLEAN" "$RERUN_N" "$first_rows" "$cpu_main_s" "$cold_pss" "$cold_drm" "$cold_child_pss" "$cold_sample_ms" "$LAUNCH_POS"'
check "the row ends with guard, TUI, cold memory and launch place arguments" "$guard_args" "$(case $row_line in *"$guard_args") printf '%s' "$guard_args" ;; esac)"

# The loop that runs an entrant, taken from the bench itself, with run_one replaced by one that drives the guard.
eval "$(sed -n '/^WRITE_RERUNS=/p' "$bench")"
eval "$(sed -n '/^write_guard_ranked() {/,/^}/p; /^write_guard_summary() {/,/^}/p' "$bench")"
eval "$(sed -n '/^run_guarded() {$/,/^}$/p' "$bench")"
declare -F run_guarded > /dev/null || { echo "FAIL: run_guarded is not in the bench"; exit 1; }
RERUN_N=0
WG_CLEAN=""
# STUB_SEQ is the outcome of each attempt in turn: clean, dirty (writes before the launch), launch (writes during it) or error.
STUB_SEQ=""
run_one() {
  local outcome win=0 launch=0 ranked=yes
  set -- $STUB_SEQ
  outcome=${!#}
  [ "$(( RERUN_N + 1 ))" -le "$#" ] && eval "outcome=\${$(( RERUN_N + 1 ))}"
  case $outcome in
    error) printf '%s,%s,ERROR,no-process\n' "$id" "$RUN_INDEX"; return 1 ;;
    dirty) win=65536 ;;
    launch) launch=20000 ;;
  esac
  WIN_ADD=$win; SYNCS=0
  set_meminfo 300 12; set_vmstat "$BASE"
  write_guard_pre
  set_vmstat $(( BASE + win + launch ))
  write_guard_launch_end
  ranked=$(write_guard_ranked "$ranked")
  printf '%s,%s,%s,%s,%s,%s,%s\n' "$id" "$RUN_INDEX" "$ranked" "$WG_DIRTY_WB_KB" "$WG_PGPGOUT_PRE_KB" "$WG_PGPGOUT_LAUNCH_KB" "$WG_CLEAN,$RERUN_N"
}
# The stub's own header, in the order its row is printed, so the summary reads write_clean by name.
stub_header="id,run,ranked,dirty_wb_kb,pgpgout_pre_kb,pgpgout_launch_kb,write_clean,rerun"

# drive SEQ: runs the bench's loop for one entrant and leaves the CSV in $OUT.
drive() {
  STUB_SEQ=$1
  OUT="$scratch/out.csv"; id=flea; RUN_INDEX=1
  kind=gui; class=c; comm=c; token=t; helper_comm=""; helper_token=""; cmd=x
  echo "$stub_header" > "$OUT"
  run_guarded "$id" "$kind" "$class" "$comm" "$token" "$helper_comm" "$helper_token" "$cmd" > "$scratch/loop.log" 2> "$scratch/loop.err"
}
# Field $2 of data row $1 (row 1 is the first line after the header), read in plain bash.
cell() {
  local -a h f
  local i c=0 n=0 line=
  { IFS=, read -r -a h; while [ "$n" -lt "$1" ] && IFS= read -r line; do n=$(( n + 1 )); done; } < "$OUT"
  for i in "${!h[@]}"; do [ "${h[$i]}" = "$2" ] && c=$(( i + 1 )); done
  [ "$c" -gt 0 ] || { printf '\n'; return; }
  IFS=, read -r -a f <<< "$line"
  printf '%s\n' "${f[$(( c - 1 ))]}"
}

drive "clean"
check "a clean launch writes one row" 1 "$(( $(wc -l < "$OUT") - 1 ))"
check "and is not repeated" 0 "$(cell 1 rerun)"

drive "dirty clean"
check "a contaminated launch is repeated: two rows" 2 "$(( $(wc -l < "$OUT") - 1 ))"
check "the contaminated row stays in the CSV with write_clean 0" 0 "$(cell 1 write_clean)"
check "and carries its numbers" 65536 "$(cell 1 pgpgout_pre_kb)"
check "and does not rank" "unranked: writes in flight (dirty_wb 312 kB pgpgout_pre 65536 kB pgpgout_launch 0 kB)" "$(cell 1 ranked)"
check "the repeat is rerun 1" 1 "$(cell 2 rerun)"
check "and is clean and ranked" "1 yes" "$(cell 2 write_clean) $(cell 2 ranked)"

drive "dirty dirty dirty"
check "a launch that stays contaminated stops after two repeats: three rows" 3 "$(( $(wc -l < "$OUT") - 1 ))"
check "their rerun cells are 0, 1 and 2" "0 1 2" "$(cell 1 rerun) $(cell 2 rerun) $(cell 3 rerun)"
check "and all three stay in the CSV with write_clean 0" "0 0 0" "$(cell 1 write_clean) $(cell 2 write_clean) $(cell 3 write_clean)"
check "the loop names the launch it gave up on" 1 "$(grep -c '^WRITE-DIRTY: flea run 1' "$scratch/loop.err")"

drive "launch clean"
check "writes during the launch are kept in the row and do not repeat it" "1 1 20000" "$(( $(wc -l < "$OUT") - 1 )) $(cell 1 write_clean) $(cell 1 pgpgout_launch_kb)"

drive "error"
check "a launch that never started is not repeated" 1 "$(( $(wc -l < "$OUT") - 1 ))"

drive "dirty dirty dirty"
summary=$(write_guard_summary "$OUT" 2>&1)
check "the summary counts only write_clean 1 rows and names the rest" "write guard: flea counts 0 of 3 rows, left out 3" "$summary"
drive "dirty clean"
check "the summary says how many a clean repeat left out" "write guard: flea counts 1 of 2 rows, left out 1" "$(write_guard_summary "$OUT" 2>&1)"
check "a CSV with no write_clean column counts nothing and says so" 1 "$(printf 'id,run\nflea,1\n' > "$scratch/old.csv"; write_guard_summary "$scratch/old.csv" > /dev/null 2>&1; echo $?)"

# The night kit's copy: the same block byte for byte, and its JSON keys.
night=${NIGHT_LIB:-}
if [ -z "$night" ]; then
  echo "note night-lib.sh is not in this tree, so its copy was not compared (NIGHT_LIB=<path> compares it)"
elif [ ! -f "$night" ]; then
  echo "FAIL NIGHT_LIB=$night is not a file"; fail=1
else
  check "night-lib.sh holds the same guard block, byte for byte" "$block" "$(sed -n '/^# write guard begin$/,/^# write guard end$/p' "$night")"
  night_root="$scratch/night"
  mkdir -p "$night_root/steps" "$night_root/out"
  : > "$night_root/.flea-test-sandbox"
  got=$(
    ROOT=$night_root
    . "$night"
    sync() { :; }
    sleep() { set_vmstat $(( BASE + WIN_ADD )); }
    WIN_ADD=65536; set_meminfo 300 12; set_vmstat "$BASE"
    pre=$(night_guard_pre_json)
    base=$(printf '%s' "$pre" | python3 -c 'import json,sys; print(json.load(sys.stdin)["pgpgout_base_kb"])')
    pc=$(printf '%s' "$pre" | python3 -c 'import json,sys; print(json.load(sys.stdin)["pre_clean"])')
    set_vmstat $(( base + 100 ))
    end=$(night_guard_end_json "$base" "$pc" 0)
    set_vmstat "$BASE"
    night_guard_step contaminated-step
    printf '%s\n%s\n' "$pre" "$end"
    cat "$ROOT/out/guard.jsonl"
  )
  check "night pre keys are valid JSON and call the window contaminated" "0 65536" "$(printf '%s\n' "$got" | sed -n 1p | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d["pre_clean"], d["pgpgout_pre_kb"])')"
  check "night end keys carry write_clean 0 and the launch kB" "0 100" "$(printf '%s\n' "$got" | sed -n 2p | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d["write_clean"], d["pgpgout_launch_kb"])')"
  check "a contaminated night step is still written to guard.jsonl" "contaminated-step 0" "$(printf '%s\n' "$got" | sed -n 3p | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d["step"], d["pre_clean"])')"
fi

if [ "$fail" = 0 ]; then echo "field-write-guard: all passed"; else echo "field-write-guard: FAILED"; fi
exit "$fail"
