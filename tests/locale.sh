#!/usr/bin/env bash
# Runs both locale audits and requires each regression control's intended exit status.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1
parent="$(pwd -P)/.superpowers/i0"
mkdir -p "$parent" || exit 1
box=$(mktemp -d "$parent/locale-suite.XXXXXX") || exit 1
marker=.flea-test-sandbox
case $box in "$parent"/locale-suite.?*) ;; *) exit 1 ;; esac
: > "$box/$marker" || exit 1
passed=0
failed=0
serial=0
cleanup() {
    local status=$?
    if [ "$status" -ne 0 ] || [ "$failed" -ne 0 ]; then
        printf 'locale: evidence=%s\n' "$box"
        return
    fi
    case $box in
        /*) [ -n "$box" ] && [ "${box%/*}" = "$parent" ] && [ -f "$box/$marker" ] && rm -rf -- "$box" ;;
    esac
}
trap cleanup EXIT

# A caller's private locale tree must not supply this suite's extra locales.
unset LOCPATH
export LC_ALL=C.UTF-8 LANG=C.UTF-8 LANGUAGE=de:ar
mkdir -p "$box/locales" || exit 1
for name in de_DE ar_AE; do
    reason='localedef is not installed'
    if command -v localedef >/dev/null 2>&1; then
        if env -u LANGUAGE localedef --no-archive -i "$name" -f UTF-8 "$box/locales/$name.UTF-8" > "$box/$name.log" 2>&1; then
            continue
        fi
        reason='localedef could not generate the locale'
        IFS= read -r reason < "$box/$name.log" || [ -n "$reason" ] || reason='localedef returned no diagnostic'
    fi
    printf 'FAIL locale required data %s.UTF-8: %s\n' "$name" "$reason"
    failed=$((failed + 1))
done
if [ "$failed" -ne 0 ]; then
    printf 'locale: %d passed, %d failed\n' "$passed" "$failed"
    exit 1
fi
export LOCPATH="$box/locales"

run() {
    local label=$1 expected=$2 detection=$3 actual matched=1
    shift 3
    serial=$((serial + 1))
    # The broker supplies a PID namespace; each check also gets CI's empty environment, detached session and null stdin.
    setsid -w env -i PATH="$PATH" HOME="$box" LC_ALL="$LC_ALL" LANG="$LANG" LANGUAGE="${LANGUAGE-}" LOCPATH="$LOCPATH" \
        "$@" < /dev/null > "$box/check-$serial.log" 2>&1
    actual=$?
    if [ "$expected" -eq 1 ] || [ -n "$detection" ]; then
        matched=0
        [ -n "$detection" ] && grep -Fxq -- "$detection" "$box/check-$serial.log" && matched=1
    fi
    if [ "$actual" -eq "$expected" ] && [ "$matched" -eq 1 ]; then
        passed=$((passed + 1))
        printf 'ok   locale %s: exit %s\n' "$label" "$actual"
    else
        failed=$((failed + 1))
        printf 'FAIL locale %s: expected exit %s, got %s\n' "$label" "$expected" "$actual"
        [ -z "$detection" ] || printf '  expected detection: %s\n' "$detection"
        cat "$box/check-$serial.log"
    fi
}

for script in tests/locale-shell.py tests/locale-parsers.py; do
    run "$script normal" 0 "" python3 -B "$script"
    for name in de_DE ar_AE; do
        run "$script $name.UTF-8" 0 "" env LC_ALL="$name.UTF-8" LANG="$name.UTF-8" python3 -B "$script"
    done
    # Sample input: parser.add_argument('--regress-uniq', action='store_true').
    if ! python3 -B - "$script" > "$box/controls" <<'PY'
import ast
from pathlib import Path
import runpy
import sys

path = Path(sys.argv[1])
tree = ast.parse(path.read_text())
flags = sorted({arg.value for node in ast.walk(tree)
                if isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute)
                and node.func.attr == 'add_argument' for arg in node.args
                if isinstance(arg, ast.Constant) and isinstance(arg.value, str)
                and arg.value.startswith('--regress-')})
if not flags:
    raise SystemExit('FAIL locale: no regression controls found in ' + str(path))
for flag in flags:
    print(flag)
    if flag == '--regress-byte-marker':
        print(flag + ' all')
        targets = runpy.run_path(str(path.with_name('locale-shell-scope.py')))['BYTE_TOOLS']
        for target in targets:
            print(flag + ' ' + target)
PY
    then
        failed=$((failed + 1))
        printf 'FAIL locale %s control discovery\n' "$script"
        cat "$box/controls"
        continue
    fi
    while read -r flag target; do
        locale_name=C
        if [ "$flag" = --regress-decimal ]; then
            locale_name=de_DE.UTF-8
        fi
        args=("$flag")
        [ -z "$target" ] || args+=("$target")
        detection="DETECTED ${args[*]}"
        [ "$flag" != --regress-byte-marker ] || [ -n "$target" ] || detection+=' all'
        run "$script ${args[*]}" 1 "$detection" env LC_ALL="$locale_name" LANG="$locale_name" python3 -B "$script" "${args[@]}"
    done < "$box/controls"
done
# Nine dead-check mutants and their unmodified twins add eighteen checks to the original twenty-five.
expected_mutants=(continuations exports case-scope block-boundaries block-balance physical-lines fixture-scope charset gio-charset)
if python3 -B tests/locale-control-mutants.py --list > "$box/mutants"; then
    mapfile -t listed_mutants < "$box/mutants"
    valid=1
    [ "${#listed_mutants[@]}" -eq "${#expected_mutants[@]}" ] || valid=0
    for expected in "${expected_mutants[@]}"; do
        matches=0
        for actual in "${listed_mutants[@]}"; do
            [ "$actual" != "$expected" ] || matches=$((matches + 1))
        done
        [ "$matches" -eq 1 ] || valid=0
    done
    if [ "$valid" -eq 1 ]; then
        for control in "${listed_mutants[@]}"; do
            for variant in mutant twin; do
                run "$variant --regress-$control" 0 "" python3 -B tests/locale-control-mutants.py "$control" "$variant"
            done
        done
    else
        failed=$((failed + 1))
        printf 'FAIL locale mutant discovery: expected all %s controls exactly once\n' "${#expected_mutants[@]}"
        printf '  listed: <%s>\n' "${listed_mutants[*]}"
    fi
else
    failed=$((failed + 1))
    printf 'FAIL locale mutant discovery\n'
fi
run 'probe with locale variables unset' 0 'PASS mutant --regress-charset: runner exit 1; victim=ps preserves UTF-8 process arguments' \
    env -u LC_ALL -u LANG -u LOCPATH python3 -B tests/locale-control-mutants.py charset mutant
run 'parser setup with locale variables unset' 0 'PASS mutant --regress-fixture-scope: runner exit 1; victim=matched fixture sort child environment' \
    env -u LC_ALL -u LANG -u LOCPATH python3 -B tests/locale-control-mutants.py fixture-scope mutant
run 'parser setup failure with locale variables unset' 1 'FAIL mutant --regress-fixture-scope: locale setup exit 42' \
    env -u LC_ALL -u LANG -u LOCPATH python3 -B tests/locale-control-mutants.py fixture-scope mutant --fail-locale-setup
run 'i50 roster regression controls' 0 '' python3 -B tests/i18n/test_roster.py
run 'jq required for all-code plural equality' 0 '' jq --version
run 'installed Omarchy roster and references (catalog completeness is the release gate)' 0 '' \
    python3 -B tools/flea-i18n-roster --omarchy-path /usr/share/omarchy --check-installed --warn-only
printf 'locale: %d passed, %d failed\n' "$passed" "$failed"
[ "$failed" -eq 0 ]
