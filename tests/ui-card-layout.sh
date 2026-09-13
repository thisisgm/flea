# Sourced by ui.sh; reuse its guarded fixtures, exact window ownership, input and cleanup.
cardsize_expect() {
    local reader="$1" expected="$2" seen=unread end=$((SECONDS + 20))
    shift 2
    while (( SECONDS < end )); do
        seen=$(ipc "$reader" "$@") || fail "cardsizes: $reader failed"
        [[ "$seen" == "$expected" ]] && return
        sleep 0.05
    done
    fail "cardsizes: $reader expected '$expected', observed '$seen'"
}

cardsize_window_box() {
    local box
    box=$(window_box) || fail "cardsizes: native window geometry failed"
    [[ "$box" =~ ^-?[0-9]+\ -?[0-9]+\ [1-9][0-9]*\ [1-9][0-9]*$ ]] \
        || fail "cardsizes: invalid native window geometry: $box"
    printf '%s\n' "$box"
}

cardsize_rect() {
    local name="$1" rect="$2" x y width height wx wy ww wh box rounding=1
    read -r x y width height <<< "$rect"
    [[ "$rect" =~ ^-?[0-9]+\ -?[0-9]+\ [0-9]+\ [0-9]+$ ]] || fail "cardsizes: $name returned no valid rectangle: $rect"
    box=$(cardsize_window_box) || fail "cardsizes: $name has no native viewport"
    read -r wx wy ww wh <<< "$box"
    (( width > 0 && height > 0 && x >= -rounding && y >= -rounding && x + width <= ww + rounding && y + height <= wh + rounding )) \
        || fail "cardsizes: $name rectangle $rect leaves the ${ww}x${wh} viewport"
    printf 'CARD_RECT surface=%s viewport=%sx%s rect=%s\n' "$name" "$ww" "$wh" "$rect"
}

cardsize_dispatch() {
    local result
    result=$(hyprctl dispatch "$1" 2>&1) || fail "cardsizes: compositor dispatch failed: $result"
    [[ "$result" == ok* ]] || fail "cardsizes: compositor refused dispatch: $result"
}

cardsize_network() {
    [[ "$(ipc focusView)" == rail ]] || key -k Tab >/dev/null || fail "cardsizes: could not focus rail"
    cardsize_expect focusView rail
    key a >/dev/null || fail "cardsizes: Add Network key failed"
    cardsize_expect dialogOpen true
}

cardsize_focus() {
    local phase="$1" observed
    observed=$(ipc contextMenuFocusState) || fail "cardsizes: native focus observation failed"
    jq -e '(.opened | not) and (.menu | not) and .pane and .list and .view == "list"' <<< "$observed" >/dev/null \
        || fail "cardsizes: $phase lost listing keyboard focus: $observed"
    printf 'CARD_FOCUS phase=%s state=%s\n' "$phase" "$observed"
}

