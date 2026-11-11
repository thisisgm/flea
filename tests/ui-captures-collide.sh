# Sourced by ui.sh, run only by name (ui:cap_collide): issue 261's collision cards, every button state, on four themes.
. "$repo/tests/lib/pointer-glide.sh"

# ydotool's left button down and up codes, the park inset off the window corner, and the move that leaves a held button.
cap_collide_left_down=0x40
cap_collide_left_up=0x80
cap_collide_park_inset=12
cap_collide_leave_y=-200
cap_collide_fixture() {
    local dir="$1"
    sandbox_scratch "$dir"
    mkdir -p "$dir/src/trip" "$dir/Pictures/trip"
    printf 'src photo one\n' > "$dir/src/trip/IMG_4120.HEIC"
    printf 'src photo two\n' > "$dir/src/trip/IMG_4122.HEIC"
    printf 'there photo one\n' > "$dir/Pictures/trip/IMG_4120.HEIC"
    printf 'src memo\n' > "$dir/src/memo.txt"
    printf 'there memo\n' > "$dir/Pictures/memo.txt"
}
cap_collide_listed() {
    wait_listing "$(find "$1" -mindepth 1 -maxdepth 1 | wc -l)"
}
# Yank one source row and paste it into Pictures: the card is up when this returns.
cap_collide_paste() {
    local dir="$1" name="$2"
    seek_row_named "$name" || fail "cap_collide: $name never came under the cursor"
    key y >/dev/null
    settle
    key -k BackSpace >/dev/null
    wait_path "$dir"
    seek_row_named "Pictures" || fail "cap_collide: Pictures never came under the cursor"
    key -k Return >/dev/null
    wait_path "$dir/Pictures"
    cap_collide_listed "$dir/Pictures"
    key p >/dev/null
    menus_expect collideState ".opened and .title == \"$name already exists in Pictures.\" and .focus == \"keep\"" "the card for $name opens on Keep both"
}
cap_collide_back() {
    local dir="$1"
    key -k Escape >/dev/null
    menus_expect collideState '.opened | not' "Escape cancels the card"
    key -k BackSpace >/dev/null
    wait_path "$dir"
    seek_row_named "src" || fail "cap_collide: src never came under the cursor"
    key -k Return >/dev/null
    wait_path "$dir/src"
    cap_collide_listed "$dir/src"
}
# The pointer goes to the window's lower left corner, over the ground and off every button.
cap_collide_park() {
    local wx wy _ww wh
    read -r wx wy _ww wh < <(window_box) || fail "cap_collide: native window coordinates unavailable"
    pointer_glide "$((wx + cap_collide_park_inset))" "$((wy + wh - cap_collide_park_inset))" || fail "cap_collide: pointer park failed"
    settle
}
# Hover, then a held press let go off the button, each asserted from collideState before its shot.
cap_collide_pointer() {
    local button="$1" name="$2" centre cx cy wx wy
    centre=$(ipc collideState | jq -er --arg b "$button" '.buttons[] | select(.name == $b and .visible) | .centre') \
        || fail "cap_collide: no visible $button button to point at"
    read -r cx cy <<< "$centre"
    read -r wx wy _ww _wh < <(window_box) || fail "cap_collide: native window coordinates unavailable"
    pointer_glide "$((wx + cx))" "$((wy + cy))" || fail "cap_collide: pointer move failed"
    menus_expect collideState "[.buttons[] | select(.name == \"$button\") | .hovered] == [true]" "$button reports the hover"
    settle
    shot "$name-hover-$button"
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool click "$cap_collide_left_down" >/dev/null 2>&1 || fail "cap_collide: press failed"
    menus_expect collideState "[.buttons[] | select(.name == \"$button\") | .pressed] == [true]" "$button reports the press"
    settle
    shot "$name-pressed-$button"
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool mousemove -x 0 -y "$cap_collide_leave_y" >/dev/null 2>&1 || fail "cap_collide: move off failed"
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool click "$cap_collide_left_up" >/dev/null 2>&1 || fail "cap_collide: release failed"
    menus_expect collideState '.opened' "a press let go off $button acts on nothing"
    printf 'CAP_COLLIDE_POINTER %s %s\n' "$name" "$button"
}
# One open card, pointer parked: rest, then focus walked to every button (the card has no disabled button to shoot).
cap_collide_card() {
    local name="$1" focus step walked=" "
    cap_collide_park
    shot "$name-rest"
    local left=3
    [[ "$(ipc collideState | jq -r '[.buttons[] | select(.name == "merge" and .visible)] | length')" == 0 ]] || left=4
    for ((step = 0; step < left; step++)); do key h >/dev/null; done
    menus_expect collideState '.focus == "cancel"' "h reaches the leftmost Cancel on $name"
    for step in 1 2 3 4 5 6 7 8; do  # a card has fewer than 8 buttons, so a Keep Both out of reach fails instead of looping
        focus=$(ipc collideState | jq -r '.focus')
        [[ "$walked" != *" $focus "* ]] || fail "cap_collide: the focus walk on $name came round to $focus before it reached keep"
        walked+="$focus "
        settle
        shot "$name-focus-$focus"
        [[ "$focus" != keep ]] || break
        key l >/dev/null
        menus_expect collideState ".focus != \"$focus\"" "l leaves $focus on $name"
    done
    [[ "$focus" == keep ]] || fail "cap_collide: l never reached Keep Both on $name"
    key -k Escape >/dev/null
    menus_expect collideState '.opened | not' "Escape cancels the $name card"
}
# The card was cancelled after the focus walk: back to src and paste the same row again for the pointer.
cap_collide_paste_again() {
    local dir="$1" name="$2"
    key -k BackSpace >/dev/null
    wait_path "$dir"
    seek_row_named "src" || fail "cap_collide: src never came under the cursor"
    key -k Return >/dev/null
    wait_path "$dir/src"
    cap_collide_listed "$dir/src"
    cap_collide_paste "$dir" "$name"
    cap_collide_park
}
cap_collide_theme() {
    local theme="$1" dir="$fixture_root/cap-collide-$1" home="$fixture_root/cap-collide-home-$1" real_home="$HOME" state_theme
    local live_foreground="$real_foreground" name button buttons want
    cap_collide_fixture "$dir"
    fixture_home_make "$home"
    state_theme="$home/.local/state/omarchy/current"
    local real_state_dir="$state_theme"
    if [[ "$theme" != live ]]; then
        cp "/usr/share/omarchy/themes/$theme/colors.toml" "$state_theme/theme/colors.toml" || fail "cap_collide: no $theme palette"
        printf '%s\n' "$theme" > "$state_theme/theme.name"
        # Sample input, one line of colors.toml: foreground = "#dcd7ba"
        real_foreground=$(grep -E '^foreground' "$state_theme/theme/colors.toml" | grep -oE '#[0-9A-Fa-f]{6}')
    fi
    seed_ui_state "$fixture_root/cap-collide-state-$theme" '{"keys":"default","view":"list"}'
    export HOME="$home"
    launch "$dir/src"
    export HOME="$real_home"
    cap_collide_listed "$dir/src"
    cap_resize 1100 700
    [[ "$(ipc metrics)" == "13 12 14 37" ]] || fail "cap_collide: metrics $(ipc metrics), not text size 14"
    for name in trip memo.txt; do
        cap_collide_paste "$dir" "$name"
        cap_collide_card "collide-$theme-${name%%.*}"
        cap_collide_paste_again "$dir" "$name"
        buttons=$(ipc collideState | jq -er '[.buttons[] | select(.visible) | .name] | join(" ")') || fail "cap_collide: no button list on $name"
        # The folder card offers Merge and the file card does not.
        want="cancel skip merge replace keep"
        [[ "$name" == trip ]] || want="cancel skip replace keep"
        [[ "$buttons" == "$want" ]] || fail "cap_collide: $name shows buttons '$buttons', not '$want'"
        for button in $buttons; do
            cap_collide_pointer "$button" "collide-$theme-${name%%.*}"
        done
        cap_collide_back "$dir"
    done
    printf 'CAP_COLLIDE theme=%s accent=%s foreground=%s\n' "$theme" "$(grep -E '^accent' "$state_theme/theme/colors.toml" | grep -oE '#[0-9A-Fa-f]{6}')" "$real_foreground"
    real_foreground="$live_foreground"
    kill_flea
}
case_cap_collide() {
    local theme menus_checks=0
    for theme in ${CAP_COLLIDE_THEMES:-live matte-black solitude kanagawa}; do
        cap_collide_theme "$theme"
    done
}
