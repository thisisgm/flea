#!/bin/bash
# Sourced over the shared transport by cases whose launch target is a double: spawns the --binary given with its --ui, wrap prefix kept.
flea_test_launch() {
    local start_path="$1" flea_log="$2" ui="${flea_ui-}" bin="${flea_bin-}" arg
    shift 2
    local -a wrap=()
    while [ $# -gt 0 ]; do
        arg=$1
        case "$arg" in
            --ui|--binary)
                [ $# -ge 2 ] || fail "fixture launch got $arg with no value"
                if [ "$arg" = --ui ]; then ui=$2; else bin=$2; fi
                shift ;;
            --ui=*) ui=${arg#--ui=} ;;
            --binary=*) bin=${arg#--binary=} ;;
            --wrap=*) wrap+=("${arg#--wrap=}") ;;
            *) fail "fixture launch takes only --ui, --binary and --wrap, got $arg" ;;
        esac
        shift
    done
    [ -n "$bin" ] || fail "fixture launch has no binary"
    FLEA_UI="$ui" FLEA_BIN="$bin" ${wrap[@]+"${wrap[@]}"} \
        setsid nohup "$bin" --gui "$start_path" >"$flea_log" 2>&1 </dev/null &
    FLEA_TEST_PID=$!
}
