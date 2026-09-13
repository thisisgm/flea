#!/usr/bin/env bash
# Sourced after ui-menus.sh and ui-permissions.sh; shared helpers drive controls and guard backend ownership.
# shellcheck disable=SC2034,SC2154 # ui.sh supplies state; sourced helpers consume dynamically scoped locals.
convert_choose_format() {
    local format="$1" centre
    centre=$(ipc convertState | jq -er --arg format "$format" '.formats[] | select(.name == $format and .visible and .enabled) | .centre') \
        || fail "convert: requested format is not available"
    menus_point "$centre"
    menus_expect convertState ".opened and (.checking | not) and .format == \"$format\"" "format $format selects and probes without committing"
}

convert_pointer_state() {
    local stage="$1" compositor row
    # Sample native cursorpos: {"x":1280,"y":594}; MenuRow.probe reports hovered, point and resting point.
    compositor=$(hyprctl cursorpos -j | jq -ce '{x, y} | select((.x | type) == "number" and (.y | type) == "number")') \
        || fail "convert: cannot observe native pointer position"
    row=$(ipc convertState | jq -ce '.formats[] | select(.name == "webp" and (.pointerProbe | type) == "string") | {centre, rect, current, pointerProbe}') \
        || fail "convert: cannot observe WebP pointer handler"
    printf 'CONVERT_POINTER stage=%s compositor=%s row=%s\n' "$stage" "$compositor" "$row"
}

convert_pause_backend() {
    local pid="$1" state end=$((SECONDS + 15))
    permissions_backend_owned "$pid" || fail "convert: backend executable, fixture or session identity differs"
    kill -STOP "$pid" || fail "convert: owned backend could not pause"
    while (( SECONDS < end )); do
        # Sample process state: Tsl; its leading T confirms the owned backend stopped.
        state=$(ps -o stat= -p "$pid") || fail "convert: paused backend disappeared"
        [[ "$state" == T* ]] && return
        sleep 0.05
    done
    fail "convert: backend did not stop"
}

convert_backend_death() {
    local phase="$1" pid before request expected conversion_stopped=""
    local -a pids
    launch "$permissions_listing"
    wait_listing 3
    permissions_viewport
    menus_file_menu photo.png key
    menus_choose convert
    menus_expect convertState '.opened and (.checking | not)' "$phase failure fixture opens Convert"
    convert_choose_format avif
    menus_control convertState 'Remove metadata'
    menus_expect convertState '.canConvert and .strip' "$phase failure fixture has an editable draft"
    before=$(sha256sum < "$source")
    mapfile -t pids < <(backend_pids)
    [[ "${#pids[@]}" == 1 ]] || fail "convert: backend death requires one owned backend"
    pid="${pids[0]}"
    if [[ "$phase" == pending ]]; then
        conversion_stopped="$pid"
        trap 'permissions_resume_stopped "$conversion_stopped"' EXIT
        convert_pause_backend "$pid"
        menus_control convertState Convert
        menus_expect convertState '.busy and (.canConvert | not)' 'controlled pending conversion reached the paused backend'
        expected='(.error | contains("outcome is unknown") and contains("inspect the output"))'
    else
        expected='(.error | contains("Conversion is unavailable") and contains("restart Flea"))'
    fi
    request=$(ipc convertState | jq -er .requestId)
    permissions_backend_owned "$pid" || fail "convert: backend identity changed before the failure signal"
    kill -KILL "$pid" || fail "convert: owned backend could not terminate"
    conversion_stopped=""
    trap - EXIT
    menus_expect convertState ".opened and .unavailable and (.busy | not) and (.checking | not) and .strip and .format == \"avif\" and $expected" "$phase backend death retains the draft and reports an honest outcome"
    menus_expect convertState 'all(.formats[]; .enabled | not) and all(.controls[] | select(.name != "Cancel"); .enabled | not) and any(.controls[]; .name == "Cancel" and .enabled and .focused)' 'backend death disables format, metadata and Convert while Cancel keeps native focus'
    menus_point "$(ipc convertState | jq -er '.formats[] | select(.name == "webp") | .centre')"
    menus_control convertState 'Remove metadata'
    menus_control convertState Convert
    key -k Down -k Up >/dev/null
    menus_expect convertState ".opened and .unavailable and .requestId == $request and (.checking | not) and .strip and .format == \"avif\" and $expected" 'disabled clicks and keys cannot clear the error or probe a dead backend'
    key -k Tab >/dev/null
    key -M shift -k Tab -m shift >/dev/null
    menus_expect convertState 'any(.controls[]; .name == "Cancel" and .focused)' 'Tab in both directions keeps focus on the remaining live action'
    [[ "$(sha256sum < "$source")" == "$before" && ! -e "$permissions_listing/photo (converted).avif" ]] \
        || fail "convert: controlled backend death changed the paused fixture"
    shot "convert-backend-dead-$phase"
    menus_control convertState Cancel
    menus_expect convertState '.opened | not' 'Cancel remains usable after backend failure'
    menus_expect contextMenuFocusState '.list' 'failure dismissal restores actual listing focus'
}

