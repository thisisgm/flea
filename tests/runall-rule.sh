#!/usr/bin/env bash
# Proves run-all's FAIL-line rule on a scratch copy of run-all.sh with stub suites and a stub cargo.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1
# A pipe holds 64 KB; a suite printing more than this after its FAIL line is what a loud real suite does.
PIPE_BYTES=65536
FILLER_BYTES=$((PIPE_BYTES * 2))
MARKER=.flea-test-sandbox
box=$(mktemp -d) || exit 1
: > "$box/$MARKER"
# Removes only an absolute, non-empty root that still carries its marker.
cleanup() {
    case $box in
        /?*) [ -f "$box/$MARKER" ] && rm -rf -- "$box" ;;
    esac
}
trap cleanup EXIT
python3 -B tests/headless-commons-check.py "$box" || exit 1
checks=0
bad=0
# The verdict word is reworded in every line this suite prints, so only a real failure reads as one to flea-ci.
say() { local s=$*; printf '%s\n' "${s//FAIL/failed}"; }
check() {
    checks=$((checks + 1))
    if [ "$1" = ok ]; then
        say "ok   $2"
    else
        bad=$((bad + 1))
        say "BAD  $2"
    fi
}

mkdir -p "$box/tests" "$box/bin" || exit 1
printf '#!/bin/sh\nexit 0\n' > "$box/bin/cargo"
chmod +x "$box/bin/cargo"
# Sample input, one stub per case: name|body, the body being what the stub suite runs.
stubs="stubquiet|exit 0
stubfail|echo 'FAIL x'; exit 0
stubloud|echo 'suite: FAIL x'; yes 'filler line' | head -c $FILLER_BYTES; echo; exit 0
stubctrl|printf '%s\\n' 'ok FAILED soon' 'xFAIL x' 'expected: failed x'; exit 0
stubexit1|echo boom; exit 1"
names=""
while IFS='|' read -r name body; do
    printf '#!/bin/bash\n%s\n' "$body" > "$box/tests/$name.sh"
    chmod +x "$box/tests/$name.sh"
    names="$names $name"
done <<< "$stubs"
names=${names# }
last=${names##* }
first=${names% *}

# Each shipped headless= list becomes a stub list (first: all but the last stub, middle: none, last: the final stub); rule lines stay shipped bytes.
shipped_lists=$(grep -c '^headless=' tests/run-all.sh)
seen=0
while IFS= read -r line; do
    if [[ "$line" == headless=* ]]; then
        seen=$((seen + 1))
        if [ "$shipped_lists" -eq 1 ]; then line="headless=\"$first $last\""
        elif [ "$seen" -eq 1 ]; then line="headless=\"$first\""
        elif [ "$seen" -eq "$shipped_lists" ]; then line="headless=\"\$headless $last\""
        else line="headless=\"\$headless\""
        fi
    fi
    printf '%s\n' "$line"
done < tests/run-all.sh > "$box/tests/run-all.sh"
chmod +x "$box/tests/run-all.sh"
# Every shipped list must be swapped: a shipped headless= line left in the copy would run real suites.
[ "$shipped_lists" -ge 1 ] && [ "$(grep -c '^headless=' "$box/tests/run-all.sh")" -eq "$shipped_lists" ] \
    && ! grep -qxF -f <(grep '^headless=' tests/run-all.sh) "$box/tests/run-all.sh" \
    && check ok "the copy swaps all $shipped_lists shipped headless= lists for stubs" \
    || check bad "the copy swaps all $shipped_lists shipped headless= lists for stubs"

# The scratch copy swaps every list for stubs, so the shipped lists are read here: a name with no tests/NAME.sh, such as two names run together, runs nothing.
headless=""
while IFS= read -r line; do
    [[ "$line" == headless=* ]] && eval "$line"
done < tests/run-all.sh
unrunnable=""
for name in $headless; do
    [ -x "tests/$name.sh" ] || unrunnable="$unrunnable $name"
done
[ -n "$headless" ] && [ -z "$unrunnable" ] && check ok "every name in the shipped headless lists has an executable suite" \
    || check bad "every name in the shipped headless lists has an executable suite, no runner for:${unrunnable:- (the lists are empty)}"

# Each shipped rule line must be present, once, and byte-equal in the copy.
for pattern in '^fail_line=' '\|\| printed=\$\('; do
    shipped=$(grep -E -- "$pattern" tests/run-all.sh)
    if [ -n "$shipped" ] && [ "$(printf '%s\n' "$shipped" | wc -l)" -eq 1 ] && grep -Fxq -- "$shipped" "$box/tests/run-all.sh"; then
        check ok "the copy keeps the shipped rule line $pattern"
    else
        check bad "the copy keeps the shipped rule line $pattern"
    fi
done

# flea-ci shards run-all.sh to loop over FLEA_CI_SUITES and keep each log in FLEA_CI_SUITE_LOGS; the scratch run points both at itself.
mkdir -p "$box/logs" || exit 1
out=$(env PATH="$box/bin:$PATH" FLEA_CI_SUITES="$names" FLEA_CI_SUITE_LOGS="$box/logs" bash "$box/tests/run-all.sh" 2>&1)
status=$?
# The line run-all printed for one suite.
line_of() { grep -a -E -- "^  $1 +" <<< "$out" | head -n 1; }
expect() {
    local name=$1 pattern=$2 why=$3 got
    got=$(line_of "$name")
    if grep -qE -- "$pattern" <<< "$got"; then
        check ok "$why"
    else
        check bad "$why, run-all printed: $got"
    fi
}
expect stubquiet ' ok +' "a quiet suite that exits 0 is ok"
expect stubfail ' FAIL +rc=0 but its output holds a FAIL line: FAIL x$' "a suite that exits 0 over FAIL x is counted failed"
expect stubloud ' FAIL +rc=0 but its output holds a FAIL line: suite: FAIL x$' "a loud suite (over $PIPE_BYTES bytes after its FAIL line) is counted failed"
expect stubctrl ' ok +expected: failed x$' "the non-matching controls stay ok"
expect stubexit1 ' FAIL +rc=1 +boom$' "a suite that exits 1 with no FAIL line is counted failed with rc=1"
[ "$status" -ne 0 ] && check ok "run-all exits nonzero (status $status)" || check bad "run-all exits nonzero (status $status)"
tally=$(grep -a -E '^run-all: [0-9]+ suite' <<< "$out")
grep -qE '^run-all: 5 suite\(s\) run, 3 failed$' <<< "$tally" && check ok "the tally names three failed suites" \
    || check bad "the tally names three failed suites, run-all printed: $tally"

[ "$bad" -eq 0 ] || say "$out"
printf 'runall-rule: %d checks, %d failed\n' "$checks" "$bad"
[ "$bad" -eq 0 ]
