# #454: launch with Tight before the first listing, then reach the last row after live density changes.
CASES+=("tight-list-window 1100x700 view:list"
        "tight-columns-window 1100x700 view:columns"
        "tight-grid-window 1100x700 view:grid")

# A moved cursor gets this many reads, this far apart, to paint its row: a 6 s ceiling.
tight_drawn_attempts=60
tight_drawn_interval=0.1

# visibleRowName reads painted rows, so a cursor read alone can precede the repaint it needs.
wait_drawn() {
    local attempt
    for attempt in $(seq 1 "$tight_drawn_attempts"); do
        [ "$(ipc visibleRowName "$1")" = "$2" ] && return 0
        sleep "$tight_drawn_interval"
    done
    return 1
}

tight_window() (
    [ "$VISUAL_FRONTEND" = gpui ] || return 77
    local view=$1 root=$C/tight-window/files last=469 tight_pitch=23 compact_pitch=31 index density pitch
    local last_name; last_name=file-$(printf %03d "$last").txt
    mkdir -p "$root" || return 1
    : > "$root/.flea-test-sandbox" || return 1
    for index in $(seq -f %03g 0 "$last"); do
        printf 'Tight window fixture\n' > "$root/file-$index.txt" || return 1
    done
    touch -d "$STAMP" "$root"/*.txt || return 1
    LAUNCH_STATE=',"density":"tight","display":{"textSize":{"mode":14}}' launch "$view" "$root" || return 1
    wait_ipc total "$((last + 1))" || return 1
    wait_ipc state ready || return 1
    wait_ipc fileRowHeight "$tight_pitch" || return 1
    for density in compact tight; do
        "$flea_bin" --ui-state "{\"density\":\"$density\"}" > /dev/null 2>> "$case_notes" || return 1
        pitch=$tight_pitch
        [ "$density" != compact ] || pitch=$compact_pitch
        wait_ipc fileRowHeight "$pitch" || return 1
        key -k Home || return 1
        wait_ipc cursor 0 || return 1
        wait_drawn 0 file-000.txt || {
            state_failure "$view $density did not draw its first row"
            return 1
        }
        key -k End || return 1
        wait_ipc cursor "$last" || return 1
        wait_drawn "$last" "$last_name" || {
            state_failure "$view $density did not draw its last row"
            return 1
        }
        ipc statusActivityState | jq -e '.errors == 0 and (.notice | contains("could not be completed") | not)' > /dev/null || {
            state_failure "$view $density reported a listing error"
            return 1
        }
    done
    snap
)

case_tight_list_window() { tight_window list; }
case_tight_columns_window() { tight_window columns; }
case_tight_grid_window() { tight_window grid; }
