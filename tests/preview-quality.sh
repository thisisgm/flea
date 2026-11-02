#!/bin/bash
# Preview decode quality, device sizing, thumbnail refinement and vector resize regression gate.
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ ${1:-} == --runner ]]; then
    shift
    root=$(mktemp -d /tmp/flea-preview-quality.XXXXXXXX)
    : > "$root/.flea-test-sandbox"
    mkdir -p "$root/config" "$root/data" "$root/cache" "$root/state" "$root/tmp"
    export XDG_CONFIG_HOME="$root/config" XDG_DATA_HOME="$root/data" XDG_CACHE_HOME="$root/cache" XDG_STATE_HOME="$root/state" TMPDIR="$root/tmp"
    trap 'case $root in /tmp/flea-preview-quality.?*) [ -f "$root/.flea-test-sandbox" ] && rm -rf -- "$root" ;; esac' EXIT
    "$@"
    exit $?
fi
# Every preview_quality test in the GUI library, the core library and the binary when the gate last grew; a renamed or dropped test fails it.
min_tests=36
logs=$(mktemp -d /tmp/flea-preview-quality-log.XXXXXXXX)
: > "$logs/.flea-test-sandbox"
trap 'case $logs in /tmp/flea-preview-quality-log.?*) [ -f "$logs/.flea-test-sandbox" ] && rm -rf -- "$logs" ;; esac' EXIT
runner="target.'cfg(all())'.runner=['/bin/bash','$PWD/tests/preview-quality.sh','--runner']"
total=0
for target in flea-gui:--lib flea:--lib flea:--bin=flea; do
    package=${target%%:*} kind=${target#*:}
    log=$logs/$package${kind//[-=]/_}.log
    cargo test --locked -p "$package" "$kind" preview_quality --config "$runner" -- --nocapture > "$log" 2>&1 || { tail -35 "$log"; exit 1; }
    cat "$log"
    # Sample input: test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 4310 filtered out; finished in 3.02s
    count=$(sed -n 's/^test result: ok\. \([0-9]*\) passed;.*/\1/p' "$log")
    total=$((total + ${count:-0}))
done
[[ $total -ge $min_tests ]] || { echo "preview-quality: $total regression tests ran, want at least $min_tests"; exit 1; }
printf 'preview-quality: PASS %s regression tests\n' "$total"