case_cardsizes() {
    local dir="$fixture_root/cardsizes" addr viewport initial_width initial_height wx wy ww wh index=0
    local base protocol network_rect before sy content visible after bx by bw bh cx cy focus last menu_count step box body eye rows menu_rect menu_width work_width
    sandbox_make "$dir"
    mkdir -p "$dir/listing" "$dir/config" "$dir/data" "$dir/bin" || fail "cardsizes: fixture creation failed"
    export XDG_CONFIG_HOME="$dir/config" XDG_DATA_HOME="$dir/data"
    seed_ui_state "$dir/state" '{"keys":"default","view":"list","preview":{"column":false,"thumbnails":"off"},"places":{"showNetwork":true},"display":{"textSize":{"mode":14}}}'
    magick -size 16x16 xc:white "$dir/listing/0-shot.png" || fail "cardsizes: could not create PNG fixture"
    for name in a b c d e f g h i j k l m n o p; do
        printf '%s\n' "$name" > "$dir/listing/$name.txt" || fail "cardsizes: text fixture creation failed"
    done
    # Geometry tests never open a file; refuse an accidental opener before it can reach an operator application.
    [[ "$open_handoff" == gio ]] || fail "cardsizes: unsupported file opener $open_handoff"
    FLEA_CARDSIZE_REAL_OPENER=$(command -v "$open_handoff") || fail "cardsizes: gio is missing"
    [[ "$FLEA_CARDSIZE_REAL_OPENER" == /* && -x "$FLEA_CARDSIZE_REAL_OPENER" ]] || fail "cardsizes: gio path is invalid"
    export FLEA_CARDSIZE_REAL_OPENER
    export FLEA_CARDSIZE_OPENER_RECEIPT="$dir/unexpected-opener"
    cat > "$dir/bin/$open_handoff" <<'OPENER' || fail "cardsizes: opener refusal could not be written"
#!/usr/bin/env bash
if [[ "${1:-}" != open ]]; then
    exec "$FLEA_CARDSIZE_REAL_OPENER" "$@"
fi
[[ -n "$FLEA_CARDSIZE_OPENER_RECEIPT" && "$FLEA_CARDSIZE_OPENER_RECEIPT" == /* && -f "${FLEA_CARDSIZE_OPENER_RECEIPT%/*}/.flea-test-sandbox" ]] || exit 64
printf '%s\n' "$*" > "$FLEA_CARDSIZE_OPENER_RECEIPT"
exit 1
OPENER
    chmod +x "$dir/bin/$open_handoff" || fail "cardsizes: opener refusal could not be installed"
    export PATH="$dir/bin:$PATH"
    launch "$dir/listing"
    wait_listing 17
    addr=$(hyprctl clients -j | jq -er --argjson pid "$(flea_pid)" '.[] | select(.pid == $pid) | .address') \
        || fail "cardsizes: owned native window is missing"
    [[ "$addr" =~ ^0x[0-9a-fA-F]+$ ]] || fail "cardsizes: invalid native window address"
    box=$(cardsize_window_box) || fail "cardsizes: initial viewport is missing"
    read -r wx wy initial_width initial_height <<< "$box"
    omarchy-drive window float "$addr" >/dev/null || fail "cardsizes: owned window could not float"

    # Preserve full, half, quarter, third and sixth shapes using the actual initial viewport, not an obsolete monitor layout.
    for viewport in "${initial_width}x${initial_height}" "$((initial_width / 2))x${initial_height}" \
        "$((initial_width / 2))x$((initial_height / 2))" "$((initial_width / 3))x${initial_height}" \
        "$((initial_width / 3))x$((initial_height / 2))" 560x400 fullscreen; do
        index=$((index + 1))
        if [[ "$viewport" == fullscreen ]]; then
            omarchy-drive window fullscreen "$addr" >/dev/null || fail "cardsizes: fullscreen failed"
        else
            cardsize_dispatch "hl.dsp.window.resize({ x = ${viewport%x*}, y = ${viewport#*x}, exact = true, window = \"address:$addr\" })"
            omarchy-drive window center "$addr" >/dev/null || fail "cardsizes: centering failed"
        fi
        settle
        box=$(cardsize_window_box) || fail "cardsizes: resized viewport is missing"
        read -r wx wy ww wh <<< "$box"
        [[ "$viewport" == fullscreen || "$ww $wh" == "${viewport%x*} ${viewport#*x}" ]] \
            || fail "cardsizes: expected $viewport, actual viewport ${ww}x${wh}"
        printf 'CARD_VIEWPORT index=%s requested=%s actual=%sx%s address=%s\n' "$index" "$viewport" "$ww" "$wh" "$addr"

        cardsize_focus "resized-$index"
        cardsize_network
        base=$(ipc networkChipCentre SMB)
        [[ "$base" =~ ^[0-9]+\ [0-9]+$ ]] || fail "cardsizes: SMB chip has no native centre"
        for protocol in SFTP FTPS WebDAV NFS SMB; do
            click_chip "$protocol" || fail "cardsizes: $protocol pointer activation failed"
            cardsize_expect networkProtocol "$protocol"
            [[ "$(ipc networkChipCentre SMB)" == "$base" ]] || fail "cardsizes: $protocol moved the Network chip row"
        done
        network_rect=$(ipc networkCardRect)
        cardsize_rect "network-$index" "$network_rect"
        shot "cardsizes-network-$index-$viewport"
        IFS='|' read -r sy content visible <<< "$(ipc networkScroll)"
        [[ "$sy $content $visible" =~ ^[0-9]+\ [0-9]+\ [0-9]+$ && "$visible" -gt 0 && "$content" -gt 0 ]] \
            || fail "cardsizes: Network returned invalid scrolling geometry"
        if (( content > visible )); then
            body=$(ipc networkBodyRect) || fail "cardsizes: Network body geometry failed"
            cardsize_rect "network-body-$index" "$body"
            read -r bx by bw bh <<< "$body"
            before=$(ipc listContentY)
            omarchy-drive move "$((wx + bx + bw / 2))" "$((wy + by + bh / 2))" >/dev/null || fail "cardsizes: wheel target failed"
            omarchy-drive scroll down 1 >/dev/null || fail "cardsizes: Network wheel failed"
            settle
            IFS='|' read -r after _ _ <<< "$(ipc networkScroll)"
            [[ "$after" =~ ^[0-9]+$ ]] || fail "cardsizes: Network returned invalid scroll position"
            (( after > sy )) || fail "cardsizes: clipped Network body did not scroll"
            [[ "$(ipc listContentY)" == "$before" ]] || fail "cardsizes: Network wheel reached the listing"
            shot "cardsizes-network-scrolled-$index-$viewport"
            key -k Escape >/dev/null || fail "cardsizes: Network dismissal failed"
            cardsize_expect dialogOpen false
            cardsize_network
            IFS='|' read -r sy _ _ <<< "$(ipc networkScroll)"
            [[ "$sy" == 0 ]] || fail "cardsizes: reopened Network did not start at the top"
            cardsize_expect networkFocus Host
            for focus in Port Share Domain Username Password; do
                key -k Tab >/dev/null || fail "cardsizes: Network Tab delivery failed"
                cardsize_expect networkFocus "$focus"
            done
            body=$(ipc networkBodyRect) || fail "cardsizes: Network body geometry failed"
            cardsize_rect "network-body-focused-$index" "$body"
            read -r bx by bw bh <<< "$body"
            eye=$(ipc networkPasswordEyeCentre) || fail "cardsizes: Password eye geometry failed"
            [[ "$eye" =~ ^-?[0-9]+\ -?[0-9]+$ ]] || fail "cardsizes: invalid Password eye geometry: $eye"
            read -r cx cy <<< "$eye"
            (( cx >= bx && cx <= bx + bw && cy >= by && cy <= by + bh )) \
                || fail "cardsizes: keyboard-focused Password is outside the clipped body"
            shot "cardsizes-network-password-$index-$viewport"
        fi
        key -k Escape >/dev/null || fail "cardsizes: Network dismissal failed"
        cardsize_expect dialogOpen false
        key -k Escape >/dev/null || fail "cardsizes: listing focus restoration failed"
        cardsize_expect focusView list

        key '?' >/dev/null || fail "cardsizes: Keymap key failed"
        cardsize_expect keymapSheetOpen true
        cardsize_rect "keymap-$index" "$(ipc keymapCardRect)"
        shot "cardsizes-keymap-$index-$viewport"
        key -k Escape >/dev/null || fail "cardsizes: Keymap dismissal failed"
        cardsize_expect keymapSheetOpen false

        key -k Home >/dev/null || fail "cardsizes: listing Home failed"
        settle
        [[ "$(ipc path)" == "$dir/listing" && "$(ipc rowAt 0)" == '0-shot.png|'* ]] \
            || fail "cardsizes: conversion source is not the owned PNG"
        click_row 0 right || fail "cardsizes: PNG context menu failed"
        cardsize_expect contextMenuVisible true
        last=$(menu_row_index Convert) || fail "cardsizes: the PNG offers no Convert action"
        menu_count=$(ipc contextMenuEntries | tr '|' '\n' | wc -l) || fail "cardsizes: menu inventory failed"
        for (( step = 0; step <= menu_count; step++ )); do
            [[ "$(ipc contextMenuCursor)" == "$last" ]] && break
            key -k Down >/dev/null || fail "cardsizes: Convert menu navigation failed"
            settle
        done
        [[ "$(ipc contextMenuCursor)" == "$last" ]] || fail "cardsizes: Convert action was never selected"
        key -k Return >/dev/null || fail "cardsizes: Convert activation failed"
        cardsize_expect convertOpen true
        cardsize_rect "convert-$index" "$(ipc convertCardRect)"
        shot "cardsizes-convert-$index-$viewport"
        key -k Escape >/dev/null || fail "cardsizes: Convert dismissal failed"
        cardsize_expect convertOpen false

        rows=$(ipc visibleRows) || fail "cardsizes: visible listing row count failed"
        [[ "$rows" =~ ^[0-9]+$ ]] || fail "cardsizes: invalid visible row count: $rows"
        last=$((rows - 1))
        (( last < 17 )) || last=16
        (( last >= 0 )) || fail "cardsizes: no visible listing row for menu placement"
        click_row "$last" right || fail "cardsizes: bottom-row context menu failed"
        cardsize_expect contextMenuVisible true
        menu_rect=$(ipc contextMenuRect) || fail "cardsizes: menu frame observation failed"
        cardsize_rect "bottom-menu-$index" "$menu_rect"
        read -r bx by bw bh <<< "$menu_rect"
        menu_width=$(token_of menuWidth) || fail "cardsizes: menu width token observation failed"
        work_width=$(ipc menuState | jq -er '.workArea.width | numbers') || fail "cardsizes: menu work area observation failed"
        jq -en --argjson observed "$bw" --argjson token "$menu_width" --argjson area "$work_width" --argjson rounding 1 \
            '([$token, $area] | all(type == "number" and . > 0)) and (($observed - ([$token, $area] | min | round) | fabs) <= $rounding)' >/dev/null \
            || fail "cardsizes: menu frame width $bw differs from token $menu_width clamped to work area $work_width"
        shot "cardsizes-menu-$index-$viewport"
        key -k Escape >/dev/null || fail "cardsizes: context menu dismissal failed"
        cardsize_expect contextMenuVisible false
        cardsize_focus "menu-dismissed-$index"
        key -k Tab >/dev/null || fail "cardsizes: restored listing Tab failed"
        cardsize_expect focusView rail
        key -M shift -k Tab -m shift >/dev/null || fail "cardsizes: restored rail Shift+Tab failed"
        cardsize_expect focusView list
        cardsize_focus "tab-cycle-$index"
        [[ ! -e "$FLEA_CARDSIZE_OPENER_RECEIPT" ]] || fail "cardsizes: unexpected file opener was refused"
        printf 'CARDSIZES viewport=%s network=ok protocols=5 scroll=ok keymap=ok convert=ok menu=ok\n' "$viewport"
    done
    kill_flea
}
