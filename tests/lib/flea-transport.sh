#!/bin/bash
# Shell entry points share the owned-window implementation with Python harnesses.
flea_test_helper=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/flea_transport_cli.py

flea_test_launch() {
    local path=$1 log=$2
    shift 2
    export FLEA_TEST_OWNER_PID=$$
    FLEA_TEST_WINDOW=$(python3 -B "$flea_test_helper" start "$path" --log "$log" "$@") || return $?
    export FLEA_TEST_WINDOW
    FLEA_TEST_PID=$(python3 -B "$flea_test_helper" pid "$FLEA_TEST_WINDOW") || return $?
}

flea_test_call() { python3 -B "$flea_test_helper" call "$FLEA_TEST_WINDOW" "$@"; }
flea_test_quit() { python3 -B "$flea_test_helper" quit "$FLEA_TEST_WINDOW"; }
flea_test_stop() { python3 -B "$flea_test_helper" stop "$FLEA_TEST_WINDOW"; }
flea_test_stop_all() { python3 -B "$flea_test_helper" stop-all "$FLEA_TEST_WINDOWS_ROOT"; }

flea_test_exec() {
    export FLEA_TEST_OWNER_PID=$$
    python3 -B "$flea_test_helper" exec -- "$@"
}

flea_test_running() {
    local pattern=$1 found=0 line
    while IFS= read -r line; do
        [ "$line" = "$pattern" ] || continue
        found=1
    done < <(ps -eo args=)
    if [ "$found" = 1 ]; then
        ps -eo pid=,args= | while read -r pid rest; do
            [ "$rest" = "$pattern" ] && printf '%s\n' "$pid"
        done
        return 0
    fi
    return 1
}
