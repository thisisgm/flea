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

# Stubbed at flea's desktop-writing modes, xdg-mime and systemctl, each answering only the exact calls this row makes and
# refusing the rest into refused.log, so the box follows only the stub's handler file and nothing touches the operator's
# mimeapps.list, bindings or portal.
case_makedefault() {
    local dir="$fixture_root/makedefault" box="$fixture_root/makedefault-box" real_bin="$flea_bin"
    sandbox_scratch "$dir"
    sandbox_scratch "$box"
    : > "$dir/a.txt"
    mkdir -p "$box/bin" "$box/data/applications"
    local queries="$box/queries.log" handler="$box/handler" mode="$box/mode" restart="$box/restart" refused="$box/refused.log" runs
    local no_key='type == "object" and ([.. | objects | has("makeDefault")] | any | not)'
    makedefault_log="$box/ran.log"
    makedefault_restart_log="$box/systemctl.log"
    makedefault_events_log="$box/events.log"
    : > "$makedefault_log"
    : > "$queries"
    : > "$makedefault_restart_log"
    : > "$makedefault_events_log"
    : > "$refused"
    printf 'org.gnome.Nautilus.desktop\n' > "$handler"
    printf 'ok\n' > "$mode"
    printf 'ok\n' > "$restart"
    makedefault_from_source "$box"
    # The row looks for the packaged entry on the XDG data ladder before it offers a claim, as flea --default does.
    printf '[Desktop Entry]\nType=Application\nName=Flea\nExec=flea %%U\n' > "$box/data/applications/com.thisisgm.flea.desktop"

    # The two --default shapes are answered, other argv of a desktop-writing mode is refused, and the rest runs the real binary.
    cat > "$box/bin/flea" <<'STUB'
#!/bin/sh
# Sample input: flea --default off
box="$FLEA_MAKEDEFAULT_BOX"
if [ "$#" -eq 1 ] && [ "$1" = --default ]; then
    shape=claim
elif [ "$#" -eq 2 ] && [ "$1" = --default ] && [ "$2" = off ]; then
    shape=release
elif [ "$#" -gt 0 ] && grep -q -x -F -e "$1" "$box/guarded"; then
    printf 'REFUSED flea %s\n' "$*" >> "$box/refused.log"
    exit 64
else
    exec "$FLEA_MAKEDEFAULT_REAL" "$@"
fi
printf 'RAN %s\n' "$*" >> "$box/ran.log"
printf 'RAN %s\n' "$*" >> "$box/events.log"
trap 'echo EXITED >> "$box/events.log"' EXIT
# Long enough to read the working state and to press again while it lasts.
sleep 2
mode=$(cat "$box/mode")
case "$mode" in
    refuse) cat "$box/refusal" >&2; exit "$(cat "$box/refusal-status")" ;;
    fail) echo "flea: xdg-mime default exited 0 but inode/directory still resolves to org.gnome.Nautilus.desktop" >&2; exit 1 ;;
esac
if [ "$shape" = claim ]; then
    echo com.thisisgm.flea.desktop > "$box/handler"
    [ "$mode" != partly ] || cat "$box/skipped" >&2
    echo "undo both with: flea --default off"
else
    echo org.gnome.Nautilus.desktop > "$box/handler"
fi
STUB
    chmod +x "$box/bin/flea"
    cat > "$box/bin/xdg-mime" <<'STUB'
#!/bin/sh
# Sample input: xdg-mime query default inode/directory
box="$FLEA_MAKEDEFAULT_BOX"
if [ "$#" -ne 3 ] || [ "$1" != query ] || [ "$2" != default ] || [ "$3" != inode/directory ]; then
    printf 'REFUSED xdg-mime %s\n' "$*" >> "$box/refused.log"
    exit 1
fi
echo query >> "$box/queries.log"
answer=$(cat "$box/handler")
echo ASKED >> "$box/events.log"
# A held query answers what the handler was when it was asked, once the case opens the gate or 20 s have passed.
waited=0
while [ -e "$box/hold" ] && [ ! -e "$box/gate" ] && [ "$waited" -lt 400 ]; do
    sleep 0.05
    waited=$((waited + 1))
done
echo "ANSWERED $answer" >> "$box/events.log"
echo "$answer"
STUB
    chmod +x "$box/bin/xdg-mime"
    # Every call is logged and only the exact portal restart is answered, so no argv reaches the operator's systemd.
    cat > "$box/bin/systemctl" <<'STUB'
