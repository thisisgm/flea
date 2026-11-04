#!/bin/bash
# Measure launch to a mapped picker owned by the same launched PID, on the native host.
set -euo pipefail
: "${BIN:?absolute picker launcher}" "${MATCH:?picker command-line marker}" "${FX:?guarded fixture root}"
[[ $BIN == /* && -x $BIN && -n $MATCH ]] || exit 2
export FLEA_FIXTURE_ROOT="$FX"
. "$(dirname -- "$0")/../../../tools/flea-sandbox-guard"
sandbox_root_ok
FX=$SANDBOX_ROOT
[[ -f $FX/$SANDBOX_MARKER && ! -L $FX/$SANDBOX_MARKER ]] || sandbox_refuse "unowned fixture: $FX"
timeout_seconds=${TIMEOUT:-15}
runs=${RUNS:-5}
[[ $timeout_seconds =~ ^[1-9][0-9]?$ && $runs =~ ^[1-5]$ ]] || exit 2
sandbox_require "${FOLDER:-$FX/v/work}"
folder=$SANDBOX_PATH
[ -d "$folder" ] || exit 2
scratch=$(mktemp -d "$FX/metric.XXXXXXXX")
: > "$scratch/$SANDBOX_MARKER"
request=$(python3 -c 'import json,sys; print(json.dumps(dict(mode="open", directory=False, multiple=False, title="Metric", folder=sys.argv[1])))' "$folder")
pid=
cleanup() {
    if [ -n "$pid" ]; then
        kill -TERM "$pid" 2>/dev/null || true
        for ((n = 0; n < 50; n++)); do
            kill -0 "$pid" 2>/dev/null || break
            sleep 0.02
        done
        kill -KILL "$pid" 2>/dev/null || true
        wait "$pid" 2>/dev/null || true
        pid=
    fi
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
wait_identity() {
    local deadline=$((SECONDS + timeout_seconds)) argv
    while ((SECONDS < deadline)); do
        kill -0 "$pid" 2>/dev/null || return 1
        argv=$(tr '\0' ' ' < "/proc/$pid/cmdline" 2>/dev/null) || return 1
        [[ $argv == *"$MATCH"* ]] && return 0
        sleep 0.02
    done
    return 1
}
wait_window() {
    local deadline=$((SECONDS + timeout_seconds)) clients address
    while ((SECONDS < deadline)); do
        kill -0 "$pid" 2>/dev/null || return 1
        if clients=$(timeout 1 hyprctl -j clients 2>/dev/null); then
            address=$(python3 -c 'import json,sys; print(next((c["address"] for c in json.load(sys.stdin) if c.get("pid") == int(sys.argv[1]) and c.get("mapped") is True), ""))' "$pid" <<< "$clients") || return 1
            if [ -n "$address" ]; then
                printf '%s' "$address"
                return 0
            fi
        fi
        sleep 0.02
    done
    return 1
}
for ((i = 1; i <= runs; i++)); do
    sandbox_require "$scratch/reply-$i.json"
    reply=$SANDBOX_PATH
    start=$(date +%s%3N)
    FLEA_PICKER="$request" "$BIN" --pick "$reply" > "$scratch/launch-$i.log" 2>&1 &
    pid=$!
    wait_identity || { printf 'not-the-picker: pid=%s marker=%s\n' "$pid" "$MATCH" >&2; exit 1; }
    address=$(wait_window) || { printf 'no mapped picker window for pid=%s\n' "$pid" >&2; exit 1; }
    kill -0 "$pid" 2>/dev/null || exit 1
    printf 'run=%s pid=%s window=%s elapsed_ms=%s\n' "$i" "$pid" "$address" "$(( $(date +%s%3N) - start ))"
    cleanup
done
printf 'metric evidence: %s\n' "$scratch"
