# Defines the native Make Flea the default case; tests/ui.sh supplies the guarded fixture and IPC helpers.

# The row and the line under it as on|inert|note|role, the last two empty when off says nothing.
makedefault_state() {
    ipc settingsModel | jq -r '(map(.id) | index("makeDefault")) as $at
        | .[$at] as $row | (.[$at + 1] | if .kind == "hint" then . else {} end) as $note
        | [$row.on, ($row.inert // false), ($note.label // ""), ($note.role // "")] | map(tostring) | join("|")'
}

# The File manager fact, which states the same xdg-mime answer the box is ticked by.
makedefault_handler() {
    ipc settingsModel | jq -r '.[] | select(.label == "File manager") | .value'
}

makedefault_ran() {
    tr '\n' ';' < "$makedefault_log"
}

# The stubs' shared timeline: a query asked and answered, a run started and exited.
makedefault_events() {
    tr '\n' ';' < "$makedefault_events_log"
}

# How many portal restarts the stub swallowed, once every one of them is the switch's exact try-restart.
makedefault_restarts() {
    local log="$makedefault_restart_log"
    ! grep -q -v -x -F 'SYSTEMCTL --user try-restart xdg-desktop-portal.service' "$log" \
        || fail "makedefault: systemctl was asked something else: $(tr '\n' ';' < "$log")"
    grep -c . "$log" || true
}

# The third argument raises the 100 polls, for a state that waits on more than one process.
makedefault_wait() {
    local want="$1" what="$2" attempts="${3:-100}" got=""
    for _attempt in $(seq 1 "$attempts"); do
        got=$(makedefault_state)
        [[ "$got" == "$want" ]] && return
        sleep 0.05
    done
    fail "makedefault: $what: want '$want', got '$got'"
}

makedefault_wait_handler() {
    local want="$1" got=""
    for _attempt in $(seq 1 100); do
        got=$(makedefault_handler)
        [[ "$got" == "$want" ]] && return
        sleep 0.05
    done
    fail "makedefault: the File manager row never stated $want, it states '$got'"
}

makedefault_wait_event() {
    local want="$1"
    for _attempt in $(seq 1 100); do
        grep -q -x -F "$want" "$makedefault_events_log" && return
        sleep 0.05
    done
    fail "makedefault: the stubs never logged $want, only '$(makedefault_events)'"
}

# The stub speaks the binary's own sentences and guards the binary's own desktop-writing modes, all read from the source.
makedefault_from_source() {
    local box="$1" id refusal status skipped guarded
    # Sample input: pub const DESKTOP_ID: &str = "com.thisisgm.flea.desktop";
    id=$(grep -o 'pub const DESKTOP_ID: &str = "[^"]*"' "$repo/src/defaults.rs" | cut -d'"' -f2)
    [[ "$id" == com.thisisgm.flea.desktop ]] || fail "makedefault: src/defaults.rs claims '$id', which this case's fixture entry does not name"
    # Sample input: "flea: {} is not installed in any applications folder, so nothing was made the default. Install the package first.",
    refusal=$(sed -n '/^fn claim_from(entry: Option<PathBuf>) -> i32 {$/,/^}$/p' "$repo/src/defaults.rs" | grep -o '"flea: [^"]*"' | cut -d'"' -f2)
    [[ "$refusal" == *"{}"* && "$refusal" != *$'\n'* ]] || fail "makedefault: defaults::claim_from() has no one refusal to stub, found '$refusal'"
    # Sample input: pub const EXIT_UNINSTALLED: i32 = 3;
    status=$(grep -o 'pub const EXIT_UNINSTALLED: i32 = [0-9]*' "$repo/src/defaults.rs" | grep -o '[0-9]*$')
    [[ "$status" =~ ^[1-9][0-9]*$ ]] || fail "makedefault: src/defaults.rs names no EXIT_UNINSTALLED status, found '$status'"
    # Sample input: eprintln!("flea: no file chooser service is installed, so Flea could not become the file chooser");
    skipped=$(sed -n '/^pub(super) fn claim_both() -> i32 {$/,/^}$/p' "$repo/src/claims.rs" | grep -o 'eprintln!("flea: [^"]*")' | cut -d'"' -f2)
    [[ -n "$skipped" && "$skipped" != *$'\n'* ]] || fail "makedefault: claim_both() has no one skipped-chooser line to stub, found '$skipped'"
    # Sample input: if args.len() == 3 && args[1] == "--default" && args[2] == "off" {
    guarded=$(grep -B1 -E 'exit\((claim_both|release_both|claim_picker|chooser::release)\(\)\)' "$repo/src/main.rs" \
        | grep -o 'args\[1\] == "--[a-z0-9-]*"' | cut -d'"' -f2 | sort -u)
    grep -q -x -F -e --default <<< "$guarded" || fail "makedefault: src/main.rs names no --default mode to guard, found '$guarded'"
    printf '%s\n' "${refusal//"{}"/$id}" > "$box/refusal"
    printf '%s\n' "$status" > "$box/refusal-status"
    printf '%s\n' "$skipped" > "$box/skipped"
    printf '%s\n' "$guarded" > "$box/guarded"
}

# Launches over the stubs, or on the PATH given fourth, then hands the suite its own PATH and data dirs back.
makedefault_launch() {
    local dir="$1" box="$2" real_bin="$3" path="${4:-$2/bin:$PATH}" saved_path="$PATH" saved_dirs="${XDG_DATA_DIRS-}"
    export PATH="$path" FLEA_MAKEDEFAULT_BOX="$box" FLEA_MAKEDEFAULT_REAL="$real_bin"
    # Prepended, so the fixture entry is found whether or not a Flea package is installed on this box.
    export XDG_DATA_DIRS="$box/data:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"
    # The transport launches only this worktree's build, so the stub reaches the window as its backend through FLEA_BIN.
    local -a flea_launch_wrap=(env "FLEA_BIN=$box/bin/flea")
    launch "$dir"
    export PATH="$saved_path"
    if [[ -n "$saved_dirs" ]]; then export XDG_DATA_DIRS="$saved_dirs"; else unset XDG_DATA_DIRS; fi
    unset FLEA_MAKEDEFAULT_BOX FLEA_MAKEDEFAULT_REAL
    wait_listing 1
}

# The stubs but xdg-mime, then every other program on the suite's PATH, the first of each name winning, so xdg-mime is nowhere on it.
makedefault_path_without_mime() {
    local out="$1/nomime" dir dirs
    mkdir -p "$out"
    cp "$1/bin/flea" "$1/bin/systemctl" "$out/"
    IFS=: read -r -a dirs <<< "$PATH"
    for dir in "${dirs[@]}"; do
        [[ "$dir" == /* && -d "$dir" ]] || continue
        # ln refuses a name already there, which is what keeps the stubs and the first of every other name.
        find "$dir" -mindepth 1 -maxdepth 1 ! -name xdg-mime -print0 | xargs -0 -r ln -s -t "$out" 2>/dev/null || true
    done
    [[ -z "$(env PATH="$out" sh -c 'command -v xdg-mime' || true)" ]] || fail "makedefault: xdg-mime is still on the PATH built without it"
    [[ -n "$(env PATH="$out" sh -c 'command -v qs' || true)" ]] || fail "makedefault: the PATH built without xdg-mime has no qs to launch"
}