#!/bin/sh
# Sample input: systemctl --user try-restart xdg-desktop-portal.service
printf 'SYSTEMCTL %s\n' "$*" >> "$FLEA_MAKEDEFAULT_BOX/systemctl.log"
[ "$*" = "--user try-restart xdg-desktop-portal.service" ] || exit 1
[ "$(cat "$FLEA_MAKEDEFAULT_BOX/restart")" = ok ]
STUB
    chmod +x "$box/bin/systemctl"

    makedefault_launch "$dir" "$box" "$real_bin"

    settings_open_key
    settle
    settings_section about
    makedefault_wait_handler org.gnome.Nautilus.desktop
    # The entry probe runs beside the handler read, and a miss would grey the row after this point.
    settle
    makedefault_wait "false|false||" "About did not open unticked and live over a Nautilus handler"
    ipc settingsModel | jq -e '(map(.id) | index("makeDefault")) as $at | .[$at - 1].label == "File manager" and .[$at].caption == null' >/dev/null \
        || fail "makedefault: the row is not directly under the File manager fact with no caption, got $(ipc settingsModel | jq -c 'map(.label)')"
    shot makedefault-off

    # Keyboard: Space claims, and a second Space while the run lasts runs nothing.
    settings_focus_row makeDefault
    key -k space >/dev/null
    makedefault_wait "false|true|Making Flea the default|muted" "Space did not start the claim"
    key -k space >/dev/null
    makedefault_wait "true|false|Folders, Show in folder and file dialogs open Flea.|foreground" "the claim did not settle ticked"
    [[ "$(makedefault_ran)" == "RAN --default;" ]] \
        || fail "makedefault: the stub ran '$(makedefault_ran)', so a press during the run was not ignored"
    makedefault_wait_handler com.thisisgm.flea.desktop
    [[ "$(makedefault_restarts)" == 1 ]] || fail "makedefault: a claim that went through did not restart the portal once"
    [[ "$(grep -c . "$queries")" == 2 ]] \
        || fail "makedefault: xdg-mime was asked $(grep -c . "$queries") times, so the handler was not read again after the run"
    ipc settingsModel | jq -e '(map(.id) | index("makeDefault")) as $at | .[$at + 1].elide == "right"' >/dev/null \
        || fail "makedefault: the note is not the one eliding line"
    shot makedefault-on

    # Pointer: a click on the row hands folders back, and the box follows the handler read after it.
    settings_click_control makeDefault
    makedefault_wait "true|true|Handing folders back|muted" "a click did not start handing folders back"
    makedefault_wait "false|false||" "handing back did not settle unticked with no note"
    [[ "$(makedefault_ran)" == "RAN --default;RAN --default off;" ]] || fail "makedefault: the stub ran '$(makedefault_ran)'"
    makedefault_wait_handler org.gnome.Nautilus.desktop
    [[ "$(makedefault_restarts)" == 2 ]] || fail "makedefault: a release that went through did not restart the portal"

    # Enter claims too; a build with no portal files says the file dialogs are left out.
    printf 'partly\n' > "$mode"
    key -k Return >/dev/null
    makedefault_wait "false|true|Making Flea the default|muted" "Enter did not start the claim"
    makedefault_wait "true|false|File dialogs need the flea package's portal files.|foreground" "a claim that skipped the chooser step did not say so"
    shot makedefault-partly
    printf 'ok\n' > "$mode"
    key -k space >/dev/null
    makedefault_wait "false|false||" "the partly claim was not handed back"
    [[ "$(makedefault_restarts)" == 4 ]] || fail "makedefault: the partly claim and its release did not each restart the portal"

    # A portal that will not restart keeps the switch as it went and says when file dialogs follow.
    printf 'fail\n' > "$restart"
    key -k space >/dev/null
    makedefault_wait "true|false|File dialogs follow after xdg-desktop-portal restarts.|foreground" "a failed portal restart did not say so"
    shot makedefault-portal
    printf 'ok\n' > "$restart"
    key -k space >/dev/null
    makedefault_wait "false|false||" "the claim with the late portal was not handed back"
    [[ "$(makedefault_restarts)" == 6 ]] || fail "makedefault: the portal was not restarted after each switch that went through"

    # A failed claim shows flea's own line in the error role, and the box stays what xdg-mime answers.
    printf 'fail\n' > "$mode"
    key -k space >/dev/null
    makedefault_wait "false|false|xdg-mime default exited 0 but inode/directory still resolves to org.gnome.Nautilus.desktop|error" \
        "a failed claim did not show flea's own line"
    [[ "$(makedefault_restarts)" == 6 ]] || fail "makedefault: a failed run restarted the portal"
    shot makedefault-failed

    # A refused claim, the build with no desktop entry, leaves the row inert to Space and to a click.
    printf 'refuse\n' > "$mode"
    key -k space >/dev/null
    makedefault_wait "false|true|Install a Flea package to make it the default.|foreground" "a refused claim did not make the row inert"
    runs=$(grep -c . "$makedefault_log")
    key -k space >/dev/null
    settings_click_control makeDefault
    sleep 1
    [[ "$(grep -c . "$makedefault_log")" == "$runs" ]] || fail "makedefault: the inert row still ran '$(makedefault_ran)'"
    makedefault_wait "false|true|Install a Flea package to make it the default.|foreground" "the inert row changed under a press"
    [[ "$(makedefault_restarts)" == 6 ]] || fail "makedefault: a refused run restarted the portal"
    shot makedefault-unpackaged

    # An action with a live state, never a setting: nothing reached ui.json, at any depth, and no save was refused.
    ipc uiSettings | jq -e "$no_key" >/dev/null \
        || fail "makedefault: the session state carries makeDefault, or did not read as an object: $(ipc uiSettings)"
    [[ ! -e "$XDG_STATE_HOME/flea/ui.json" ]] || jq -e "$no_key" "$XDG_STATE_HOME/flea/ui.json" >/dev/null \
        || fail "makedefault: ui.json carries makeDefault, or does not parse"
    [[ "$(ipc lastMessage)" != "That setting could not be saved." ]] || fail "makedefault: a press was sent to the settings writer"

    # A fresh window's first read, held past a claim's exit, answers from before it; only the read begun after may tick the box.
    printf 'ok\n' > "$mode"
    : > "$makedefault_events_log"
    : > "$box/hold"
    makedefault_launch "$dir" "$box" "$real_bin"
    settings_open_key
    settle
    settings_section about
    settings_focus_row makeDefault
    key -k space >/dev/null
    makedefault_wait "false|true|Making Flea the default|muted" "Space did not start the claim over a held read"
    makedefault_wait_event EXITED
    # Time for the window to take the exit in before the held answer lands.
    settle
    [[ "$(makedefault_events)" == "ASKED;RAN --default;EXITED;" ]] \
        || fail "makedefault: the claim did not exit inside the held read, the stubs logged '$(makedefault_events)'"
    : > "$box/gate"
    makedefault_wait "true|false|Folders, Show in folder and file dialogs open Flea.|foreground" \
        "the answer from before the claim settled it" 200
    [[ "$(makedefault_events)" == "ASKED;RAN --default;EXITED;ANSWERED org.gnome.Nautilus.desktop;ASKED;ANSWERED com.thisisgm.flea.desktop;" ]] \
        || fail "makedefault: the handler was not read again behind the held read, the stubs logged '$(makedefault_events)'"
    [[ "$(makedefault_restarts)" == 7 ]] || fail "makedefault: the claim behind the held read did not restart the portal"
    shot makedefault-held

    # With xdg-mime nowhere on PATH no read can start, and neither About's read nor a claim's re-read may leave the row working.
    makedefault_path_without_mime "$box"
    # Twice: About's own read, then the claim's re-read, each an xdg-mime that cannot start.
    for _ in 1 2; do
        printf '%s\n' 'Process failed to start, likely because the binary could not be found. Command: QList("xdg-mime", "query", "default", "inode/directory")' >> "$expected_warnings"
    done
    : > "$makedefault_events_log"
    makedefault_launch "$dir" "$box" "$real_bin" "$box/nomime"
    settings_open_key
    settle
    settings_section about
    # The entry probe lands beside the read that never started.
    settle
    makedefault_wait "false|false||" "About on a PATH with no xdg-mime did not open live and unticked"
    [[ "$(makedefault_handler)" == "Not reported" ]] || fail "makedefault: with no xdg-mime the File manager row states '$(makedefault_handler)'"
    settings_focus_row makeDefault
    key -k space >/dev/null
    makedefault_wait "false|true|Making Flea the default|muted" "Space did not start the claim with no xdg-mime"
    makedefault_wait "false|false||" "the claim stayed working behind a re-read whose xdg-mime could not start"
    [[ "$(makedefault_events)" == "RAN --default;EXITED;" ]] \
        || fail "makedefault: with no xdg-mime on PATH the stubs logged '$(makedefault_events)'"
    [[ "$(makedefault_handler)" == "Not reported" ]] || fail "makedefault: the re-read with no xdg-mime stated '$(makedefault_handler)'"
    [[ "$(makedefault_restarts)" == 8 ]] || fail "makedefault: the claim with no xdg-mime did not restart the portal"
    shot makedefault-nomime

    [[ ! -s "$refused" ]] || fail "makedefault: a stub was asked something it does not answer: $(tr '\n' ';' < "$refused")"
    key -k Escape >/dev/null
    settle

    printf 'MAKEDEFAULT keyboard=ok pointer=ok enter=ok partly=ok portal=ok failed=ok unpackaged=ok stored=none held=ok nomime=ok runs=%s restarts=%s\n' \
        "$runs" "$(makedefault_restarts)"
    kill_flea
}
