#!/usr/bin/env bash
# Sourced after ui-menus.sh and ui-permissions.sh; their guards and native input helpers are shared.
# shellcheck disable=SC2034,SC2154 # ui.sh supplies state; sourced helpers consume dynamically scoped locals.
operations_copy_to() {
    local destination="$1"
    menus_guard "$destination"
    key m >/dev/null
    menus_expect menuState '.opened and .snapshotReady' "Operations menu snapshots the native selection"
    menus_choose copyTo
    menus_expect menuDialogState '.opened and .action == "copyTo"' "Copy to opens its actual destination field"
    key -M ctrl -k a -m ctrl "$destination" -k Return >/dev/null
    # The live case observes transfer progress next; an extra dialog round trip could spend that window.
    [[ "${2:-}" == live ]] && return
    menus_expect menuDialogState '.opened | not' "Copy to submits through the destination field"
}

operations_footer_click() {
    local frame x y width height body caption inset row
    frame=$(ipc statusFooterState | jq -er .frame) || fail "operations: footer frame unavailable"
    read -r x y width height <<< "$frame"
    read -r body caption inset row <<< "$(ipc metrics)"
    [[ "$frame $inset" =~ ^[0-9]+(\ [0-9]+){4}$ ]] || fail "operations: invalid footer geometry: $frame inset=$inset"
    (( width > 2 * inset && height > 0 )) || fail "operations: footer has no visible former Dismiss position"
    menus_point "$((x + width - inset - 1)) $((y + height / 2))"
}

operations_secondary() {
    cardsize_expect statusSecondary "$1"
    menus_equal "$2" "$1" "$(ipc statusSecondary)"
}

