# Search typing owns listing controls; Grid uses the existing disabled toolbar recipe.
CASES+=("search-chrome-grid 1100x700 key:search"
        "search-chrome-list 1100x700 key:search"
        "search-chrome-columns 1100x700 key:search")

# A held press is read this many times, this far apart, so a late press delivery cannot pass unseen.
search_chrome_press_reads=5
search_chrome_press_interval=0.1
# An enabled hover gets this many reads at the same interval: a 2 s ceiling.
search_chrome_hover_reads=20

# Glides to a toolbar control's window-relative centre; refuses a missing or non-integer point.
search_chrome_point() {
    local glyph=$1 at centre ax ay x y
    # Sample win_json .at: [640,370]; sample chromeButtonCentre reply: "512 40".
    at=$(win_json | jq -r '.at | "\(.[0]) \(.[1])"') || { state_failure "search chrome: no window position"; return 1; }
    centre=$(ipc chromeButtonCentre "$glyph") || { state_failure "search chrome: $glyph centre read failed"; return 1; }
    [[ "$at" =~ ^-?[0-9]+\ -?[0-9]+$ && "$centre" =~ ^[0-9]+\ [0-9]+$ ]] \
        || { state_failure "search chrome: $glyph has no painted centre: at='$at' centre='$centre'"; return 1; }
    read -r ax ay <<< "$at"
    read -r x y <<< "$centre"
    glide_to "$((ax + x))" "$((ay + y))"
}

search_chrome() {
    local mode=$1 glyph before state
    [ "$VISUAL_FRONTEND" = gpui ] || return 77
    launch "$mode" "$W" || return 1
    if [ "$mode" = grid ]; then
        # The same points hover an enabled Filter and Sort first, so the disabled reads below are known to be over them.
        for glyph in filter sort; do
            search_chrome_point "$glyph" || return 1
            state=""
            for _i in $(seq 1 "$search_chrome_hover_reads"); do
                state=$(ipc chromeButtonState "$glyph") && jq -e '.enabled and .hovered' <<< "$state" >/dev/null && break
                state=""
                sleep "$search_chrome_press_interval"
            done
            [ -n "$state" ] || { state_failure "search chrome: enabled $glyph never drew hover"; return 1; }
        done
    fi
    key -M ctrl f -m ctrl || return 1
    wait_ipc_has keyDeliveryState '"searchMode":"typing"' || return 1
    before=$(ipc statusActivityState | jq -er '.notice') || return 1
    if [ "$mode" = grid ]; then
        for glyph in filter sort; do
            state=$(ipc chromeButtonState "$glyph") || return 1
            jq -e '.visible and (.enabled | not)' <<< "$state" >/dev/null || return 1
            search_chrome_point "$glyph" || return 1
            ydotool click 0x40 >/dev/null 2>&1 || return 1
            for _i in $(seq 1 "$search_chrome_press_reads"); do
                state=$(ipc chromeButtonState "$glyph") \
                    && jq -e '(.enabled or .hovered or .pressed) | not' <<< "$state" >/dev/null \
                    || { ydotool click 0x80 >/dev/null 2>&1; state_failure "search chrome: disabled $glyph drew a state: $state"; return 1; }
                sleep "$search_chrome_press_interval"
            done
            ydotool click 0x80 >/dev/null 2>&1 || return 1
            [ "$(ipc statusActivityState | jq -er '.notice')" = "$before" ] || return 1
            wait_ipc_has keyDeliveryState '"searchMode":"typing"' || return 1
        done
    else
        # List and Columns draw no toolbar Filter or Sort; their availability is the gridchrome Rust tests'.
        for glyph in filter sort; do
            ipc chromeButtonState "$glyph" | jq -e '(.visible or .enabled) | not' >/dev/null || return 1
        done
    fi
    snap
}

case_search_chrome_grid() ( search_chrome grid )
case_search_chrome_list() ( search_chrome list )
case_search_chrome_columns() ( search_chrome columns )
