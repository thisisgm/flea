# GPUI override for the backend-death case: the oracle body, plus the one diagnostic line it causes on purpose.
case_terminal() {
    local dir="$fixture_root/terminal"
    sandbox_scratch "$dir"
    : > "$dir/only.txt"
    launch "$dir"
    wait_listing 1
    local qs_pid backend_pid
    qs_pid=$(flea_pid)
    backend_pid=$(pgrep -P "$qs_pid" -x flea)
    [[ -n "$backend_pid" ]] || fail "no flea backend child of qs $qs_pid"
    printf 'TERMINAL qs=%s backend=%s before state=%s total=%s row=%q\n' \
        "$qs_pid" "$backend_pid" "$(ipc state)" "$(ipc total)" "$(ipc rowAt 0)"
    # flea-gui states a pane error on stderr as well as in the pane, so the death this case causes is registered once.
    printf '%s\n' 'flea-gui: The backend closed its reply stream' >> "$expected_warnings"
    kill "$backend_pid"
    flea_ipc_wait -p "$flea_ui/boot" flea state error --timeout 15 >/dev/null \
        || fail "the pane stayed in state '$(ipc state)' after the backend died"
    [[ "$(ipc total)" == "0" ]] || fail "the stale total $(ipc total) survived the backend"
    [[ "$(ipc rowAt 0)" == "loading" ]] || fail "the stale row $(ipc rowAt 0) survived the backend"
    [[ -n "$(ipc stateMessage)" ]] || fail "no recovery sentence after the backend died"
    sleep "$transient_clear_s"
    printf 'TERMINAL after state=%s total=%s row=%q message=%q transient=%q\n' \
        "$(ipc state)" "$(ipc total)" "$(ipc rowAt 0)" "$(ipc stateMessage)" "$(ipc lastMessage)"
    shot terminal-after-death
    [[ "$(ipc state)" == "error" ]] || fail "the error state did not outlive the transient timer"
    [[ -n "$(ipc stateMessage)" ]] || fail "the recovery sentence did not outlive the transient timer"
    [[ "$(ipc total)" == "0" ]] || fail "the total came back after the transient timer"
    assert_window
}