# HIG locked-menu keeps a notice whole by using the count zone's unpainted space before the disk text.
operations_footer_geometry() {
    local body caption inset row muted
    read -r body caption inset row <<< "$(ipc metrics)"
    [[ "$body $caption $inset $row" =~ ^[0-9]+(\ [0-9]+){3}$ ]] || fail "operations: live footer tokens unavailable"
    muted=$(ipc palette | cut -d' ' -f4)
    [[ -n "$muted" ]] || fail "operations: muted role unavailable"
    menus_expect statusFooterState ".borderWidth == 0 and (.frame | split(\" \") | map(tonumber) | .[2] == 880 and .[3] == 27)
        and .left.x == $inset and .left.fontSize == $caption
        and .disk.fontSize == $caption
        and (if .centre.visible then .centre.fontSize == $caption else .centre.text == \"\" end)
        and (if .secondary.visible then .secondary.fontSize == $caption and .secondary.color == \"$muted\" else .secondary.text == \"\" end)
        and (.secondary.text == \"\" or (.secondary.text | startswith(\" · \")))
        and (.secondary.text | contains(\"|\") | not)
        and (if .centre.visible then .centre.x >= .left.x + ([.left.width, .left.implicitWidth] | min) + $inset
            and .centre.x + .centre.width <= .disk.x + .disk.width - ([.disk.width, .disk.implicitWidth] | min) - $inset else true end)" "$1 matches the drawn count, notice and disk roles"
    printf 'OPERATIONS_FOOTER label=%q state=%s\n' "$1" "$(ipc statusFooterState)"
}

# Sample input: 100000 prints 100,000 and 999 prints 999, as ui/js/Format.js count() groups every count by thousands.
operations_group_count() {
    local digits="$1" grouped=""
    while (( ${#digits} > 3 )); do
        grouped=",${digits: -3}$grouped"
        digits=${digits:0:${#digits}-3}
    done
    printf '%s' "$digits$grouped"
}

operations_idle_footer() {
    local total="$1" selected="$2" label="$3" grouped_total grouped_selected items
    grouped_total=$(operations_group_count "$total")
    grouped_selected=$(operations_group_count "$selected")
    items="$grouped_total items"
    [[ "$total" == 1 ]] && items="1 item"
    # The left zone answers the selection when there is one, and may carry a byte total after it.
    [[ "$selected" == 0 ]] || items="$grouped_selected of $grouped_total selected"
    # The idle strip retains its count and disk while drawing no notice.
    menus_expect statusFooterState ".total == $total and .selected == $selected and .filesystem != \"unknown\" and (.left.text | startswith(\"$items\")) and .disk.text == .filesystem and .disk.width > 0 and .disk.color == .left.color and .disk.fontSize == .left.fontSize and .centre.text == \"\" and .disk.x >= .left.x + .left.width" "$label"
    menus_equal "$label foreground" "$(ipc themeForeground)" "$(ipc statusColor)"
    operations_footer_geometry "$label"
}

operations_counts_footer() {
    menus_expect statusFooterState '.left.text == .counts and .left.text != .path and .centre.text != .filesystem and .disk.text == .filesystem' "$1 retains counts and the disk beside activity"
    operations_footer_geometry "$1"
}

operations_missing_footer() {
    local missing="$menu_box/missing"
    menus_guard "$missing"
    [[ ! -e "$missing" && ! -L "$missing" ]] || fail "operations: missing-path fixture already exists"
    launch "$missing"
    permissions_viewport 880 620
    menus_expect statusFooterState '.listingState == "error" and .filesystem == "unknown"' "missing directory says unknown rather than the filesystem it did not reach"
    menus_acknowledge
    menus_expect statusFooterState '.left.text == "unavailable" and .disk.text == "unknown" and .centre.text == ""' "missing filesystem reports unavailable on the left and unknown on the right"
    menus_equal "missing filesystem fallback foreground" "$(ipc themeForeground)" "$(ipc statusColor)"
    shot operations-no-filesystem
    kill_flea
}

operations_pause_backend() {
    local pid end state
    local -a pids
    menus_guard "$permissions_listing"
    mapfile -t pids < <(backend_pids)
    [[ "${#pids[@]}" == 1 ]] || fail "operations: loading proof requires one owned backend"
    pid="${pids[0]}"
    permissions_backend_owned "$pid" || fail "operations: loading backend candidate, fixture or session differs"
    operations_stopped="$pid"
    kill -STOP "$pid" || fail "operations: could not pause the owned listing backend"
    end=$((SECONDS + 15))
    while (( SECONDS < end )); do
        # Sample process state: Tsl; its leading T proves the owned backend stopped.
        state=$(ps -o stat= -p "$pid") || fail "operations: paused listing backend disappeared"
        [[ "$state" == T* ]] && break
        sleep 0.05
    done
    [[ "$state" == T* ]] || fail "operations: listing backend did not stop"
}

operations_loading_footer() (
    local permissions_listing="$1" total="$2" destination="$3" destination_total="$4" operations_stopped=""
    menus_guard "$destination"
    trap 'permissions_resume_stopped "$operations_stopped"' EXIT
    operations_pause_backend
    key -M ctrl -k l -m ctrl "$destination" -k Return >/dev/null
    menus_expect statusFooterState '.listingState == "loading" and .filesystem != "unknown" and .left.text == "" and .left.text == .counts and .disk.text == .filesystem' "native refresh clears stale counts while the backend cannot reply"
    shot operations-loading-footer
    permissions_resume_stopped "$operations_stopped" || fail "operations: listing backend could not resume"
    operations_stopped=""
    wait_listing "$destination_total"
    key -M ctrl -k l -m ctrl "$permissions_listing" -k Return >/dev/null
    wait_listing "$total"
    operations_idle_footer "$total" 0 "resumed listing restores idle counts and filesystem"
)

operations_search_footer() (
    local permissions_listing="$menu_box/mixed" operations_stopped=""
    launch "$permissions_listing"
    wait_listing 5
    permissions_viewport 880 620
    trap 'permissions_resume_stopped "$operations_stopped"' EXIT
    operations_pause_backend
    key f >/dev/null
    key c.txt -k Return >/dev/null
    menus_expect keyDeliveryState '.searchMode == "results" and .searchQuery == "c.txt" and .searchRunning' "native Search submits while its owned backend is stopped"
    menus_expect statusFooterState '.listingState == "loading" and .left.text == .counts and .left.text == "" and .centre.text == "0 found · Searching, 0 scanned"' "submitted search pairs its found count with its actual initial scan"
    # V7: StatusBar rule 4 drops advice from this zone, so the way out is the search strip's alone.
    operations_secondary "" "a running Search leaves the strip to say how to leave it"
    operations_footer_geometry "initial search progress"
    shot operations-search-submitted
    permissions_resume_stopped "$operations_stopped" || fail "operations: search backend could not resume"
    operations_stopped=""
    menus_expect keyDeliveryState '.searchMode == "results" and .searchQuery == "c.txt" and (.searchRunning | not)' "resumed backend completes the real native search"
    wait_listing 1
    menus_expect statusFooterState '.left.text == "1 item" and (.centre.text | test("^1 found in [0-9]+\\.[0-9] s$"))' "completed search reports what it found and how long it took"
    [[ "$(ipc rowAt 0)" == c.txt\|file\|* ]] || fail "operations: Search returned another fixture identity"
    shot operations-search-completed
    key -k Escape >/dev/null
    menus_expect keyDeliveryState '.searchMode == ""' "native Escape closes completed search"
    wait_listing 5
    operations_idle_footer 5 0 "leaving Search restores the directory footer"
    kill_flea
    printf 'OPERATIONS_SEARCH initial_submitted=ok resumed_result=ok close=ok positive_scanned_live=not_run\n'
)

operations_absent() {
    local path="$1" end=$((SECONDS + 15))
    menus_guard "$path"
    while (( SECONDS < end )); do
        [[ ! -e "$path" && ! -L "$path" ]] && return
        sleep 0.05
    done
    fail "operations: an owned operation retained $path"
}

operations_mixed() {
    local source="$menu_box/mixed" destination="$menu_box/mixed-out" name selected notice
    for name in "$source" "$destination"; do menus_guard "$name"; mkdir "$name"; done
    for name in a.txt b.txt c.txt d.txt e.txt; do
        menus_guard "$source/$name"
        printf 'original %s\n' "$name" > "$source/$name"
    done
    # A real failure that is not a name: c.txt cannot be read, so its copy fails and the other four land.
    chmod 000 "$source/c.txt" || fail "operations: cannot make c.txt unreadable"
    launch "$source"
    wait_listing 5
    permissions_viewport 880 620
    operations_secondary "" "no retry claim exists before an attributed failure"
    operations_idle_footer 5 0 "idle footer shows all five items and actual filesystem"
    # mixed-out holds nothing yet, so the resumed listing lands on the empty state.
    operations_loading_footer "$source" 5 "$destination" 0 || fail "operations: paused navigation proof failed"
    key v >/dev/null
    operations_idle_footer 5 1 "native selection adds the separate one-selected label"
    shot operations-idle-selected
    key v >/dev/null
    operations_idle_footer 5 0 "native deselection removes the selection label"
    hotkey --global ctrl a flea >/dev/null
    menus_expect selectionCount '. == 5' "native Select All captures all five sources"
    operations_idle_footer 5 5 "native Select All updates the separate selection label"
    operations_copy_to "$destination"
    menus_expect statusActivityState '(.activities | length) == 0 and .errors == 1 and (.notice | contains("Copied 4 of 5") and contains("1 failed"))' "mixed completion retains all counts behind its named error"
    menus_error 'Copy failed: c.txt · permission denied' 'failure names the failed source and its cause'
    operations_counts_footer "persistent error"
    menus_expect selectionCount '. == 1' "failed original is selected for retry"
    selected=$(ipc selectedIndices)
    [[ "$selected" == "$(row_index_of c.txt)" ]] || fail "operations: retry selected a different source"
    operations_secondary " · esc dismisses" "unacknowledged error names only its dismissal key"
    menus_expect statusFooterState '(.centre.text | startswith("Copy failed: c.txt · ")) and (.centre.text | contains("(os error") | not)' "error is a plain sentence with a named cause"
    for name in a.txt b.txt d.txt e.txt; do menus_same_file "committed copy $name" "$source/$name" "$destination/$name"; done
    [[ ! -e "$destination/c.txt" && ! -L "$destination/c.txt" ]] || fail "operations: the unreadable source left a copy behind"
    shot operations-mixed-error
    sleep "$transient_clear_s"
    menus_expect statusActivityState '.errors == 1 and (.notice | contains("Copied 4 of 5"))' "error and hidden outcome survive the notice timeout"
    cardsize_expect focusView list
    operations_footer_click
    # A native focus cycle flushes click delivery without changing the retry-selection identity.
    key -k Tab >/dev/null
    cardsize_expect focusView rail
    key -k Tab >/dev/null
    cardsize_expect focusView list
    menus_expect statusActivityState '.errors == 1' "pointer at the removed Dismiss control leaves the persistent error unacknowledged"
    menus_error 'Copy failed: c.txt' 'removed footer control does not dismiss the named failure'
    shot operations-error-footer-click-inert
    key -k Escape >/dev/null
    menus_message 'Copied 4 of 5' 'acknowledgement reveals the complete outcome'
    menus_expect statusActivityState '.undoAvailable and .errors == 0' "successful items remain undoable through the native key"
    operations_counts_footer "acknowledged completion"
    operations_secondary " · z undoes · c.txt selected for retry" "acknowledged completion retains Undo hint and selected-retry secondary"
    notice=$(ipc statusActivityState | jq -er .notice) || fail "operations: acknowledged Undo notice unavailable"
    sleep "$transient_clear_s"
    menus_equal 'acknowledged Undo notice survives the transient timeout unchanged' "$notice" "$(ipc statusActivityState | jq -er .notice)"
    menus_expect statusActivityState '.undoAvailable and .errors == 0' "acknowledged completion stays undoable beyond the notice timeout"
    operations_secondary " · z undoes · c.txt selected for retry" "acknowledged retry identity and Undo hint survive the notice timeout"
    shot operations-mixed-acknowledged
    key -k Escape >/dev/null
    menus_expect selectionCount '. == 0' "native Escape clears the retry selection"
    operations_secondary " · z undoes" "changing selection removes the previous retry claim while retaining Undo"
    seek_row_named c.txt
    key v >/dev/null
    menus_expect selectionCount '. == 1' "native re-selection names one source for the explicit retry"
    operations_secondary " · z undoes" "manual re-selection cannot revive an earlier identity proof"
    operations_copy_to "$destination"
    menus_expect statusActivityState '(.activities | length) == 0 and .errors == 1' "a repeated failure records its own completed failure"
    operations_secondary " · esc dismisses" "a repeated error retains the dismissal hint"
    menus_acknowledge
    operations_secondary " · c.txt selected for retry" "acknowledged failure exposes its fresh identity-verified retry text"
    # Making it readable again is the fix, and the metadata change it makes is what the watch sees.
    menus_guard "$source/c.txt"
    chmod 644 "$source/c.txt" || fail "operations: cannot make c.txt readable again"
    operations_secondary "" "external metadata change invalidates the completed outcome's retry proof"
    menus_expect selectionCount '. == 1' "watch invalidation does not change the user's selected source"
    menus_guard "$destination/c.txt"
    operations_copy_to "$destination"
    menus_expect statusActivityState '(.activities | length) == 0 and .errors == 0 and (.notice | contains("Copied 1 item"))' "retry copies only the retained original selection"
    menus_same_file 'retry preserves source contents' "$source/c.txt" "$destination/c.txt"
    operations_counts_footer "undoable transient"
    operations_secondary " · z undoes" "successful retry names the native Undo key"
    shot operations-transient-undo
    menus_guard "$destination/c.txt"
    cardsize_expect focusView list
    operations_footer_click
    key -k Tab >/dev/null
    cardsize_expect focusView rail
    key -k Tab >/dev/null
    cardsize_expect focusView list
    menus_expect statusActivityState '.undoAvailable and (.notice | contains("Copied 1 item"))' "clicking the informational footer cannot undo a completed copy"
    menus_same_file 'footer click preserves the completed retry' "$source/c.txt" "$destination/c.txt"
    key z >/dev/null
    menus_message 'Undid the copy' 'native z reverses the retry'
    [[ ! -e "$destination/c.txt" ]] || fail "operations: retry Undo retained its created file"
    for name in a.txt b.txt d.txt e.txt; do menus_guard "$destination/$name"; done
    key z >/dev/null
    menus_message 'Undid the copy' 'native z reverses the earlier committed items'
    for name in a.txt b.txt d.txt e.txt; do operations_absent "$destination/$name"; done
    for name in a.txt b.txt c.txt d.txt e.txt; do [[ "$(cat "$source/$name")" == "original $name" ]] || fail "operations: source changed through copy or Undo"; done
    kill_flea
}

# Copy to onto a name that exists asks the paste's own question, and Keep both lands beside it.
operations_copy_to_collision() {
    local source="$menu_box/collide" destination="$menu_box/collide-out" name
    for name in "$source" "$destination"; do menus_guard "$name"; mkdir "$name"; done
    for name in photo.png notes.txt; do
        menus_guard "$source/$name"
        printf 'yours %s\n' "$name" > "$source/$name"
    done
    menus_guard "$destination/photo.png"
    printf 'there\n' > "$destination/photo.png"
    launch "$source"
    wait_listing 2
    permissions_viewport 880 620
    hotkey --global ctrl a flea >/dev/null
    menus_expect selectionCount '. == 2' "Copy to collision selects both sources"
    menus_guard "$destination/photo copy.png"
    operations_copy_to "$destination"
    menus_expect collideState '.opened and .title == "photo.png already exists in collide-out" and .names == ["photo.png"] and .focus == "keep"
        and .buttonsFit and (.titleTruncated | not)' "Copy to asks once about the name that exists"
    shot collide-copyto
    key -k Return >/dev/null
    menus_message 'Copied 2 items' "Keep both on Copy to copies every item"
    menus_same_file 'Keep both lands the incoming photo beside the one there' "$source/photo.png" "$destination/photo copy.png"
    menus_equal 'Keep both leaves the photo already there' 'there' "$(cat "$destination/photo.png")"
    menus_same_file 'the free name is copied as it is' "$source/notes.txt" "$destination/notes.txt"
    key z >/dev/null
    menus_message 'Undid the copy' 'native z reverses the kept copy'
    [[ ! -e "$destination/photo copy.png" && ! -e "$destination/notes.txt" ]] || fail "operations: Undo left the Copy to copies"
    menus_equal 'Undo leaves the photo already there' 'there' "$(cat "$destination/photo.png")"
    kill_flea
}

operations_long_error() {
    local source="$menu_box/long-name" destination="$menu_box/long-name-out" name path
    for path in "$source" "$destination"; do
        menus_guard "$path"
        mkdir "$path" || fail "operations: long-name fixture directory creation failed"
    done
    printf -v name 'long-%0230d.txt' 0
    menus_guard "$source/$name"
    menus_guard "$destination/$name"
    printf 'long-name source\n' > "$source/$name" || fail "operations: legal long-name source creation failed"
    chmod 000 "$source/$name" || fail "operations: cannot make the long-name source unreadable"
    launch "$source"
    wait_listing 1
    permissions_viewport 880 620
    key v >/dev/null
    menus_expect selectionCount '. == 1' "long-name failure selects its real source"
    operations_copy_to "$destination"
    menus_expect statusActivityState '(.activities | length) == 0 and .errors == 1 and (.notice | contains("Copied 0 of 1") and contains("1 failed"))' "one long-name failure records its real failed outcome"
    menus_error "Copy failed: $name" 'long-name error retains the exact failed source identity'
    menus_equal 'long-name retry selects the original row' "$(row_index_of "$name")" "$(ipc selectedIndices)"
    operations_secondary " · esc dismisses" "long-name error keeps the short dismissal hint visible"
    menus_expect statusFooterState '.centre.visible and .centre.width > 0 and .centre.truncated and .centre.implicitWidth > .centre.width
        and .secondary.visible and .secondary.width > 0 and (.secondary.truncated | not) and .secondary.implicitWidth <= .secondary.width
        and .hintWidth > 0 and .secondary.width >= ([.hintWidth, .slotWidth] | min)' \
        "long error elides while its complete dismissal hint stays visible"
    menus_equal 'long-name error retains its semantic role' "$(ipc palette | cut -d' ' -f6)" "$(ipc statusColor)"
    operations_footer_geometry "long-name persistent error"
    [[ ! -e "$destination/$name" && ! -L "$destination/$name" ]] || fail "operations: the unreadable long-name source left a copy behind"
    shot operations-long-name-error
    menus_acknowledge
    menus_expect statusFooterState '(.centre.text | startswith("Copied 0 of 1")) and .centre.width > 0 and (.centre.truncated | not) and .secondary.truncated' \
        "acknowledging the long-name error leaves its complete short outcome ahead of the elided retry"
    shot operations-long-name-acknowledged
    kill_flea
    chmod 644 "$source/$name" || fail "operations: cannot make the long-name source readable again"
    [[ "$(cat "$source/$name")" == 'long-name source' ]] || fail "operations: the long-name failure changed its source"
}

operations_copy_gate() {
    local window_pid="" frontend="${test_frontend:-qml}"
    [[ "$frontend" != gpui ]] || window_pid=$(flea_pid)
    python3 - "$1" "$flea_bin" "$3" "$XDG_STATE_HOME" "$menu_box" "$2" "$operations_bytes" "$frontend" "$window_pid" "$flea_test_helper" "${4:-published}" 3<&0 <<'PY'
import ctypes, json, os, select, signal, stat, struct, sys, time
from pathlib import Path

pid = int(sys.argv[1])
binary, source, state_home, root, destination = map(Path, sys.argv[2:7])
total = int(sys.argv[7])
frontend, window_pid, transport_helper = sys.argv[8:11]
mode = sys.argv[11]
if mode not in ("published", "staged"):
    raise RuntimeError("operations: unknown copy gate mode")
timeout_seconds = 15

def guard(path):
    if not path.is_absolute() or not root.is_absolute() or root.resolve() != root or not (root / ".flea-test-sandbox").is_file():
        raise RuntimeError(f"operations: copy gate needs an absolute owned sandbox: {path}")
    if path.resolve() == root or not path.resolve().is_relative_to(root):
        raise RuntimeError(f"operations: copy gate path escaped its sandbox: {path}")
    return path

for path in (source, state_home, destination):
    guard(path)
partial = guard(destination / "a-large.bin")
later = guard(destination / "b-after.txt")
if os.path.lexists(partial) or os.path.lexists(later):
    raise RuntimeError("operations: copy gate destination is not empty")

process = Path("/proc", str(pid))
pidfd = os.pidfd_open(pid)
watchfd = None
stagefd = None
stopped = False
commands = os.fdopen(3)
def interrupted(number, frame):
    raise RuntimeError(f"operations: copy gate interrupted by signal {number}")

def candidate_gui(helper, pid):
    sys.path.insert(0, str(Path(helper).parent))
    from flea_transport_cli import select as owned_window
    window, arguments = owned_window(["--pid", pid])
    record = window.record()
    if arguments or record["frontend"] != "gpui" or record["pid"] != int(pid) or not record["gui"]:
        raise RuntimeError("operations: copy gate has no exact native window record")
    return record["gui"]

def reading_source():
    expected = source / "a-large.bin"
    original = expected.lstat()
    if not stat.S_ISREG(original.st_mode):
        return False
    for descriptor in (process / "fd").iterdir():
        try:
            if os.readlink(descriptor) != os.fspath(expected):
                continue
            # Sample input: "flags:\t0100000", an octal read-only descriptor.
            flags = next(int(line.split()[1], 8) for line in (process / "fdinfo" / descriptor.name).read_text().splitlines() if line.startswith("flags:"))
            if flags & os.O_ACCMODE not in (os.O_RDONLY, os.O_RDWR) or flags & os.O_PATH:
                continue
            actual = descriptor.stat()
            if (actual.st_dev, actual.st_ino) == (original.st_dev, original.st_ino):
                return True
        except (FileNotFoundError, ProcessLookupError):
            continue
    return False

def staged_file():
    # The production copier publishes an O_TMPFILE or hidden sibling only after all bytes land.
    for descriptor in (process / "fd").iterdir():
        try:
            target = Path(os.readlink(descriptor).removesuffix(" (deleted)"))
            if target.parent != destination or not (target.name.startswith(("#", ".flea-part-")) or target == partial):
                continue
            # Sample input: the fdinfo line "flags:\t0100002", octal open flags.
            flags = next(int(line.split()[1], 8) for line in (process / "fdinfo" / descriptor.name).read_text().splitlines() if line.startswith("flags:"))
            if flags & os.O_ACCMODE not in (os.O_WRONLY, os.O_RDWR):
                continue
            held = os.open(descriptor, os.O_RDONLY | os.O_NONBLOCK)
            metadata = os.fstat(held)
            if (stat.S_ISREG(metadata.st_mode) and metadata.st_uid == os.getuid()
                    and Path(os.readlink(f"/proc/self/fd/{held}").removesuffix(" (deleted)")) == target
                    and metadata.st_dev == destination.stat().st_dev and metadata.st_size < total):
                return held
            os.close(held)
        except (FileNotFoundError, ProcessLookupError):
            continue
    return None

try:
    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGINT, interrupted)
    # Sample argv: /owned/target/release/flea NUL --backend NUL.
    argv = (process / "cmdline").read_bytes().rstrip(b"\0").split(b"\0")
    # Sample environment entry: FLEA_PATH=/tmp/owned/cancel-source NUL.
    environment = dict(item.split(b"=", 1) for item in (process / "environ").read_bytes().split(b"\0") if b"=" in item)
    expected = {b"FLEA_BIN": os.fsencode(binary), b"XDG_STATE_HOME": os.fsencode(state_home)}
    if frontend == "gpui":
        # GPUI names the start folder in the direct candidate parent's argv.
        gui_binary = candidate_gui(transport_helper, window_pid)
        # Sample input: the status line "PPid:\t4242".
        parent_pid = next(line.split()[1] for line in (process / "status").read_text().splitlines() if line.startswith("PPid:"))
        parent = Path("/proc", parent_pid)
        parent_argv = (parent / "cmdline").read_bytes().rstrip(b"\0").split(b"\0")
        if (parent_pid != window_pid or parent.stat().st_uid != os.getuid()
                or (parent / "exe").resolve() != Path(gui_binary).resolve()
                or parent_argv != [os.fsencode(gui_binary), b"--", os.fsencode(source)]):
            raise RuntimeError("operations: copy gate candidate window ownership differs")
    elif frontend == "qml":
        expected[b"FLEA_PATH"] = os.fsencode(source)
    else:
        raise RuntimeError("operations: copy gate frontend is unknown")
    if process.stat().st_uid != os.getuid() or (process / "exe").resolve() != binary.resolve() or argv != [os.fsencode(binary), b"--backend"] or any(environment.get(key) != value for key, value in expected.items()):
        raise RuntimeError(f"operations: copy gate backend {pid} ownership differs")
    if select.select([pidfd], [], [], 0)[0]:
        raise RuntimeError(f"operations: copy gate backend {pid} already exited")
    libc = ctypes.CDLL(None, use_errno=True)
    watchfd = libc.inotify_init1(os.O_CLOEXEC | os.O_NONBLOCK)
    if watchfd < 0:
        raise OSError(ctypes.get_errno(), "operations: inotify_init1 failed")
    create_mask, overflow_mask = 0x100, 0x4000
    watched_mask = create_mask | (0x20 | 0x2 if mode == "staged" else 0)
    libc.inotify_add_watch.argtypes = [ctypes.c_int, ctypes.c_char_p, ctypes.c_uint32]
    watched = libc.inotify_add_watch(watchfd, os.fsencode(destination), watched_mask)
    if watched < 0:
        raise OSError(ctypes.get_errno(), f"operations: cannot watch {destination}")
    print(json.dumps({"state": "armed", "pid": pid, "destination": str(destination)}), flush=True)
    deadline = time.monotonic() + timeout_seconds
    while not stopped:
        ready, _, _ = select.select([watchfd, commands, pidfd], [], [], max(0, deadline - time.monotonic()))
        if not ready:
            raise RuntimeError("operations: real copy did not create its destination before the gate timeout")
        if commands in ready:
            if commands.readline() == "":
                raise SystemExit(0)
            raise RuntimeError("operations: copy gate received a command before the real copy began")
        if pidfd in ready:
            raise RuntimeError("operations: backend exited before the real copy began")
        # Sample inotify event: wd:i32, mask:u32, cookie:u32, name_len:u32, NUL-padded filename.
        events = os.read(watchfd, 4096)
        offset = 0
        while offset < len(events):
            watch, mask, _, length = struct.unpack_from("iIII", events, offset)
            offset += struct.calcsize("iIII")
            name = events[offset:offset + length].rstrip(b"\0")
            offset += length
            if mask & overflow_mask:
                raise RuntimeError("operations: copy gate lost inotify events")
            if watch != watched or not mask & watched_mask:
                continue
            if mode == "staged":
                stagefd = staged_file()
                if stagefd is None:
                    continue
            elif name != os.fsencode(partial.name):
                continue
            if mode == "staged" or mask & create_mask:
                stopped = True
                signal.pidfd_send_signal(pidfd, signal.SIGSTOP)
                break
    deadline = time.monotonic() + timeout_seconds
    while True:
        # Sample task status line: State: T (stopped); every thread must have reached the stop.
        states = [next(line.split()[1] for line in task.read_text().splitlines() if line.startswith("State:")) for task in (process / "task").glob("*/status")]
        if states and all(state == "T" for state in states):
            break
        if time.monotonic() >= deadline:
            raise RuntimeError("operations: copy backend did not stop all threads")
        time.sleep(0.01)
    metadata = os.fstat(stagefd) if stagefd is not None else partial.lstat()
    if mode == "staged" and not reading_source():
        raise RuntimeError("operations: stopped copy does not hold its expected source for reading")
    if (not stat.S_ISREG(metadata.st_mode) or not 0 <= metadata.st_size < total or os.path.lexists(later)
            or mode == "staged" and os.path.lexists(partial)):
        raise RuntimeError(f"operations: copy completed before interruption; partial bytes={metadata.st_size}, total={total}")
    print(json.dumps({"state": "stopped", "pid": pid, "bytes": metadata.st_size, "total": total, "threads": len(states)}), flush=True)
    # EOF releases the stop if native proof aborts; the input checks own their deadlines.
    ready, _, _ = select.select([commands, pidfd], [], [])
    if pidfd in ready:
        raise RuntimeError("operations: interrupted backend exited before cancellation resumed it")
    command = commands.readline()
    if command not in ("resume\n", ""):
        raise RuntimeError(f"operations: unknown copy gate command: {command!r}")
finally:
    signal.signal(signal.SIGTERM, signal.SIG_IGN)
    signal.signal(signal.SIGINT, signal.SIG_IGN)
    if stopped:
        try:
            signal.pidfd_send_signal(pidfd, signal.SIGCONT)
        except ProcessLookupError:
            pass
    if watchfd is not None and watchfd >= 0:
        os.close(watchfd)
    if stagefd is not None:
        os.close(stagefd)
    os.close(pidfd)
PY
}

operations_close_gate() {
    local result=0
    if [[ -n "$gate_input" ]]; then exec {gate_input}>&-; gate_input=""; fi
    if [[ -n "$gate_pid" ]]; then wait "$gate_pid" || result=1; gate_pid=""; fi
    if [[ -n "$gate_output" ]]; then exec {gate_output}<&-; gate_output=""; fi
    return "$result"
}

operations_cancel() (
    local variant="$1" source="$menu_box/cancel-source" destination="$menu_box/cancel-$1"
    local permissions_listing="$source" pid gate_pid="" gate_input="" gate_output="" receipt cancel_centre selected cursor
    local -a pids
    menus_guard "$destination"
    mkdir "$destination" || fail "operations: cancellation destination could not be created"
    launch "$source"
    wait_listing 2
    permissions_viewport 880 620
    hotkey --global ctrl a flea >/dev/null
    menus_expect selectionCount '. == 2' "interrupted cancellation selects two real files"
    mapfile -t pids < <(backend_pids)
    [[ "${#pids[@]}" == 1 ]] || fail "operations: cancellation needs one owned backend"
    pid="${pids[0]}"
    permissions_backend_owned "$pid" || fail "operations: backend executable, fixture or session identity differs"
    coproc OPERATIONS_GATE { operations_copy_gate "$pid" "$destination" "$source"; }
    gate_pid="$OPERATIONS_GATE_PID" gate_input="${OPERATIONS_GATE[1]}" gate_output="${OPERATIONS_GATE[0]}"
    trap 'operations_close_gate || { printf "FAIL: operations: copy gate teardown failed\n" >&2; exit 1; }' EXIT
    read -r -t 15 -u "$gate_output" receipt || fail "operations: copy gate did not arm"
    jq -e '.state == "armed"' <<< "$receipt" >/dev/null || fail "operations: invalid copy gate readiness: $receipt"
    printf 'OPERATIONS_GATE %s\n' "$receipt"
    operations_copy_to "$destination"
    read -r -t 15 -u "$gate_output" receipt || fail "operations: real copy was not interrupted"
    jq -e '.state == "stopped" and .bytes < .total and .threads > 0' <<< "$receipt" >/dev/null || fail "operations: invalid interruption receipt: $receipt"
    printf 'OPERATIONS_GATE %s\n' "$receipt"
    menus_expect statusActivityState '.activities[0].running and .transferCard.visible and .transferCard.cancel.visible and .transferCard.cancel.enabled' "real in-flight transfer remains cancellable in its card while interrupted"
    operations_counts_footer "interrupted transfer"
    # StatusBar rule 8 (3b4223c5): the card owns a running transfer's cancel key and the strip's secondary draws nothing for it.
    operations_secondary "" "transfer footer leaves the cancellation key to the card"
    key m >/dev/null
    menus_expect menuState '.opened' "a native popup opens above the running transfer"
    key -k Escape >/dev/null
    menus_expect menuState '.opened | not' "Escape closes the popup before cancelling its transfer"
    menus_expect statusActivityState '.activities[0].running and (.activities[0].cancelling | not)' "popup Escape leaves the named transfer running"
    key / >/dev/null
    menus_expect keyDeliveryState '.filterTyping' "native filter opens above the running transfer"
    key -k Escape >/dev/null
    menus_expect keyDeliveryState '(.filterTyping | not) and .filterQuery == ""' "Escape closes the filter before cancelling its transfer"
    menus_expect statusActivityState '.activities[0].running and (.activities[0].cancelling | not)' "filter Escape leaves the named transfer running"
    key f >/dev/null
    menus_expect keyDeliveryState '.searchMode == "typing"' "native search takes focus above the running transfer"
    key -k Escape >/dev/null
    menus_expect keyDeliveryState '.searchMode == ""' "Escape closes focused search before cancelling its transfer"
    menus_expect statusActivityState '.activities[0].running and (.activities[0].cancelling | not)' "focused-search Escape leaves the named transfer running"
    shot "operations-transfer-$variant-paused"
    menus_guard "$destination/a-large.bin"
    menus_guard "$destination/b-after.txt"
    selected=$(ipc selectedIndices) || fail "operations: cancellation selection unavailable"
    cursor=$(ipc cursor) || fail "operations: cancellation cursor unavailable"
    if [[ "$variant" == pointer ]]; then
        cancel_centre=$(ipc statusActivityState | jq -er '.transferCard.cancel | select(.visible and .enabled) | .centre') \
            || fail "operations: live transfer-card Cancel is unavailable"
        menus_point "$cancel_centre"
    else
        [[ "$variant" == escape ]] || fail "operations: unknown cancellation input: $variant"
        key -k Escape >/dev/null
    fi
    menus_expect statusActivityState '.activities[0].cancelling and .transferCard.visible and .transferCard.cancelling and ((.transferCard.cancel.visible and .transferCard.cancel.enabled) | not)' "$variant cancellation becomes visibly pending while the backend is interrupted"
    operations_counts_footer "pending cancellation"
    if [[ "$variant" == pointer ]]; then menus_point "$cancel_centre"; else key -k Escape >/dev/null; fi
    key / >/dev/null
    menus_expect keyDeliveryState '.filterTyping' "a later native key is delivered before checking repeated $variant cancellation"
    key -k Escape >/dev/null
    menus_expect keyDeliveryState '.filterTyping | not' "Escape closes the pending-transfer filter without changing cancellation"
    menus_expect statusActivityState '.activities[0].cancelling and .transferCard.cancelling and ((.transferCard.cancel.visible and .transferCard.cancel.enabled) | not)' "repeated $variant cancellation remains pending"
    menus_equal "$variant cancellation does not reach the listing selection" "$selected" "$(ipc selectedIndices)"
    menus_equal "$variant cancellation does not move the listing cursor" "$cursor" "$(ipc cursor)"
    shot "operations-cancelling-$variant"
    printf 'resume\n' >&"$gate_input" || fail "operations: copy gate could not resume the owned backend"
    operations_close_gate || fail "operations: interrupted copy gate failed"
    menus_expect statusActivityState '(.activities | length) == 0 and .errors == 0 and (.notice | contains("Copied 0 of 2") and contains("2 skipped") and contains("cancelled") and (contains("failed") | not))' "interrupted cancellation reports skipped work without a false write error"
    [[ ! -e "$destination/a-large.bin" && ! -e "$destination/b-after.txt" ]] || fail "operations: cancellation retained a partial copy or started a later item"
    [[ "$(stat -c '%s' "$source/a-large.bin")" == "$operations_bytes" && "$(cat "$source/b-after.txt")" == 'after cancellation' ]] \
        || fail "operations: cancellation changed source data"
    shot "operations-cancelled-$variant-interrupted"
    printf 'OPERATIONS_CANCEL variant=%s interrupted_native=ok skipped=2 failed=0 partial_cleanup=ok source_preserved=ok unpaused_live=not_run\n' "$variant"
    kill_flea
)

operations_missed_window() {
    printf 'OPERATIONS_MISSED_WINDOW phase=%q state=%s attempts=1 workload_unchanged=true\n' "$1" "$2"
    fail "operations: $1 missed the live observation/input window; no retry or workload enlargement"
}

operations_cancel_live() (
    local source="$menu_box/cancel-source" destination="$menu_box/cancel-live" permissions_listing="$menu_box/cancel-source"
    local state observed_bytes source_hash source_identity pid deadline path
    local -a pids
    menus_guard "$destination"
    mkdir "$destination" || fail "operations: live cancellation destination creation failed"
    for path in "$source/a-large.bin" "$source/b-after.txt" "$destination/a-large.bin" "$destination/b-after.txt"; do menus_guard "$path"; done
    source_hash=$(sha256sum < "$source/a-large.bin") || fail "operations: source checksum failed"
    source_identity=$(stat -c '%d:%i:%s:%Y' "$source/a-large.bin") || fail "operations: source identity unavailable"
    launch "$source"
    trap 'kill_flea' EXIT
    wait_listing 2
    permissions_viewport 880 620
    hotkey --global ctrl a flea >/dev/null
    menus_expect selectionCount '. == 2' "live cancellation selects both real files"
    mapfile -t pids < <(backend_pids)
    [[ "${#pids[@]}" == 1 ]] || fail "operations: live cancellation requires one owned backend"
    pid="${pids[0]}"
    permissions_backend_owned "$pid" || fail "operations: live backend identity differs"
    state=$(ps -o stat= -p "$pid") || fail "operations: live backend state unavailable"
    [[ "$state" != T* ]] || fail "operations: live backend is stopped before the native operation"
    operations_copy_to "$destination" live
    deadline=$((SECONDS + 15))
    while (( SECONDS < deadline )); do
        state=$(ipc statusActivityState) || fail "operations: live transfer observer failed"
        jq -e '.errors == 0' <<< "$state" >/dev/null || fail "operations: unpaused transfer failed: $state"
        # The card draws the sample it published last (250 ms beat), so its byte line trails the activity text; wait on the line naming a total.
        if jq -e '.activities[0].running and .activities[0].text == "Copying 1 of 2 · a-large.bin" and (.transferCard.byteLine | contains(" of "))' <<< "$state" >/dev/null; then break; fi
        if jq -e '(.activities | length) == 0 and .notice != ""' <<< "$state" >/dev/null; then operations_missed_window transfer-progress "$state"; fi
        sleep 0.05
    done
    jq -e '.activities[0].running and .activities[0].text == "Copying 1 of 2 · a-large.bin" and .transferCard.visible' <<< "$state" >/dev/null \
        || fail "operations: no filename-bearing live transfer before deadline: $state"
    # Directive 45: the sweep runs beside the copy, so a batch names its total once it settles, and the loop above waited for that.
    jq -e '.transferCard.byteLine | contains(" of ")' <<< "$state" >/dev/null \
        || fail "operations: the batch card states no total, its line reads [$(jq -r '.transferCard.byteLine' <<< "$state")]"
    printf 'OPERATIONS_BATCH_TOTAL line=%s\n' "$(jq -r '.transferCard.byteLine' <<< "$state")"
    observed_bytes=$(stat -c '%s' "$destination/a-large.bin") || fail "operations: live destination byte count unavailable"
    (( observed_bytes > 0 && observed_bytes < operations_bytes )) || operations_missed_window transfer-before-capture "$state"
    printf 'OPERATIONS_LIVE_TRANSFER before_cancel_bytes=%s state=%s\n' "$observed_bytes" "$state"
    # The separately retained filename capture must not consume this real cancellation window.
    key -k Escape >/dev/null
    menus_expect statusActivityState '(.activities | length) == 0' "unpaused native Escape reaches a terminal transfer state"
    state=$(ipc statusActivityState) || fail "operations: live cancellation outcome unavailable"
    menus_equal "unpaused cancellation preserves source identity" "$source_identity" "$(stat -c '%d:%i:%s:%Y' "$source/a-large.bin")"
    menus_equal "unpaused cancellation preserves all source bytes" "$source_hash" "$(sha256sum < "$source/a-large.bin")"
    menus_equal "unpaused cancellation preserves later source" 'after cancellation' "$(cat "$source/b-after.txt")"
    jq -e '.errors == 0' <<< "$state" >/dev/null || fail "operations: live cancellation reported an error: $state"
    jq -e '.notice | contains("Copied 0 of 2") and contains("2 skipped") and contains("cancelled") and (contains("failed") | not)' <<< "$state" >/dev/null \
        || operations_missed_window transfer-cancellation "$state"
    operations_absent "$destination/a-large.bin"
    operations_absent "$destination/b-after.txt"
    # Completion refreshes the listing and resets selection in released 0.1.6 as well.
    menus_expect listInFlight '. == false' "completed cancellation finishes its listing refresh"
    wait_listing 2
    menus_equal "completed cancellation resets listing marks" "" "$(ipc selectedIndices)"
    menus_equal "completed cancellation resets listing cursor" 0 "$(ipc cursor)"
    shot operations-cancelled-unpaused
    printf 'OPERATIONS_CANCEL variant=escape unpaused_native=ok filename_observed=ok skipped=2 failed=0 partial_cleanup=ok source_preserved=ok attempts=1\n'
)

operations_search_live() (
    local source="$menu_box/search-live" query=flea-operations-no-match state footer deadline
    local directory_count=100000
    menus_guard "$source"
    mkdir "$source" || fail "operations: nested Search fixture creation failed"
    # Reuse case_nosweep's established directory count; no larger tree or second attempt follows a miss.
    python3 - "$menu_box" "$source" "$directory_count" <<'PY' || fail "operations: nested Search fixture could not be populated"
from pathlib import Path
import sys

sandbox, root = map(Path, sys.argv[1:3])
count = int(sys.argv[3])
if not sandbox.is_absolute() or not root.is_absolute() or not (sandbox / ".flea-test-sandbox").is_file():
    raise SystemExit("operations: nested Search needs an absolute marked sandbox")
if sandbox.resolve() != sandbox or root.resolve() != root or sandbox not in root.parents or any(root.iterdir()):
    raise SystemExit("operations: nested Search root is not an empty canonical child of its sandbox")
for index in range(1, count + 1):
    child = root / f"dir_{index}"
    if not child.is_absolute() or sandbox not in child.parents:
        raise SystemExit(f"operations: nested Search path escaped its sandbox: {child}")
    child.mkdir()
PY
    printf 'OPERATIONS_SEARCH_WORKLOAD directories=%s source=%q\n' "$directory_count" "$source"
    launch "$source"
    trap 'kill_flea' EXIT
    wait_listing "$directory_count"
    permissions_viewport 880 620
    key f >/dev/null
    menus_expect keyDeliveryState '.searchMode == "typing"' "live Search opens through native input"
    key "$query" -M ctrl -k Return -m ctrl >/dev/null
    deadline=$((SECONDS + 15))
    while (( SECONDS < deadline )); do
        state=$(ipc keyDeliveryState) || fail "operations: live Search observer failed"
        if jq -e '.searchMode == "results" and .searchRunning and .searchScanned > 0' <<< "$state" >/dev/null; then break; fi
        if jq -e '.searchMode == "results" and (.searchRunning | not)' <<< "$state" >/dev/null; then operations_missed_window search-progress "$state"; fi
        sleep 0.05
    done
    jq -e --arg query "$query" --argjson count "$directory_count" '.searchMode == "results" and .searchQuery == $query and .searchRunning and (.searchCancelled | not) and .searchScanned == $count' <<< "$state" >/dev/null \
        || fail "operations: Search did not expose the real positive scan before deadline: $state"
    footer=$(ipc statusFooterState) || fail "operations: live Search footer unavailable"
    if ! jq -e '(.centre.text | test("^[0-9]{1,3}(,[0-9]{3})* found · Searching, 100,000 scanned$")) and .secondary.text == ""' <<< "$footer" >/dev/null; then
        state=$(ipc keyDeliveryState) || fail "operations: Search state unavailable after footer mismatch"
        if jq -e '.searchMode == "results" and (.searchRunning | not)' <<< "$state" >/dev/null; then
            operations_missed_window search-footer "$state footer=$footer"
        fi
        fail "operations: running Search footer differs: state=$state footer=$footer"
    fi
    printf 'OPERATIONS_LIVE_SEARCH before_capture=%s footer=%s\n' "$state" "$footer"
    shot operations-search-positive-scanned-live
    state=$(ipc keyDeliveryState) || fail "operations: live Search post-capture observer failed"
    jq -e '.searchMode == "results" and .searchRunning and .searchScanned > 0 and (.searchCancelled | not)' <<< "$state" >/dev/null \
        || operations_missed_window search-after-capture "$state"
    printf 'OPERATIONS_LIVE_SEARCH after_capture=%s\n' "$state"
    key -k Escape >/dev/null
    menus_expect keyDeliveryState '.searchRunning | not' "native Escape reaches a terminal Search state"
    state=$(ipc keyDeliveryState) || fail "operations: live Search cancellation outcome unavailable"
    jq -e '.searchMode == "results" and .searchCancelled and .searchScanned == 100000' <<< "$state" >/dev/null \
        || operations_missed_window search-cancellation "$state"
    cardsize_expect total 0
    shot operations-search-cancelled-live
    key -k Escape >/dev/null
    menus_expect keyDeliveryState '.searchMode == ""' "second Escape closes cancelled live Search"
    wait_listing "$directory_count"
    operations_idle_footer "$directory_count" 0 "closing cancelled Search restores its real directory"
    printf 'OPERATIONS_SEARCH positive_scanned_live=ok filename_matches=0 cancelled_native=ok close=ok attempts=1 state=%s\n' "$state"
)

case_operationsdesign() (
    local menu_box menus_checks=0 path live_cancel=failed live_search=failed
    local operations_bytes=$((1024 * 1024 * 1024))
    sandbox_require "$fixture_root"
    menu_box=$(mktemp -d "$fixture_root/operations-design.XXXXXXXX") || fail "operations: fixture creation failed"
    printf 'native Operations fixture\n' > "$menu_box/.flea-test-sandbox"
    [[ "$menu_box" == "$(realpath -e "$menu_box")" ]] || fail "operations: fixture is not canonical"
    for path in state config cache data cancel-source; do menus_guard "$menu_box/$path"; mkdir "$menu_box/$path"; done
    export XDG_STATE_HOME="$menu_box/state" XDG_CONFIG_HOME="$menu_box/config" XDG_CACHE_HOME="$menu_box/cache" XDG_DATA_HOME="$menu_box/data"
    "$flea_bin" --ui-state '{"view":"list","keys":"default","display":{"textSize":{"mode":14}},"menu":{"hidden":[]}}' >/dev/null || fail "operations: fixture settings failed"
    if [[ "${1:-all}" != live ]]; then
        operations_missing_footer || fail "operations: missing-filesystem proof failed"
        operations_mixed || fail "operations: mixed-outcome proof failed"
        operations_copy_to_collision || fail "operations: Copy to collision proof failed"
        operations_long_error || fail "operations: long-name footer proof failed"
        operations_search_footer || fail "operations: search footer proof failed"
    fi
    menus_guard "$menu_box/cancel-source/a-large.bin"
    truncate -s "$operations_bytes" "$menu_box/cancel-source/a-large.bin"
    menus_guard "$menu_box/cancel-source/b-after.txt"
    printf 'after cancellation\n' > "$menu_box/cancel-source/b-after.txt"
    printf 'OPERATIONS_WORKLOAD bytes=%s source=%q\n' "$operations_bytes" "$menu_box/cancel-source/a-large.bin"
    if [[ "${1:-all}" == live ]]; then
        operations_cancel_live || fail "operations: unpaused cancellation proof failed"
        return
    fi
    operations_cancel pointer || fail "operations: interrupted pointer cancellation proof failed"
    operations_cancel escape || fail "operations: interrupted Escape cancellation proof failed"
    if operations_cancel_live; then live_cancel=ok; fi
    if operations_search_live; then live_search=ok; fi
    printf 'OPERATIONS_DESIGN mixed=ok retry=ok acknowledgement=ok undo=ok informational_footer=ok long_name_elision=ok search_initial=ok interrupted_pointer_cancel=ok interrupted_escape_cancel=ok unpaused_live=%s positive_scanned_live=%s visual_inspection=pending\n' "$live_cancel" "$live_search"
    [[ "$live_cancel" == ok && "$live_search" == ok ]] || fail "operations: required unpaused proof remains incomplete"
)

case_operationslive() { case_operationsdesign live; }

operations_footer_fixture() {
    local part
    sandbox_require "$fixture_root"
    menu_box=$(mktemp -d "$fixture_root/footer-$1.XXXXXXXX") || fail 'footer: cannot create fixture'
    printf 'native Footer fixture\n' > "$menu_box/.flea-test-sandbox" || fail 'footer: cannot mark fixture'
    [[ "$menu_box" == "$(realpath -e "$menu_box")" ]] || fail 'footer: fixture is not canonical'
    for part in state config cache data payload destination; do
        menus_guard "$menu_box/$part"
        mkdir "$menu_box/$part" || fail "footer: cannot create $part"
    done
    export XDG_STATE_HOME="$menu_box/state" XDG_CONFIG_HOME="$menu_box/config"
    export XDG_CACHE_HOME="$menu_box/cache" XDG_DATA_HOME="$menu_box/data"
    "$flea_bin" --ui-state '{"view":"list","keys":"default","display":{"textSize":{"mode":14}},"preview":{"thumbnails":"off","loadOn":"manual"},"menu":{"hidden":[]}}' >/dev/null \
        || fail 'footer: private settings seed failed'
}

operations_footer_capture() {
    local name="$1" expression="$2" before after
    operations_footer_geometry "$name"
    before=$(ipc statusFooterState) || fail "footer: $name pre-capture observation failed"
    jq -e "$expression" <<< "$before" >/dev/null || fail "footer: $name missed its state before capture: $before"
    menus_shot "footer-$name"
    after=$(ipc statusFooterState) || fail "footer: $name post-capture observation failed"
    jq -e "$expression" <<< "$after" >/dev/null || fail "footer: $name changed during capture: $after"
    printf 'FOOTER_CAPTURE name=%s before=%s after=%s inspection=pending\n' "$name" "$before" "$after"
}

case_footerstates() (
    local menu_box menus_checks=0 name
    operations_footer_fixture states
    for name in a.txt b.txt photo.heic y.txt z.txt keep-{1..5}.txt; do
        menus_guard "$menu_box/payload/$name"
        printf 'original %s\n' "$name" > "$menu_box/payload/$name" || fail "footer: cannot create $name"
    done
    # A real failure that is not a name: photo.heic cannot be read, so its copy fails and the other four land.
    menus_guard "$menu_box/destination/photo.heic"
    chmod 000 "$menu_box/payload/photo.heic" || fail 'footer: cannot make photo.heic unreadable'
    launch "$menu_box/payload"
    trap 'kill_flea' EXIT
    wait_listing 10
    permissions_viewport 880 620
    click_row "$(row_index_of photo.heic)" left
    operations_idle_footer 10 1 'ten-item selected idle specimen'
    operations_footer_capture idle '.total == 10 and .selected == 1 and (.left.text | startswith("1 of 10 selected")) and .disk.text == .filesystem and .centre.text == "" and .secondary.text == ""'
    # A lone selection follows plain cursor moves, so keep the click as a deliberate mark before seeking.
    key v >/dev/null || fail "footer: cannot keep photo.heic in the selection"
    for name in a.txt b.txt y.txt z.txt; do
        seek_row_named "$name"
        key v >/dev/null || fail "footer: cannot add $name to the selection"
    done
    menus_expect selectionCount '. == 5' 'mixed specimen selects five genuine originals'
    operations_copy_to "$menu_box/destination"
    menus_expect statusActivityState '.errors == 1 and (.activities | length) == 0 and (.notice | contains("Copied 4 of 5 · 1 failed"))' 'mixed specimen records its actual completed outcome'
    menus_expect selectionCount '. == 1' 'mixed specimen retains the failed original for retry'
    operations_footer_capture error-unreadable '(.left.text | startswith("1 of 10 selected")) and .centre.text == "Copy failed: photo.heic · permission denied" and .secondary.text == " · esc dismisses"'
    key -k Escape >/dev/null || fail 'footer: failure acknowledgement failed'
    menus_expect statusActivityState '.errors == 0 and .undoAvailable' 'acknowledgement reveals the actual undoable completion'
    operations_footer_capture completed-unreadable '(.left.text | startswith("1 of 10 selected")) and .centre.text == "Copied 4 of 5 · 1 failed" and .secondary.text == " · photo.heic selected for retry" and .undo.text == "z undoes"'
    for name in a.txt b.txt y.txt z.txt; do menus_same_file "committed $name" "$menu_box/payload/$name" "$menu_box/destination/$name"; done
    [[ ! -e "$menu_box/destination/photo.heic" && ! -L "$menu_box/destination/photo.heic" ]] || fail 'footer: the unreadable source left a copy behind'
    chmod 644 "$menu_box/payload/photo.heic" || fail 'footer: cannot make photo.heic readable again'
    printf 'FOOTER_LITERAL_GAP error=unreadable-source-not-ENOSPC completed=4-of-5,1-failed,0-skipped undo-and-retry-retained=true\n'
)

case_footertransfer() (
    local menu_box menus_checks=0 name before_bytes after_bytes operations_stopped=""
    local operations_bytes=$((1024 * 1024 * 1024)) permissions_listing
    operations_footer_fixture transfer
    permissions_listing="$menu_box/payload"
    for name in a-first.txt photo.heic z-after-{1..3}.txt zz-kept-{1..5}.txt; do
        menus_guard "$permissions_listing/$name"
        printf 'original %s\n' "$name" > "$permissions_listing/$name" || fail "footer: cannot create $name"
    done
    menus_guard "$permissions_listing/photo.heic"
    truncate -s "$operations_bytes" "$permissions_listing/photo.heic" || fail 'footer: cannot size transfer fixture'
    menus_guard "$menu_box/destination/photo.heic"
    launch "$permissions_listing"
    trap 'permissions_resume_stopped "$operations_stopped" || exit 1; kill_flea' EXIT
    wait_listing 10
    permissions_viewport 880 620
    click_row 0 left
    key -M shift -k Down -k Down -k Down -k Down -m shift >/dev/null || fail 'footer: five-item selection failed'
    cardsize_expect selectedIndices '0,1,2,3,4'
    operations_copy_to "$menu_box/destination" live
    menus_expect statusActivityState '.errors == 0 and .activities[0].running and .activities[0].text == "Copying 2 of 5 · photo.heic"' 'second genuine transfer item publishes its filename'
    operations_pause_backend
    before_bytes=$(stat -c '%s' "$menu_box/destination/photo.heic") || fail 'footer: partial destination unavailable'
    (( before_bytes > 0 && before_bytes < operations_bytes )) || fail 'footer: transfer finished before capture pause'
    click_row "$(row_index_of photo.heic)" left
    menus_expect selectionCount '. == 1' 'transfer specimen displays one native selection'
    # Directive 51, StatusBar rule 8: the card owns a transfer's progress and it is up whenever one
    # runs, so the strip stays at rest rather than drawing the same count and a second hairline.
    menus_expect statusActivityState '.activities[0].running and .transferCard.visible and (.transferCard.byteLine | length) > 0' 'the card is up and reporting the bytes it owns'
    menus_expect statusFooterState '.centre.text == "" and .secondary.text == ""' 'the strip stays at rest while the card reports'
    operations_footer_capture transfer '(.left.text | startswith("1 of 10 selected")) and .centre.text == "" and .secondary.text == ""'
    after_bytes=$(stat -c '%s' "$menu_box/destination/photo.heic") || fail 'footer: captured partial disappeared'
    menus_equal 'controlled capture keeps the same incomplete copy' "$before_bytes" "$after_bytes"
    printf 'FOOTER_TRANSFER controlled_pause=true bytes=%s total=%s unpaused_proof=existing-operationslive\n' "$after_bytes" "$operations_bytes"
    permissions_resume_stopped "$operations_stopped" || fail 'footer: cannot resume captured transfer'
    operations_stopped=""
    menus_expect statusActivityState '.errors == 0 and (.activities | length) == 0 and (.notice | contains("Copied 5 items"))' 'captured transfer resumes and completes all five originals'
    for name in a-first.txt photo.heic z-after-{1..3}.txt; do menus_same_file "completed $name" "$permissions_listing/$name" "$menu_box/destination/$name"; done
)

case_footersearch() (
    local menu_box menus_checks=0 query=footer-needle state
    local directory_count=100000 matched_count=10
    operations_footer_fixture search
    python3 - "$menu_box" "$directory_count" "$matched_count" "$query" <<'PY' || fail 'footer: Search fixture creation failed'
from pathlib import Path
import sys
root = Path(sys.argv[1])
payload = root / "payload"
if not root.is_absolute() or root.resolve() != root or not (root / ".flea-test-sandbox").is_file():
    raise SystemExit("footer: Search requires an absolute marked sandbox")
if payload.resolve() != payload or not payload.is_relative_to(root) or any(payload.iterdir()):
    raise SystemExit("footer: Search payload is not an empty canonical child")
for index in range(int(sys.argv[2])):
    (payload / f"dir-{index}").mkdir()
for index in range(int(sys.argv[3])):
    (payload / f"{sys.argv[4]}-{index}.txt").write_text(f"matched original {index}\n")
PY
    launch "$menu_box/payload"
    trap 'kill_flea' EXIT
    wait_listing "$((directory_count + matched_count))"
    permissions_viewport 880 620
    key f >/dev/null || fail 'footer: Search entry failed'
    menus_expect keyDeliveryState '.searchMode == "typing"' 'populated Search editor receives native focus'
    key "$query" -M ctrl -k Return -m ctrl >/dev/null || fail 'footer: Search submission failed'
    menus_expect keyDeliveryState '.searchMode == "results" and .searchRunning and .searchScanned == 100010 and (.searchCancelled | not)' 'real Search scans its populated root while descendant work remains'
    wait_listing 10
    [[ "$(ipc rowAt 0)" == "$query-"* ]] || fail 'footer: populated Search has not delivered an actual fixture match'
    operations_footer_capture search '.listingState == "ready" and .total == 10 and .selected == 0 and .left.text == "10 items" and (.centre.text | test("^10 found · Searching, 100,010 scanned$")) and .secondary.text == ""'
    state=$(ipc keyDeliveryState) || fail 'footer: captured Search state unavailable'
    jq -e '.searchRunning and (.searchCancelled | not)' <<< "$state" >/dev/null || fail 'footer: Search finished during its capture'
    key -k Escape >/dev/null || fail 'footer: Search cancellation failed'
    menus_expect keyDeliveryState '.searchMode == "results" and (.searchRunning | not) and .searchCancelled' 'populated Search remains cancellable after capture'
    wait_listing 10
    key -k Escape >/dev/null || fail 'footer: completed Search dismissal failed'
    menus_expect keyDeliveryState '.searchMode == ""' 'Search dismissal restores the source directory'
    wait_listing "$((directory_count + matched_count))"
    printf 'FOOTER_LITERAL_GAP search=100010-real-scanned-not-4120 matched_results=10\n'
)

case_footertrash() (
    local menu_box menus_checks=0 trash_box payload name index trash_checks=0
    local trash_parent_bus_id="" trash_private_bus_id="" trash_bus_address="" trash_bus_pid="" trash_provider_pid=""
    operations_footer_fixture trash
    trash_box="$menu_box" payload="$menu_box/payload"
    [[ "$(realpath -e "$(command -v gio)")" == /usr/bin/gio ]] || fail 'footer: private Trash requires the real GIO'
    for name in keep-{1..10}.txt trash-{1..4}.txt; do
        trash_guard "$payload/$name"
        printf 'original %s\n' "$name" > "$payload/$name" || fail "footer: cannot create $name"
    done
    trash_start_bus
    launch "$payload"
    wait_listing 14
    permissions_viewport 880 620
    trash_guard_store 0
    index=$(row_index_of trash-1.txt)
    click_row "$index" left
    key -M shift -k Down -k Down -k Down -m shift >/dev/null || fail 'footer: Trash selection failed'
    menus_expect selectionCount '. == 4' 'Trash specimen selects four real originals'
    for name in trash-{1..4}.txt; do trash_guard "$payload/$name"; done
    trash_guard_store 0
    key -k Delete >/dev/null || fail 'footer: native Move to Trash failed'
    wait_listing 10
    trash_wait '.count == 4 and (.busy | not)' 'four originals reach the private Trash provider'
    trash_guard_store 4
    operations_footer_capture trash '.total == 10 and .selected == 1 and (.left.text | startswith("1 of 10 selected")) and (.left.text | contains("·")) and .centre.text == "Moved 4 items to Trash" and .secondary.text == "" and .undo.text == "z undoes"'
    trash_guard_store 4
    key z >/dev/null || fail 'footer: native Trash Undo failed'
    wait_listing 14
    trash_wait '.count == 0 and (.busy | not)' 'native Undo restores the four private Trash originals'
    trash_guard_store 0
    for name in keep-{1..10}.txt trash-{1..4}.txt; do
        menus_same_file "Trash cycle preserves $name" <(printf 'original %s\n' "$name") "$payload/$name"
    done
)

case_footerspecimens() {
    case_footerstates || fail 'footer: idle and actual mixed-state captures failed'
    case_footertransfer || fail 'footer: second-file transfer capture failed'
    case_footersearch || fail 'footer: populated running Search capture failed'
    case_footertrash || fail 'footer: actual Trash transient capture failed'
    printf 'FOOTER_SPECIMENS captures=6 matched_visual_approval=pending disk_full=not_run literal_completion_difference=recorded old_dismiss_proof=reused\n'
}

operations_footer_full_mount() {
    local mount owner identity contents
    sandbox_require "$menu_box"
    [[ "$menu_box" == "$(realpath -e "$menu_box")" && -O "$menu_box" && ! -L "$menu_box/.flea-test-sandbox" && -O "$menu_box/.flea-test-sandbox" ]] \
        || fail 'footerdiskfull: prepared root or marker identity changed'
    menus_guard "$menu_box/full"
    [[ -d "$menu_box/full" && ! -L "$menu_box/full" ]] || fail 'footerdiskfull: full is not the prepared directory'
    owner=$(stat -c '%u:%g:%d:%i' "$menu_box/full") || fail 'footerdiskfull: mount ownership unavailable'
    [[ "$owner" == "$(id -u):$(id -g):"* ]] || fail 'footerdiskfull: mount does not belong to the native test user'
    mount=$(findmnt --json --bytes --mountpoint "$menu_box/full" --output TARGET,FSTYPE,SIZE,OPTIONS,ID,MAJ:MIN,SOURCE) \
        || fail 'footerdiskfull: exact prepared mount unavailable'
    # Sample findmnt row: {"target":"/owned/full","fstype":"tmpfs","size":1048576,"options":"rw,nodev,nosuid,noexec","id":123,"maj:min":"0:42","source":"tmpfs"}.
    identity=$(jq -ceS --arg target "$menu_box/full" --arg owner "$owner" '
        .filesystems | select(length == 1) | .[0] |
        select(.target == $target and .fstype == "tmpfs" and (.size | tonumber) == 1048576
            and (.options | split(",") | contains(["rw","nodev","nosuid","noexec"]))
            and .id != null and ."maj:min" != null) |
        {target,fstype,size,options,id,source,"maj:min":."maj:min",owner:$owner}' <<< "$mount") \
        || fail "footerdiskfull: expected exactly one 1MiB nodev,nosuid,noexec tmpfs: $mount"
    [[ -z "$full_identity" || "$identity" == "$full_identity" ]] || fail 'footerdiskfull: prepared mount identity changed'
    full_identity="$identity"
    if [[ "${1:-}" == empty ]]; then
        contents=$(find "$menu_box/full" -mindepth 1 -maxdepth 1 -print -quit) || fail 'footerdiskfull: cannot inspect initial destination'
        [[ -z "$contents" ]] || fail 'footerdiskfull: prepared destination is not empty'
    fi
    printf 'FOOTER_ENOSPC_MOUNT %s\n' "$identity"
}

operations_footer_full_cleanup() {
    local result="$1"
    trap - EXIT
    (operations_footer_full_mount) || result=1
    (kill_flea) || result=1
    printf 'FOOTER_ENOSPC_RETAINED root=%q mount=%q teardown_status=%s automatic_deletion=false\n' "$menu_box" "$menu_box/full" "$result"
    exit "$result"
}

case_footerdiskfull() (
    local menu_box="${FLEA_ENOSPC_ROOT:-}" menus_checks=0 full_identity="" fixture_root="$fixture_root"
    local part contents source source_identity source_hash partial_bytes error_color before_error
    local source_bytes=$((2 * 1024 * 1024))
    [[ -n "$menu_box" && "$menu_box" == /* ]] || fail 'footerdiskfull: FLEA_ENOSPC_ROOT must name the prepared absolute marked root'
    for part in "$fixture_root" "$thumb_fixture" "$hash_fixture" "$stale_fixture" "$run_root"; do
        [[ "$menu_box" != "$part" && "$menu_box" != "$part/"* ]] || fail 'footerdiskfull: prepared mount would enter automatic suite cleanup'
    done
    operations_footer_full_mount empty
    for part in state config cache data payload; do
        menus_guard "$menu_box/$part"
        [[ -d "$menu_box/$part" && ! -L "$menu_box/$part" && -O "$menu_box/$part" && -w "$menu_box/$part" ]] \
            || fail "footerdiskfull: missing or foreign prepared $part directory"
        contents=$(find "$menu_box/$part" -mindepth 1 -maxdepth 1 -print -quit) || fail "footerdiskfull: cannot inspect $part"
        [[ -z "$contents" ]] || fail "footerdiskfull: prepared $part is not empty"
    done
    kill_flea
    fixture_root="$menu_box"
    trap 'operations_footer_full_cleanup "$?"' EXIT
    export XDG_STATE_HOME="$menu_box/state" XDG_CONFIG_HOME="$menu_box/config"
    export XDG_CACHE_HOME="$menu_box/cache" XDG_DATA_HOME="$menu_box/data"
    "$flea_bin" --ui-state '{"view":"list","keys":"default","display":{"textSize":{"mode":14}},"preview":{"thumbnails":"off","loadOn":"manual"},"menu":{"hidden":[]}}' >/dev/null \
        || fail 'footerdiskfull: private settings seed failed'
    source="$menu_box/payload/photo.heic"
    menus_guard "$source"
    truncate -s "$source_bytes" "$source" || fail 'footerdiskfull: cannot create the 2MiB source'
    for part in keep-{1..9}.txt; do
        menus_guard "$menu_box/payload/$part"
        printf 'original %s\n' "$part" > "$menu_box/payload/$part" || fail "footerdiskfull: cannot create $part"
    done
    source_identity=$(stat -c '%d:%i:%f:%u:%g:%s:%Y:%Z' "$source") || fail 'footerdiskfull: source identity unavailable'
    source_hash=$(sha256sum < "$source") || fail 'footerdiskfull: source checksum unavailable'
    launch "$menu_box/payload"
    wait_listing 10
    permissions_viewport 880 620
    click_row "$(row_index_of photo.heic)" left
    operations_idle_footer 10 1 'disk-full source is the sole native selection'
    operations_footer_full_mount empty
    operations_copy_to "$menu_box/full"
    menus_expect statusActivityState '.errors == 1 and (.activities | length) == 0' 'actual bounded filesystem rejects the copy'
    menus_error 'Copy failed: photo.heic · the drive is full' 'native failure names the genuine ENOSPC cause'
    operations_footer_full_mount
    menus_guard "$menu_box/full/photo.heic"
    [[ -f "$menu_box/full/photo.heic" && ! -L "$menu_box/full/photo.heic" ]] || fail 'footerdiskfull: failed copy did not retain its regular partial'
    partial_bytes=$(stat -c '%s' "$menu_box/full/photo.heic") || fail 'footerdiskfull: partial byte count unavailable'
    (( partial_bytes > 0 && partial_bytes < source_bytes )) || fail "footerdiskfull: unexpected partial length $partial_bytes"
    menus_equal 'ENOSPC preserves source identity' "$source_identity" "$(stat -c '%d:%i:%f:%u:%g:%s:%Y:%Z' "$source")"
    menus_equal 'ENOSPC preserves every source byte' "$source_hash" "$(sha256sum < "$source")"
    menus_expect selectionCount '. == 1' 'failed original remains selected for retry'
    before_error=$(ipc statusPrimary) || fail 'footerdiskfull: error sentence unavailable'
    sleep "$transient_clear_s"
    menus_expect statusActivityState '.errors == 1' 'disk-full error survives the transient timeout'
    menus_equal 'unacknowledged disk-full sentence persists' "$before_error" "$(ipc statusPrimary)"
    error_color=$(ipc palette | cut -d' ' -f6) || fail 'footerdiskfull: semantic error role unavailable'
    [[ -n "$error_color" ]] || fail 'footerdiskfull: semantic error role is empty'
    menus_equal 'only the failure sentence uses the error role' "$error_color" "$(ipc statusColor)"
    operations_footer_capture disk-full '(.left.text | startswith("1 of 10 selected")) and .centre.text == "Copy failed: photo.heic · the drive is full" and (.centre.text | contains("(os error") | not) and .secondary.text == " · esc dismisses"'
    key -k Escape >/dev/null || fail 'footerdiskfull: native error acknowledgement failed'
    menus_expect statusActivityState '.errors == 0 and (.activities | length) == 0 and (.notice | contains("Copied 0 of 1 · 1 failed"))' 'Escape acknowledges ENOSPC and reveals the truthful outcome'
    operations_footer_full_mount
    menus_equal 'acknowledgement retains the partial for explicit recovery' "$partial_bytes" "$(stat -c '%s' "$menu_box/full/photo.heic")"
    printf 'FOOTER_ENOSPC native_entry=copy-to source_bytes=%s partial_bytes=%s error_persistent=true acknowledged=true retry_caption=retained fixture_preserved=true\n' "$source_bytes" "$partial_bytes"
)