case_convertdesign() (
    local menu_box menus_checks=0 permissions_listing convert_stopped="" path source jpeg webp contents state pid ui_pid request format cx cy wx wy ww wh row_width before_motion after_motion
    local -a pids
    sandbox_require "$fixture_root"
    menu_box=$(mktemp -d "$fixture_root/convert-design.XXXXXXXX") || fail "convert: fixture creation failed"
    printf 'native Convert fixture\n' > "$menu_box/.flea-test-sandbox"
    [[ "$menu_box" == "$(realpath -e "$menu_box")" ]] || fail "convert: fixture is not canonical"
    for path in state config cache data pictures; do menus_guard "$menu_box/$path"; mkdir "$menu_box/$path"; done
    permissions_listing="$menu_box/pictures"
    source="$permissions_listing/photo.png"
    jpeg="$permissions_listing/photo (converted).jpg"
    webp="$permissions_listing/photo (converted).webp"
    menus_guard "$source"
    magick -size 32x32 xc:steelblue -set comment 'Flea metadata fixture' "$source" || fail "convert: real image fixture creation failed"
    contents=$(sha256sum < "$source")
    menus_guard "$jpeg"
    printf 'original JPEG collision\n' > "$jpeg"
    export XDG_STATE_HOME="$menu_box/state" XDG_CONFIG_HOME="$menu_box/config" XDG_CACHE_HOME="$menu_box/cache" XDG_DATA_HOME="$menu_box/data"
    "$flea_bin" --ui-state '{"view":"list","keys":"default","menu":{"hidden":[]}}' >/dev/null || fail "convert: fixture settings failed"
    launch "$permissions_listing"
    wait_listing 2
    permissions_viewport
    menus_file_menu photo.png key
    menus_choose convert
    menus_expect convertState '.opened and (.checking | not) and .collision and (.canConvert | not) and .source.menuId > 0' 'initial output collision disables Convert with a captured menu identity'
    state=$(ipc convertState)
    [[ "$(jq -r .source.path <<< "$state")" == "$source" && "$(jq -r .outputText <<< "$state")" == "Output: $jpeg" ]] \
        || fail "convert: source or full output path differs from the captured item"
    menus_expect convertState '.error == "photo (converted).jpg already exists" and .outputLines > 1' 'collision names the output and its full path wraps'
    menus_expect convertState "all(.formats[]; .labelColor == \"$(ipc themeForeground)\") and any(.formats[]; .selected and .markColor == \"$(ipc palette | cut -d' ' -f5)\")" 'format labels retain foreground while the chosen glyph takes the accent role'
    shot convert-existing-output
    menus_control convertState Convert
    menus_expect convertState '.opened and .collision and (.busy | not)' 'disabled Convert cannot activate'
    request=$(ipc convertState | jq -er .requestId)
    read -r cx cy < <(ipc convertState | jq -er '.formats[] | select(.name == "webp") | .centre')
    row_width=$(ipc convertState | jq -er '.formats[] | select(.name == "webp") | .rect | split(" ")[2] | tonumber')
    read -r wx wy ww wh < <(window_box) || fail "native window coordinates unavailable"
    (( cx > 1 && cy > 0 && cx < ww && cy < wh )) || fail "convert: hover target is outside the owned viewport"
    assert_focus
    printf 'CONVERT_POINTER_TARGET window=%s,%s,%s,%s centre=%s,%s\n' "$wx" "$wy" "$ww" "$wh" "$cx" "$cy"
    convert_pointer_state before-warp
    omarchy-drive move "$((wx + cx))" "$((wy + cy))" >/dev/null || fail "convert: hover entry failed"
    convert_pointer_state after-warp
    # A compositor warp needs a native frame; the second position stays inside the measured row.
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool mousemove -x 1 -y 0 >/dev/null 2>&1 \
        || fail "convert: native hover entry failed"
    settle
    convert_pointer_state after-entry
    before_motion=$(hyprctl cursorpos -j | jq -c '{x,y}') || fail "convert: cannot observe resting pointer"
    omarchy-drive move "$((wx + cx + row_width / 4))" "$((wy + cy))" >/dev/null || fail "convert: within-row motion failed"
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool mousemove -x 1 -y 0 >/dev/null 2>&1 \
        || fail "convert: actual pointer movement failed"
    after_motion=$(hyprctl cursorpos -j | jq -c '{x,y}') || fail "convert: cannot observe moved pointer"
    [[ "$after_motion" != "$before_motion" ]] || fail "convert: native pointer did not move"
    convert_pointer_state after-motion
    menus_expect convertState ".requestId == $request and .format == \"jpg\" and .collision and any(.formats[]; .name == \"webp\" and .current and (.selected | not))" 'actual pointer motion highlights a format without changing the draft or probing'
    key -k Up >/dev/null
    menus_expect convertState '.format == "png" and (.checking | not) and any(.formats[]; .name == "png" and .focused and .current)' 'keyboard chooses from the moved cursor without a resting pointer stealing selection'
    for format in jpg png webp avif heic tiff bmp; do convert_choose_format "$format"; done
    [[ "$(ipc total)" == 2 && "$(sha256sum < "$source")" == "$contents" ]] || fail "convert: selecting supported formats changed the fixture"
    convert_choose_format webp
    menus_expect convertState '.canConvert and (.collision | not) and .error == "" and any(.formats[]; .name == "webp" and .focused)' 'new format clears collision and takes native focus'
    [[ ! -e "$webp" && "$(sha256sum < "$source")" == "$contents" ]] || fail "convert: format selection wrote output or changed source"
    key -k Tab >/dev/null
    menus_expect convertState 'any(.controls[]; .name == "Remove metadata" and .focused)' 'Tab enters the metadata checkbox'
    key -k space >/dev/null
    menus_expect convertState '.strip' 'Space changes the actual metadata draft'
    key -k Tab >/dev/null
    menus_expect convertState 'any(.controls[]; .name == "Cancel" and .focused)' 'Tab reaches Cancel'
    key -k Tab >/dev/null
    menus_expect convertState 'any(.controls[]; .name == "Convert" and .focused)' 'Tab reaches Convert'
    key -k Tab >/dev/null
    menus_expect convertState 'any(.formats[]; .name == "webp" and .focused)' 'Tab remains within the dialog'
    key -M shift -k Tab -m shift >/dev/null
    menus_expect convertState 'any(.controls[]; .name == "Convert" and .focused)' 'reverse Tab remains within the dialog'
    shot convert-ready

    ui_pid=$(flea_pid)
    mapfile -t pids < <(pgrep -P "$ui_pid" -x flea)
    [[ "${#pids[@]}" == 1 ]] || fail "convert: pending test needs one owned backend child"
    pid="${pids[0]}"
    convert_stopped="$pid"
    trap 'permissions_resume_stopped "$convert_stopped"' EXIT
    convert_pause_backend "$pid"
    key -k Return >/dev/null
    menus_expect convertState '.opened and .busy and .strip and .format == "webp" and (.canConvert | not) and all(.controls[]; .enabled | not)' 'activation retains draft and disables controls while backend waits'
    key -k Escape >/dev/null
    menus_control convertState Cancel
    menus_expect convertState '.opened and .busy and .strip and .format == "webp"' 'pending Escape and disabled Cancel cannot discard the draft'
    menus_guard "$webp"
    printf 'late WebP collision\n' > "$webp"
    shot convert-pending
    permissions_resume_stopped "$convert_stopped" || fail "convert: owned backend did not resume"
    convert_stopped=""
    menus_expect convertState '.opened and (.busy | not) and .collision and .strip and .format == "webp" and (.canConvert | not)' 'activation rechecks a late collision and retains every input'
    [[ "$(cat "$webp")" == 'late WebP collision' && "$(sha256sum < "$source")" == "$contents" ]] \
        || fail "convert: late collision or source was overwritten"
    shot convert-late-collision
    menus_guard "$webp"
    menus_guard "$menu_box/late-collision-kept"
    mv -- "$webp" "$menu_box/late-collision-kept"
    convert_choose_format webp
    menus_expect convertState '.canConvert and .strip and .error == ""' 'reselecting format probes repaired output without losing metadata choice'
    menus_guard "$webp"
    key -k Return >/dev/null
    menus_expect convertState '.opened | not' 'only attributed conversion success closes the dialog'
    menus_error 'photo (converted).webp already exists' 'successful retry preserves the unacknowledged collision'
    menus_acknowledge
    menus_message 'Converted to photo (converted).webp.' 'matching conversion result reaches Operations'
    [[ "$(magick identify -format '%m' "$webp")" == WEBP ]] || fail "convert: output is not a real WebP image"
    [[ "$(sha256sum < "$source")" == "$contents" && "$(cat "$jpeg")" == 'original JPEG collision' \
        && "$(cat "$menu_box/late-collision-kept")" == 'late WebP collision' ]] || fail "convert: conversion changed an original file"
    [[ -z "$(magick identify -format '%c' "$webp")" ]] || fail "convert: Remove metadata retained the fixture comment"
    menus_expect contextMenuFocusState '.list' 'successful close restores native listing focus'
    shot convert-success
    menus_file_menu photo.png key
    menus_choose convert
    menus_expect convertState '.opened and (.checking | not) and .collision and (.strip | not)' 'reopening resets defaults while checking the existing output'
    key -k Escape >/dev/null
    menus_expect convertState '.opened | not' 'Escape cancels an idle conversion draft'
    permissions_viewport 480 240
    menus_file_menu photo.png key
    menus_choose convert
    menus_expect convertState '.opened and (.checking | not) and .collision' 'Convert retains its collision state in a small viewport'
    cardsize_rect Convert "$(ipc convertCardRect)"
    key -k Tab >/dev/null
    menus_expect convertState '.scrollY > 0 and any(.controls[]; .name == "Remove metadata" and .focused)' 'small viewport scrolls to the keyboard-focused metadata control'
    menus_control convertState 'Remove metadata'
    menus_expect convertState '.strip' 'revealed metadata control accepts an actual pointer click in the small viewport'
    shot convert-small-metadata
    key -k Tab >/dev/null
    menus_expect convertState 'any(.controls[]; .name == "Cancel" and .focused)' 'small viewport reaches its available dismissal action'
    shot convert-small-cancel
    key -k Return >/dev/null
    menus_expect convertState '.opened | not' 'small viewport cancels through the actual focused control'
    convert_backend_death idle
    convert_backend_death pending
    kill_flea
    printf 'CONVERT_DESIGN native_checks=%s collision=ok selection=ok keyboard=ok pending=ok late_collision=ok success=ok metadata=ok backend_death=ok source_preserved=ok\n' "$menus_checks"
)
