# Sourced by ui.sh; the existing fixture, process and display owners cover every operation here.
case_settingscompact() {
    local dir="$fixture_root/settingscompact" addr viewport section baseline before scroll last
    local wx wy ww wh cx cy cw ch rx ry
    sandbox_scratch "$dir"
    mkdir -p "$dir/listing" "$dir/state/flea" "$dir/config"
    : > "$dir/listing/proof.txt"
    export XDG_STATE_HOME="$dir/state" XDG_CONFIG_HOME="$dir/config"
    printf '%s\n' '{"keys":"default","view":"list","display":{"textSize":{"mode":14}}}' > "$dir/state/flea/ui.json"
    launch "$dir/listing"
    wait_listing 1
    addr=$(hyprctl -j clients | jq -er --argjson pid "$(flea_pid)" '.[] | select(.pid == $pid) | .address')
    [[ "$addr" =~ ^0x[0-9a-fA-F]+$ ]] || fail "settingscompact: missing owned window"
    omarchy-drive window float flea >/dev/null

    for viewport in 1100x800 800x600 560x400 480x240; do
        hyprctl dispatch "hl.dsp.window.resize({ x = ${viewport%x*}, y = ${viewport#*x}, exact = true, window = \"address:$addr\" })" >/dev/null
        omarchy-drive window center flea >/dev/null
        settle
        read -r wx wy ww wh < <(window_box) || fail "native window coordinates unavailable"
        [[ "$ww $wh" == "${viewport%x*} ${viewport#*x}" ]] || fail "settingscompact: wrong viewport $ww $wh"
        settings_open_key
        settle
        settings_section view
        baseline=$(ipc settingsCardRect)
        read -r cx cy cw ch <<< "$baseline"
        (( cx >= 8 && cy >= 8 && cx + cw <= ww - 8 && cy + ch <= wh - 8 )) \
            || fail "settingscompact: card outside viewport $baseline"
        (( 2 * cx + cw >= ww - 1 && 2 * cx + cw <= ww + 1 && 2 * cy + ch >= wh - 1 && 2 * cy + ch <= wh + 1 )) \
            || fail "settingscompact: card not centered $baseline in $viewport"
        ipc settingsScrollState | jq -e '.compactHeight > 0 and (.compactHeight == .pane.contentHeight)' >/dev/null \
            || fail "settingscompact: stable height is not measured View content"
        if [[ "$viewport" == 1100x800 ]]; then
            scroll=$(ipc settingsScrollState)
            # Border subtraction can differ by one floating-point rounding step, never a pixel tolerance.
            jq -e '((.pane.height - .compactHeight) | fabs) <= (.compactHeight * pow(2; -52))' <<< "$scroll" >/dev/null \
                || fail "settingscompact: View has unused space or unexpected scrolling: $scroll"
        fi
        shot "settings-view-$viewport"
        settings_focus_row columns
        key -k Return >/dev/null
        settle
        [[ "$(ipc settingsSection)" == columns && "$(ipc settingsCardRect)" == "$baseline" ]] \
            || fail "settingscompact: Columns subpage changed card geometry"
        settings_focus_row backView
        key -k Return >/dev/null
        settle

        for section in places preview keys display menus about; do
            settings_section "$section"
            [[ "$(ipc settingsCardRect)" == "$baseline" ]] || fail "settingscompact: $section moved card"
            read -r rx ry <<< "$(ipc settingsRailRowCentre "$section")"
            (( rx > cx && rx < cx + cw && ry > cy && ry < cy + ch )) \
                || fail "settingscompact: selected $section rail row is clipped"
            shot "settings-$section-$viewport"
        done

        settings_section menus
        scroll=$(ipc settingsScrollState)
        jq -e '.pane.contentHeight > .pane.height and .pane.y == 0' <<< "$scroll" >/dev/null \
            || fail "settingscompact: long Menus section has no scrolling $scroll"
        before=$(ipc listContentY)
        omarchy-drive move "$((wx + cx + cw - 30))" "$((wy + cy + ch / 2))" >/dev/null
        omarchy-drive scroll down 1 >/dev/null
        settle
        ipc settingsScrollState | jq -e '.pane.y > 0' >/dev/null || fail "settingscompact: wheel did not scroll Menus"
        [[ "$(ipc listContentY)" == "$before" ]] || fail "settingscompact: wheel reached underlying listing"
        last=$(ipc settingsModel | jq -er '[.[] | select(.kind == "check")][-1].id')
        settings_focus_row "$last"
        read -r rx ry <<< "$(ipc settingsRowCentre "$last")"
        (( ry > cy && ry < cy + ch )) || fail "settingscompact: keyboard focus remains below visible card"
        shot "settings-menus-scrolled-$viewport"
        key -k Escape >/dev/null
        settle
        settings_open_key
        settle
        [[ "$(ipc settingsSection)" == menus && "$(ipc settingsCardRect)" == "$baseline" ]] \
            || fail "settingscompact: reopen forgot section or size"
        ipc settingsScrollState | jq -e '.pane.y == 0' >/dev/null || fail "settingscompact: reopened first control is offscreen"
        key -k Escape >/dev/null
        settle
        [[ "$(ipc focusView)" == list ]] || fail "settingscompact: close did not restore listing focus"
        printf 'SETTINGS_COMPACT viewport=%s card=%s centered=ok stable=ok columns=ok sections=7 wheel=ok focus=ok reopen=ok\n' "$viewport" "$baseline"
    done
    kill_flea
}
