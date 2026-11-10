# Navigate by the rail's semantic entries; painted labels can be absent during a source refresh.
open_trash() {
    local entries target step
    launch list "$W" || return 1
    key -k Tab || return 1
    wait_ipc focusView rail || return 1
    entries=$(ipc railEntries) || { state_failure "railEntries reader failed"; return 1; }
    # railEntries: [{"label":"Home","path":"/home/u","group":"home","kind":"home",...},{"label":"Trash",...,"kind":"trash","count":1}]
    target=$(jq -er 'map(.kind) | index("trash") // empty' <<< "$entries") || {
        state_failure "rail has no Trash entry in $(cut -c1-160 <<< "$entries")"
        return 1
    }
    key g || return 1
    wait_ipc railCursor 0 || return 1
    for ((step=1; step<=target; step++)); do
        key j || return 1
        wait_ipc railCursor "$step" || return 1
    done
    # A source refresh can reorder the rail between the read above and activation.
    entries=$(ipc railEntries) || { state_failure "railEntries reader failed before activation"; return 1; }
    jq -e --argjson row "$target" '.[$row].kind == "trash"' <<< "$entries" > /dev/null || {
        state_failure "rail row $target is no longer Trash in $(cut -c1-160 <<< "$entries")"
        return 1
    }
    key -k Return || return 1
    wait_ipc_has trashState '"opened":true' || return 1
    wait_ipc_has trashState '"total":1,' || return 1
    wait_ipc_has trashState '"busy":false'
}
