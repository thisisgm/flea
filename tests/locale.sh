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
    "$@" > "$box/check-$serial.log" 2>&1
    actual=$?
    if [ "$expected" -eq 1 ]; then
        matched=0
        [ -n "$detection" ] && grep -Fxq -- "$detection" "$box/check-$serial.log" && matched=1
    fi
    if [ "$actual" -eq "$expected" ] && [ "$matched" -eq 1 ]; then
        passed=$((passed + 1))
        printf 'ok   locale %s: exit %s\n' "$label" "$actual"
    else
        failed=$((failed + 1))
        printf 'FAIL locale %s: expected exit %s, got %s\n' "$label" "$expected" "$actual"
        [ "$expected" -ne 1 ] || printf '  expected detection: %s\n' "$detection"
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
printf 'locale: %d passed, %d failed\n' "$passed" "$failed"
[ "$failed" -eq 0 ]
