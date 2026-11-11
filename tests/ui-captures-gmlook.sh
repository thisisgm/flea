#!/usr/bin/env bash
# Native rail and held-network-control evidence, sourced by ui.sh on 0.3.9, 0.3.10 and the stage.

cap_gm_cleanup() {
    local status=0
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool click 0x80 >/dev/null 2>&1 || status=1
    kill_flea || status=1
    [[ -n "$cap_gm_root" && "$cap_gm_root" == /* && ! -L "$cap_gm_root" && -f "$cap_gm_root/$SANDBOX_MARKER" ]] \
        || { printf 'REFUSED: gmlook cleanup lost its marked absolute root\n' >&2; return 1; }
    sandbox_remove "$cap_gm_root" || return 1
    return "$status"
}

cap_gm_fixture() {
    local name="$1" view="$2" folder file
    command -v magick >/dev/null || fail "gmlook: magick is required for crops"
    command -v ydotool >/dev/null || fail "gmlook: ydotool is required for pointer holds"
    sandbox_require "$fixture_root/cap-gm-$name"
    cap_gm_root=$(mktemp -d "$fixture_root/cap-gm-$name.XXXXXXXX") || fail "gmlook: cannot create fixture sandbox"
    [[ -n "$cap_gm_root" && "$cap_gm_root" == /* ]] || fail "gmlook: fixture root is not absolute"
    : > "$cap_gm_root/$SANDBOX_MARKER" || fail "gmlook: cannot mark fixture sandbox"
    # The case subshell has no previous EXIT trap; release runs even when a press or a shot fails.
    trap 'cap_gm_status=$?; cap_gm_cleanup || cap_gm_status=1; exit "$cap_gm_status"' EXIT
    fixture_home_make "$cap_gm_root/home"
    mkdir -p "$cap_gm_root"/{data,cache,undo} || fail "gmlook: cannot create isolated stores"
    chmod 0700 "$cap_gm_root/undo" || fail "gmlook: cannot protect undo store"
    for folder in Downloads Pictures Videos; do
        mkdir -p "$cap_gm_root/home/$folder" || fail "gmlook: cannot create $folder"
        for file in alpha.txt notes.md readme.txt; do
            printf '%s fixture\n' "$folder" > "$cap_gm_root/home/$folder/$file" || fail "gmlook: cannot seed $folder/$file"
        done
    done
    # Every tree lists the rail's places from user-dirs.dirs, so the fixture names its own three.
    mkdir -p "$cap_gm_root/home/.config" || fail "gmlook: cannot create the fixture config"
    printf 'XDG_%s_DIR="$HOME/%s"\n' DOWNLOAD Downloads PICTURES Pictures VIDEOS Videos \
        > "$cap_gm_root/home/.config/user-dirs.dirs" || fail "gmlook: cannot write user-dirs.dirs"
    for file in notes.md todo.txt welcome.txt; do
        printf 'gmlook fixture\n' > "$cap_gm_root/home/$file" || fail "gmlook: cannot seed $file"
    done
    export XDG_CONFIG_HOME="$cap_gm_root/home/.config" XDG_DATA_HOME="$cap_gm_root/data"
    export XDG_CACHE_HOME="$cap_gm_root/cache" FLEA_UNDO_DIR="$cap_gm_root/undo"
    seed_ui_state "$cap_gm_root/state" '{"keys":"default","view":"'"$view"'","columnsLimit":3,"openMode":"double","clickRename":false,"places":{"showHome":true,"showTrash":true},"display":{"textSize":{"mode":14}}}'
    HOME="$cap_gm_root/home" launch "$cap_gm_root/home"
    wait_listing 6
    cap_resize 1100 900
    settle
    [[ "$(ipc viewMode)" == "$view" ]] || fail "gmlook: the requested $view view did not land"
    printf 'CAP_GM_FIXTURE case=%s root=%s version=%s\n' "$name" "$cap_gm_root" "$(git -C "$repo" describe --always --tags 2>/dev/null || printf unknown)"
}

# The compositor warp needs a real uinput frame before Qt sees hover or an off-control release.
cap_gm_move() {
    local x="$1" y="$2" wx wy ww wh px py
    read -r wx wy ww wh < <(window_box) || fail "gmlook: native window coordinates unavailable"
    assert_focus
    hypr_cursor_move "$((wx + x - 1))" "$((wy + y))" || fail "gmlook: pointer warp failed"
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool mousemove -x 1 -y 0 >/dev/null 2>&1 \
        || fail "gmlook: pointer frame failed"
    read -r px py < <(hyprctl cursorpos | tr -d ',')
    [[ "$px $py" =~ ^-?[0-9]+\ -?[0-9]+$ ]] || fail "gmlook: pointer position unavailable"
    (( px >= wx + x - 2 && px <= wx + x + 2 && py >= wy + y - 2 && py <= wy + y + 2 )) \
        || fail "gmlook: pointer missed $x,$y, observed $px,$py in layout coordinates"
}

cap_gm_wait() {
    local reader="$1" want="$2" seen="" end=$((SECONDS + 20))
    shift 2
    while (( SECONDS < end )); do
        seen=$(ipc "$reader" "$@") || fail "gmlook: cannot read $reader"
        [[ "$seen" == "$want" ]] && return 0
        sleep 0.05
    done
    fail "gmlook: $reader expected [$want], observed [$seen]"
}

# Scale window coordinates to screenshot pixels, keeping the complete requested rectangle.
cap_gm_crop() {
    local name="$1" suffix="$2" x="$3" y="$4" w="$5" h="$6" wx wy ww wh pw ph right bottom destination
    read -r wx wy ww wh < <(window_box) || fail "gmlook: cannot read window size for crop"
    [[ "$x $y $w $h" =~ ^[0-9]+\ [0-9]+\ [0-9]+\ [0-9]+$ ]] || fail "gmlook: invalid crop rectangle"
    (( w > 0 && h > 0 && x + w <= ww && y + h <= wh )) || fail "gmlook: crop leaves the window"
    read -r pw ph < <(magick identify -format '%w %h\n' "$evidence_dir/$name.png") \
        || fail "gmlook: cannot read screenshot size"
    [[ "$pw $ph" =~ ^[1-9][0-9]*\ [1-9][0-9]*$ ]] || fail "gmlook: invalid screenshot size"
    right=$((((x + w) * pw + ww - 1) / ww)); bottom=$((((y + h) * ph + wh - 1) / wh))
    x=$((x * pw / ww)); y=$((y * ph / wh)); w=$((right - x)); h=$((bottom - y))
    (( w > 0 && h > 0 )) || fail "gmlook: empty scaled crop"
    destination="$evidence_dir/$name-$suffix.png"
    [[ ! -e "$destination" && ! -L "$destination" ]] || fail "gmlook: refusing existing crop $destination"
    magick "$evidence_dir/$name.png" -crop "${w}x${h}+${x}+${y}" +repage "$destination" \
        || fail "gmlook: cannot create crop $destination"
    [[ -s "$destination" ]] || fail "gmlook: empty crop $destination"
    printf 'CROP %s\n' "$destination"
}

cap_gm_rail_shot() {
    local state="$1" details rail x width wx wy ww wh
    details=$(ipc railDetails) || fail "gmlook: rail geometry unavailable"
    rail=$(ipc railState) || fail "gmlook: rail state unavailable"
    printf 'CAP_GM_RAIL state=%s focus=%s cursor=%s path=%s rows=%s\n' "$state" "$(ipc focusView)" "$(ipc railCursor)" "$(ipc path)" \
        "$(jq -ec '[.rows[] | {label, y: .rect.y, height: .rect.height, rect}]' <<< "$details")"
    read -r x width < <(jq -er 'select(.hidden == false) | [((.x // 0) | round), (.width | round)] | @tsv' <<< "$rail") \
        || fail "gmlook: rail is hidden or has no rectangle"
    read -r wx wy ww wh < <(window_box) || fail "gmlook: window rectangle unavailable"
    shot "gm-rail-$state"
    cap_gm_crop "gm-rail-$state" rail "$x" 0 "$width" "$wh"
}

case_cap_gm_rail() {
    local downloads trash cx cy
    cap_gm_root=""
    cap_gm_fixture rail columns
    cap_gm_wait columnCount 3
    wait_rail_label Downloads
    wait_rail_label Trash
    cap_gm_move 550 4
    settle
    printf 'CAP NOTE rail fill, keyboard frame and row hover have no common state IPC; compare the six native shots.\n'
    cap_gm_rail_shot 1-rest
    downloads=$(rail_row_of Downloads)
    click_rail_row "$downloads" left
    wait_path "$cap_gm_root/home/Downloads"
    wait_listing 3
    cap_gm_move 550 4
    settle
    cap_gm_wait focusView list
    cap_gm_rail_shot 2-downloads-off
    read -r cx cy <<< "$(ipc railRowCentre "$downloads")"
    [[ "$cx $cy" =~ ^[0-9]+\ [0-9]+$ ]] || fail "gmlook: Downloads centre unavailable"
    cap_gm_move "$cx" "$cy"
    settle
    cap_gm_rail_shot 3-downloads-hover
    cap_gm_move 550 4
    key -k Tab >/dev/null || fail "gmlook: Tab failed"
    cap_gm_wait focusView rail
    key -k Down >/dev/null || fail "gmlook: Down failed"
    settle
    cap_gm_rail_shot 4-rail-down
    click_row 0 left
    cap_gm_wait focusView list
    settle
    cap_gm_rail_shot 5-list-click
    trash=$(rail_row_of Trash)
    click_rail_row "$trash" left
    local observed end=$((SECONDS + 20))
    while (( SECONDS < end )); do
        observed=$(ipc trashState) || fail "gmlook: Trash state unavailable"
        jq -e '.opened and (.busy == false)' <<< "$observed" >/dev/null && break
        sleep 0.05
    done
    jq -e '.opened and (.busy == false)' <<< "$observed" >/dev/null || fail "gmlook: Trash did not open and settle"
    cap_gm_move 550 4
    settle
    cap_gm_rail_shot 6-trash
}

# Network.html's 16 px padding and 12 px gaps, with the shipped Dropbox footer beneath the action row.
cap_gm_network_recipe() {
    local tokens metrics padding spacing row padding_y gap hairline body small caption size footer=0 installed=0
    # Sample input: "412 237 480 360" (x y width height), from Ipc.cardRectOf via boot/shell.qml rectOf.
    read -r cap_gm_card_x cap_gm_card_y cap_gm_card_w cap_gm_card_h <<< "$(ipc networkCardRect)"
    [[ "$cap_gm_card_x $cap_gm_card_y $cap_gm_card_w $cap_gm_card_h" =~ ^[0-9]+\ [0-9]+\ [1-9][0-9]*\ [1-9][0-9]*$ ]] \
        || fail "gmlook: network card rectangle unavailable"
    tokens=$(ipc tokens) || fail "gmlook: network tokens unavailable"
    metrics=$(ipc networkDialogMetrics) || fail "gmlook: network padding and gap unavailable"
    # Sample input: "16|12" (rounded card.padding|body.spacing), from NetworkDialog.qml formMetrics.
    IFS='|' read -r padding spacing <<< "$metrics"
    row=$(cap_gm_token rowHeight "$tokens"); padding_y=$(cap_gm_token rowPaddingY "$tokens")
    gap=$(cap_gm_token gap "$tokens"); hairline=$(cap_gm_token hairline "$tokens")
    body=$(cap_gm_token body "$tokens"); small=$(cap_gm_token bodySmall "$tokens")
    caption=$(cap_gm_token caption "$tokens"); size=$(cap_gm_token columnSize "$tokens")
    [[ "$padding $spacing $row $padding_y $gap $hairline $body $small $caption $size" =~ ^[0-9]+(\ [0-9]+){9}$ ]] \
        || fail "gmlook: network recipe has invalid metrics"
    (( body > 0 && small > 0 && spacing > 0 )) || fail "gmlook: network recipe has empty font or gaps"
    cap_gm_button_h=$((row - padding_y))
    # The nine-character default size column measures this monospace face; the seed clears stored widths.
    cap_gm_advance=$((size * 1000 * body / (9 * small)))
    cap_gm_button_pad=$((2 * gap + 2 * hairline))
    cap_gm_inset=$((padding + hairline))
    command -v dropbox-cli >/dev/null && installed=1
    cap_gm_dropbox=$((1 - installed))
    if (( cap_gm_dropbox )); then
        # OEM Ui/PanelSectionHeader.qml:21 uses native Text height; 5/4 remains the existing capture estimate, not an OEM token.
        local -r caption_line_estimate_px=$(((caption * 5 + 3) / 4))
        # OEM Ui/PanelSectionHeader.qml:30 sets topPadding to Math.ceil(fontSize * 0.15).
        local -r caption_top_padding_px=$(((caption * 15 + 99) / 100))
        # NetworkDialog.qml:339 and :373 use Style.space(12) outside and Style.space(10) inside the Dropbox section.
        local -r dropbox_section_gap_px=$(((spacing * 10 + 6) / 12))
        footer=$((2 * spacing + 1 + caption_line_estimate_px + caption_top_padding_px + dropbox_section_gap_px + cap_gm_button_h))
    fi
    cap_gm_action_y=$((cap_gm_card_y + cap_gm_card_h - cap_gm_inset - footer - cap_gm_button_h / 2))
    cap_gm_right=$((cap_gm_card_x + cap_gm_card_w - cap_gm_inset))
    printf 'CAP NOTE no common Network DialogButton geometry or pressed IPC; button centres use Network.html card offsets and the shipped footer recipe. Inspect gm-net-ready and control crops.\n'
    printf 'CAP_GM_RECIPE card=%s,%s,%s,%s padding=%s spacing=%s footer=%s buttonHeight=%s dropboxVisible=%s\n' \
        "$cap_gm_card_x" "$cap_gm_card_y" "$cap_gm_card_w" "$cap_gm_card_h" "$padding" "$spacing" "$footer" "$cap_gm_button_h" "$cap_gm_dropbox"
}

# Sample input: "rowHeight=33\nrowPaddingY=8\ngap=6\nhairline=1" from ipc tokens.
cap_gm_token() {
    local line
    while IFS= read -r line; do
        if [[ "$line" == "$1="* ]]; then printf '%s\n' "${line#*=}"; return 0; fi
    done <<< "$2"
    return 1
}

# The owned geometry was checked before press; grim avoids driver lookup time inside the hold.
cap_gm_hold_shot() {
    local name="$1" wx="$2" wy="$3" ww="$4" wh="$5" png="$evidence_dir/$1.png"
    [[ ! -e "$png" && ! -L "$png" ]] || fail "gmlook: refusing existing screenshot $png"
    mkdir -p "$evidence_dir" || fail "gmlook: cannot create evidence directory"
    grim -g "$wx,$wy ${ww}x${wh}" "$png" >/dev/null || fail "gmlook: fast screenshot failed for $name"
    [[ -s "$png" ]] || fail "gmlook: fast screenshot is empty for $name"
    printf 'SHOT %s\n' "$png"
}

# Crops happen after release, so ImageMagick cannot delay the four samples during the hold.
cap_gm_hold() {
    local name="$1" label="$2" cx="$3" cy="$4" width="$5" source="$6"
    local wx wy ww wh left top right bottom start now delay target begun ended protocol suffix
    # Capture policy: one sample during Buttons.PRESS_MS (150 ms), then three settled samples during the hold.
    local -r press_animation_ms=80 settled_early_ms=400 settled_middle_ms=900 settled_late_ms=1600
    local -ra hold_targets_ms=("$press_animation_ms" "$settled_early_ms" "$settled_middle_ms" "$settled_late_ms")
    read -r wx wy ww wh < <(window_box) || fail "gmlook: native window rectangle unavailable"
    [[ "$cx $cy $width" =~ ^[0-9]+\ [0-9]+\ [1-9][0-9]*$ ]] || fail "gmlook: invalid control centre"
    (( cx > cap_gm_card_x && cx < cap_gm_card_x + cap_gm_card_w && cy > cap_gm_card_y && cy < cap_gm_card_y + cap_gm_card_h )) \
        || fail "gmlook: $label centre leaves the card"
    left=$((cx - width / 2 - 12)); top=$((cy - cap_gm_button_h / 2 - 12))
    right=$((left + width + 24)); bottom=$((top + cap_gm_button_h + 24))
    (( left >= 0 )) || left=0
    (( top >= 0 )) || top=0
    (( right <= ww )) || right=$ww
    (( bottom <= wh )) || bottom=$wh
    printf 'CAP_GM_CONTROL label=%q centre=%s,%s crop=%s,%s,%s,%s source=%s\n' "$label" "$cx" "$cy" \
        "$left" "$top" "$((right - left))" "$((bottom - top))" "$source"
    protocol=$(ipc networkProtocol) || fail "gmlook: protocol unavailable before hold"
    cap_gm_move "$cx" "$cy"
    settle
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool click 0x40 >/dev/null 2>&1 || fail "gmlook: $label press failed"
    start=$(date +%s%3N)
    for target in "${hold_targets_ms[@]}"; do
        now=$(date +%s%3N); delay=$((start + target - now))
        if (( delay > 0 )); then sleep "$(printf '%d.%03d' "$((delay / 1000))" "$((delay % 1000))")"; fi
        begun=$(date +%s%3N)
        cap_gm_hold_shot "gm-net-$name-$target" "$wx" "$wy" "$ww" "$wh"
        ended=$(date +%s%3N)
        printf 'CAP_GM_HOLD control=%s target_ms=%s capture_start_ms=%s capture_done_ms=%s\n' "$name" "$target" "$((begun - start))" "$((ended - start))"
        if (( begun - start > target + 100 )); then printf 'CAP NOTE %s sample %s ms was late because earlier captures were still running.\n' "$name" "$target"; fi
    done
    cap_gm_move "$((cap_gm_card_x + cap_gm_inset / 2))" "$((cap_gm_card_y + cap_gm_inset / 2))"
    cap_gm_hold_shot "gm-net-$name-off-held" "$wx" "$wy" "$ww" "$wh"
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool click 0x80 >/dev/null 2>&1 || fail "gmlook: $label release failed"
    settle
    cap_gm_hold_shot "gm-net-$name-released" "$wx" "$wy" "$ww" "$wh"
    cap_gm_wait dialogOpen true
    cap_gm_wait networkProtocol "$protocol"
    ipc networkFocusState | jq -e '.busy == false' >/dev/null || fail "gmlook: $label hold started an action"
    [[ "$(ipc networkStatus)" == "" ]] || fail "gmlook: $label hold submitted the form"
    for suffix in "${hold_targets_ms[@]}" off-held released; do
        cap_gm_crop "gm-net-$name-$suffix" control "$left" "$top" "$((right - left))" "$((bottom - top))"
    done
}

case_cap_gm_nethold() {
    local protocol cx cy width cancel_width action_width action
    cap_gm_root=""
    command -v grim >/dev/null || fail "gmlook: grim is required for timed native samples"
    cap_gm_fixture nethold list
    key a >/dev/null || fail "gmlook: network key failed"
    cap_gm_wait dialogOpen true
    cap_gm_wait networkFocus Host
    assert_focus
    key 'gmlook.invalid' >/dev/null || fail "gmlook: host typing failed"
    settle
    [[ "$(ipc networkUri)" == smb://gmlook.invalid* ]] || fail "gmlook: inert host did not reach the form"
    cap_gm_network_recipe
    shot gm-net-ready
    for protocol in SMB SFTP FTPS WebDAV NFS; do
        read -r cx cy <<< "$(ipc networkChipCentre "$protocol")"
        width=$(((${#protocol} * cap_gm_advance + 500) / 1000 + cap_gm_button_pad))
        cap_gm_hold "protocol-$protocol" "$protocol" "$cx" "$cy" "$width" networkChipCentre
    done
    action=$(ipc networkAction) || fail "gmlook: action label unavailable"
    [[ "$action" == 'Connect and Save' ]] || fail "gmlook: network action is $action"
    action_width=$(((${#action} * cap_gm_advance + 500) / 1000 + cap_gm_button_pad))
    cancel_width=$(((6 * cap_gm_advance + 500) / 1000 + cap_gm_button_pad))
    width=$((cap_gm_button_pad / 2 - 1))
    cap_gm_hold cancel Cancel "$((cap_gm_right - action_width - width - cancel_width / 2))" "$cap_gm_action_y" "$cancel_width" card-recipe
    cap_gm_hold connect "$action" "$((cap_gm_right - action_width / 2))" "$cap_gm_action_y" "$action_width" card-recipe
    if (( cap_gm_dropbox )); then
        width=$(((15 * cap_gm_advance + 500) / 1000 + cap_gm_button_pad))
        cx=$((cap_gm_card_x + cap_gm_inset + width / 2))
        cy=$((cap_gm_card_y + cap_gm_card_h - cap_gm_inset - cap_gm_button_h / 2))
        cap_gm_hold dropbox 'Install Dropbox' "$cx" "$cy" "$width" card-recipe
    else
        printf 'CAP NOTE Install Dropbox is hidden because dropbox-cli is installed; no pressable button exists.\n'
    fi
}
