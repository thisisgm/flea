# The tab body keeps its plate in either setting; only its close mark brightens under the pointer, as v0.3.12 draws.
# Sample ipc tabCentre reply: "294 41".
tab_close_point() {
    local x0 y0 x1 y1 close_slot=24
    read -r x0 y0 <<< "$(ipc tabCentre 0)"
    read -r x1 y1 <<< "$(ipc tabCentre 1)"
    [[ "$x0 $y0 $x1 $y1" =~ ^[0-9]+\ [0-9]+\ [0-9]+\ [0-9]+$ ]] || return 1
    [ "$y0" = "$y1" ] && (( x1 - x0 >= 2 * close_slot )) || return 1
    # Two adjacent centres give the painted width; the close target is the centre of the trailing close slot.
    printf '%s %s\n' "$((x0 + (x1 - x0) / 2 - close_slot / 2))" "$y0"
}

# The pair shows the close target hovered and at rest, with both Highlight on hover settings covered.
tab_pointer_shot() (
    local target=$1 on=$2 at attempt paint_polls=60 poll_seconds=0.1
    [ "$VISUAL_FRONTEND" = gpui ] || return 77
    LAUNCH_STATE=",\"highlightHover\":$on" launch list "$W" || return 1
    size_window 1100 700 || return 1
    key t || return 1
    wait_ipc tabCount 2 || return 1
    wait_ipc tabIndex 1 || return 1
    wait_ipc tabBarVisible true || return 1
    [[ "$target" = close || "$target" = body ]] || return 1
    for attempt in $(seq 1 "$paint_polls"); do
        if at=$(tab_close_point); then
            [ "$target" != body ] || at=$(ipc tabCentre 0)
            [[ "$at" =~ ^[0-9]+\ [0-9]+$ ]] && break
        fi
        at=
        sleep "$poll_seconds"
    done
    [ -n "$at" ] || { state_failure "tab pointer target is not painted"; return 1; }
    hover_rest_shot "$at"
)

# A second, older row makes a real keyboard move visible without changing any other Trash fixture.
trash_keyboard_shot() {
    local trash=${XDG_DATA_HOME:-$HOME/.local/share}/Trash
    [ "$VISUAL_FRONTEND" = gpui ] || return 77
    mkdir -p "$trash/files" "$trash/info" || return 1
    printf 'a keyboard draft\n' > "$trash/keyboard-draft.txt" || return 1
    printf '[Trash Info]\nPath=%s/keyboard-draft.txt\nDeletionDate=2025-06-13T09:30:00\n' "$W" \
        > "$trash/info/keyboard-draft.txt.trashinfo" || return 1
    touch -d "$STAMP" "$trash/keyboard-draft.txt" "$trash/info/keyboard-draft.txt.trashinfo" || return 1
    # A live gvfsd-trash reads metadata when a file arrives, so publish only the complete file after its info.
    mv "$trash/keyboard-draft.txt" "$trash/files/keyboard-draft.txt" || return 1
    fxpolish_trash 2 || return $?
    key g || return 1
    wait_ipc_has trashState '"cursor":0,' || return 1
    key -k Down || return 1
    wait_ipc_has trashState '"cursor":1,' || return 1
    wait_ipc focusStop list || return 1
    # trashState: {"opened":true,"total":2,"busy":false,"cursor":1,"rows":[{"original":"/fx/v/work/old-draft.txt"},...]}.
    ipc trashState | jq -e --arg path "$W/keyboard-draft.txt" \
        '.opened and .total == 2 and .busy == false and .cursor == 1 and .rows[1].original == $path' > /dev/null || {
        state_failure "keyboard cursor did not reach the second Trash row"
        return 1
    }
    snap
}
