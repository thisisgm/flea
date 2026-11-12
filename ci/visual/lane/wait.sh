# Waits for a reader substring and records a failed state when its bounded wait expires.
WAIT_IPC_TRIES=60
WAIT_IPC_STEP_SECONDS=0.1
WAIT_IPC_NOTE_CHARS=160
wait_ipc_has() {
    local reader=$1 needle=$2 got="" status=0 _i
    for _i in $(seq 1 "$WAIT_IPC_TRIES"); do
        if got=$(ipc "$reader"); then
            status=0
            [[ "$got" == *"$needle"* ]] && return 0
        else
            status=$?
        fi
        sleep "$WAIT_IPC_STEP_SECONDS"
    done
    echo "VISUAL $case_name wait $reader has=$needle got=$(cut -c1-"$WAIT_IPC_NOTE_CHARS" <<< "$got") reader_status=$status" | tee -a "$case_notes"
    mkdir -p "$shots/.state-failures"
    touch "$shots/.state-failures/$case_name"
    return 1
}
