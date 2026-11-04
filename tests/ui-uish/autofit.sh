# GPUI size fixture follows the SI promotion at 999950 bytes (src/i18n/adv_c_tests.rs).
case_columnautofit() {
    local dir="$fixture_root/columnautofit"
    sandbox_scratch "$dir"
    mkdir -p "$dir/subdir"
    printf 'alpha\n' > "$dir/alpha.txt"
    : > "$dir/photo.jpg"
    # Sample input: "1.5 GB" is GLib's own render of 1500000000, the first screen's widest size.
    local big_bytes=1500000000
    truncate -s "$big_bytes" "$dir/big.bin"
    local i
    # Past the pane's own window (visible rows plus twice the 150-row buffer, about 336 here), so G really moves the held window.
    for i in $(seq -w 1 400); do printf 'body\n' > "$dir/file-$i.txt"; done
    # 999,949 bytes renders as "999.9 kB", wider than the "1.5 GB" above, and sorts last so it starts off screen.
    # 999,950 would promote to "1.0 MB" under src/i18n/format.rs format_size (613bea3bb), as wide as "1.5 GB".
    truncate -s 999949 "$dir/zzz-wide.bin"
    # Kind alone is widened: every other column keeps its floor, so the set still draws whole.
    seed_ui_state "$fixture_root/columnautofit-state" '{"view":"list","columns":["name","mode","size","date","kind"],"columnWidths":{"kind":200}}'
    launch "$dir"
    wait_listing 405
    local before_mark before_w
    before_mark=$(ipc sortMark)
    IFS='|' read -r _x before_w <<< "$(ipc headerCellRect kind)"
    [[ "$before_w" == "200" ]] || fail "columnautofit: kind did not take its seeded 200, got $before_w"

    local cx cy cell_x cell_w edge_x header_left wx wy ww wh
    read -r cx cy <<< "$(ipc headerCellCentre kind)"
    [[ -n "$cx" && -n "$cy" ]] || fail "columnautofit: the kind header has no centre"
    IFS='|' read -r cell_x cell_w <<< "$(ipc headerCellRect kind)"
    [[ "$cell_x" =~ ^-?[0-9]+$ ]] || fail "columnautofit: the kind header has no left edge, got $cell_x"
    header_left=$(ipc headerLeft)
    [[ "$header_left" =~ ^-?[0-9]+$ ]] || fail "columnautofit: the header has no left edge, got $header_left"
    edge_x=$(( header_left + cell_x ))
    read -r wx wy ww wh < <(window_box) || fail "native window coordinates unavailable"
    omarchy-drive click "$(( wx + edge_x ))" "$(( wy + cy ))" left --double >/dev/null
    settle
    local fitted_w
    IFS='|' read -r _x fitted_w <<< "$(ipc headerCellRect kind)"
    printf 'COLUMNAUTOFIT kind before=%s fitted=%s mark=%s\n' "$before_w" "$fitted_w" "$(ipc sortMark)"
    shot columnautofit-double
    (( fitted_w < before_w && fitted_w >= 48 )) \
        || fail "columnautofit: a double click left kind at $fitted_w, not fitted under $before_w"
    [[ "$(ipc sortMark)" == "$before_mark" ]] \
        || fail "columnautofit: the double click sorted, mark is $(ipc sortMark)"
    local stored=""
    for _attempt in $(seq 1 60); do
        stored=$(jq -er '.columnWidths.kind // empty' "$XDG_STATE_HOME/flea/ui.json" 2>/dev/null || true)
        [[ "$stored" == "$fitted_w" ]] && break
        sleep 0.05
    done
    [[ "$stored" == "$fitted_w" ]] \
        || fail "columnautofit: ui.json remembers $stored, the header draws $fitted_w"
    [[ "$(ipc rowCellOverflow 0)" == "0|0|0|0" ]] \
        || fail "columnautofit: a cell painted past its fitted column, $(ipc rowCellOverflow 0)"

    # The fit follows the held window: fitting size on the first screenful ignores the off-screen wide file, and fitting it again after G widens.
    read -r cx cy <<< "$(ipc headerCellCentre size)"
    [[ -n "$cx" && -n "$cy" ]] || fail "columnautofit: the size header has no centre"
    IFS='|' read -r cell_x cell_w <<< "$(ipc headerCellRect size)"
    [[ "$cell_w" =~ ^[0-9]+$ ]] \
        || fail "columnautofit: the size header answered '$cell_w' before the first fit"
    [[ "$cell_x" =~ ^-?[0-9]+$ ]] \
        || fail "columnautofit: the size header has no left edge before the first fit, got $cell_x"
    local size_before="$cell_w"
    edge_x=$(( header_left + cell_x ))
    omarchy-drive click "$(( wx + edge_x ))" "$(( wy + cy ))" left --double >/dev/null
    settle
    local narrow_w narrow_stored
    # Sample input: "$(ipc headerCellRect size)" prints "400|70".
    IFS='|' read -r _x narrow_w <<< "$(ipc headerCellRect size)"
    printf 'COLUMNAUTOFIT size first=%s\n' "$narrow_w"
    [[ "$narrow_w" =~ ^[0-9]+$ && "$narrow_w" != "$size_before" ]] \
        || fail "columnautofit: the first size fit never ran, width stayed at $narrow_w from $size_before"
    narrow_stored=""
    for _attempt in $(seq 1 60); do
        narrow_stored=$(jq -er '.columnWidths.size // empty' "$XDG_STATE_HOME/flea/ui.json" 2>/dev/null || true)
        [[ "$narrow_stored" == "$narrow_w" ]] && break
        sleep 0.05
    done
    [[ "$narrow_stored" == "$narrow_w" ]] \
        || fail "columnautofit: ui.json remembers size as $narrow_stored, the header draws $narrow_w"
    local held_before held_after
    # Sample input: ipc listingWindowState | jq -r '.held' answers "12".
    held_before=$(ipc listingWindowState | jq -r '.held')
    [[ "$held_before" =~ ^[0-9]+$ ]] \
        || fail "columnautofit: the held window answered '$held_before' before G, not a row"
    key G >/dev/null
    settle
    for _attempt in $(seq 1 60); do
        # Sample input: ipc listingWindowState | jq -r '.held' answers "12".
        held_after=$(ipc listingWindowState | jq -r '.held')
        [[ "$held_after" =~ ^[0-9]+$ && "$held_after" != "$held_before" ]] && break
        sleep 0.05
    done
    [[ "$held_before" =~ ^[0-9]+$ && "$held_after" =~ ^[0-9]+$ && "$held_after" != "$held_before" ]] \
        || fail "columnautofit: the held window never left the first screenful, held is $held_after from $held_before"
    read -r cx cy <<< "$(ipc headerCellCentre size)"
    IFS='|' read -r cell_x cell_w <<< "$(ipc headerCellRect size)"
    [[ "$cell_x" =~ ^-?[0-9]+$ ]] || fail "columnautofit: the size header has no left edge after G, got $cell_x"
    edge_x=$(( header_left + cell_x ))
    omarchy-drive click "$(( wx + edge_x ))" "$(( wy + cy ))" left --double >/dev/null
    settle
    local wide_w
    IFS='|' read -r _x wide_w <<< "$(ipc headerCellRect size)"
    printf 'COLUMNAUTOFIT size second=%s\n' "$wide_w"
    shot columnautofit-scrolled
    (( wide_w > narrow_w )) \
        || fail "columnautofit: fitting size after scrolling to the wide file left $narrow_w at $wide_w, so the fit scanned the directory"
    [[ "$(ipc sortMark)" == "$before_mark" ]] \
        || fail "columnautofit: fitting size sorted, mark is $(ipc sortMark)"

    # F4 must match independently double-click-fitted columns before and after reseeding above their measured widths.
    # Sample input: "$(ipc listAreaRect)" prints "0 100 732 500".
    read -r _f4_ax _f4_ay _f4_aw _f4_ah <<< "$(ipc listAreaRect)"
    [[ "$_f4_aw" =~ ^[0-9]+$ && "$_f4_aw" -ge 600 ]] \
        || fail "columnautofit: the list area answered '$_f4_ax $_f4_ay $_f4_aw $_f4_ah', so the two-phase F4 has no fixture window"
    local _f4key _drawn _have _want _stored _seed_ok _f4j_ok
    for _f4key in mode size date kind; do
        # Sample input: "$(ipc headerCellRect "$_f4key")" prints "602|70".
        IFS='|' read -r _x _drawn <<< "$(ipc headerCellRect "$_f4key")"
        [[ "$_drawn" =~ ^[0-9]+$ && "$_drawn" != "0" ]] \
            || fail "columnautofit: $_f4key draws nothing to fit first, widths $(ipc columnWidths)"
    done
    local _fit_all='{}' _fit_width
    for _f4key in mode size date kind; do
        # Sample input: "$(ipc headerCellRect mode)" prints "602|70".
        IFS='|' read -r cell_x cell_w <<< "$(ipc headerCellRect "$_f4key")"
        # Sample input: "$(ipc headerCellCentre mode)" prints "637 70".
        read -r cx cy <<< "$(ipc headerCellCentre "$_f4key")"
        [[ "$cell_x" =~ ^-?[0-9]+$ && "$cell_w" =~ ^[0-9]+$ && "$cell_w" != 0 && -n "$cx" && -n "$cy" ]] \
            || fail "columnautofit: $_f4key has no independent double-click fit target"
        edge_x=$(( header_left + cell_x ))
        omarchy-drive click "$(( wx + edge_x ))" "$(( wy + cy ))" left --double >/dev/null
        settle
        # Sample input: "$(ipc headerCellRect mode)" prints "602|95".
        IFS='|' read -r _x _fit_width <<< "$(ipc headerCellRect "$_f4key")"
        [[ "$_fit_width" =~ ^[0-9]+$ && "$_fit_width" != 0 ]] \
            || fail "columnautofit: $_f4key has no independent fitted width"
        # Sample input: jq --arg k mode --argjson w 95 '. + {($k): $w}' over '{}' answers '{"mode":95}'.
        _fit_all=$(jq -c --arg k "$_f4key" --argjson w "$_fit_width" '. + {($k): $w}' <<< "$_fit_all")
    done
    [[ "$(ipc sortMark)" == "$before_mark" ]] \
        || fail "columnautofit: an independent double-click fit sorted, mark is $(ipc sortMark)"
    # Sample input: "$(ipc columnWidths)" prints '{"mode":90,"size":120,"date":140,"kind":200}'.
    local _f4b_all _f4w_all
    _f4b_all=$(ipc columnWidths)
    key -k F4 >/dev/null
    settle
    # Sample input: "$(ipc sortMark)" prints "size:asc".
    [[ "$(ipc sortMark)" == "$before_mark" ]] \
        || fail "columnautofit: the first F4 sorted, mark is $(ipc sortMark)"
    # Sample input: "$(ipc columnWidths)" prints '{"mode":95,"size":125,"date":140,"kind":105}' once the first F4 lands.
    _f4w_all=$(ipc columnWidths)
    local _f4w_mode _f4w_size _f4w_date _f4w_kind _f4b_mode _f4b_size _f4b_date _f4b_kind
    # Sample input: jq -r '.mode // empty' over '{"mode":95,"size":125}' answers "95".
    _f4w_mode=$(jq -r '.mode // empty' <<< "$_f4w_all")
    _f4w_size=$(jq -r '.size // empty' <<< "$_f4w_all")
    _f4w_date=$(jq -r '.date // empty' <<< "$_f4w_all")
    _f4w_kind=$(jq -r '.kind // empty' <<< "$_f4w_all")
    _f4b_mode=$(jq -r '.mode // empty' <<< "$_f4b_all")
    _f4b_size=$(jq -r '.size // empty' <<< "$_f4b_all")
    _f4b_date=$(jq -r '.date // empty' <<< "$_f4b_all")
    _f4b_kind=$(jq -r '.kind // empty' <<< "$_f4b_all")
    [[ "$_f4w_mode" =~ ^[0-9]+$ && "$_f4w_size" =~ ^[0-9]+$ && "$_f4w_date" =~ ^[0-9]+$ && "$_f4w_kind" =~ ^[0-9]+$ ]] \
        || fail "columnautofit: the first F4 left mode=$_f4w_mode size=$_f4w_size date=$_f4w_date kind=$_f4w_kind, want a fitted width for every drawn key"
    for _f4key in mode size date kind; do
        # Sample input: jq -r --arg k mode '.[$k] // empty' over '{"mode":95}' answers "95".
        _want=$(jq -r --arg k "$_f4key" '.[$k] // empty' <<< "$_fit_all")
        _have=$(jq -r --arg k "$_f4key" '.[$k] // empty' <<< "$_f4w_all")
        [[ "$_have" == "$_want" ]] \
            || fail "columnautofit: F4 stored $_f4key as $_have, independent double-click fit measured $_want"
    done
    # autofitAll writes only a column whose fit moved, so ui.json is owed only the keys the first F4 changed.
    _f4j_ok=0
    for _attempt in $(seq 1 60); do
        _f4j_ok=1
        # Sample input: jq -er '.columnWidths.size // empty' over ui.json answers "125".
        [[ "$_f4w_mode" == "$_f4b_mode" || "$(jq -er '.columnWidths.mode // empty' "$XDG_STATE_HOME/flea/ui.json" 2>/dev/null || true)" == "$_f4w_mode" ]] || _f4j_ok=0
        [[ "$_f4w_size" == "$_f4b_size" || "$(jq -er '.columnWidths.size // empty' "$XDG_STATE_HOME/flea/ui.json" 2>/dev/null || true)" == "$_f4w_size" ]] || _f4j_ok=0
        [[ "$_f4w_date" == "$_f4b_date" || "$(jq -er '.columnWidths.date // empty' "$XDG_STATE_HOME/flea/ui.json" 2>/dev/null || true)" == "$_f4w_date" ]] || _f4j_ok=0
        [[ "$_f4w_kind" == "$_f4b_kind" || "$(jq -er '.columnWidths.kind // empty' "$XDG_STATE_HOME/flea/ui.json" 2>/dev/null || true)" == "$_f4w_kind" ]] || _f4j_ok=0
        (( _f4j_ok == 1 )) && break
        sleep 0.05
    done
    (( _f4j_ok == 1 )) \
        || fail "columnautofit: the first F4 never reached ui.json, want $_f4w_mode $_f4w_size $_f4w_date $_f4w_kind in $(cat "$XDG_STATE_HOME/flea/ui.json" 2>/dev/null)"
    # Seeds sit one offset above their own fits, so no seed can equal the fit the second F4 must return to.
    local SEED_OFFSET=23
    local _f4s_mode _f4s_size _f4s_date _f4s_kind
    _f4s_mode=$((_f4w_mode + SEED_OFFSET))
    _f4s_size=$((_f4w_size + SEED_OFFSET))
    _f4s_date=$((_f4w_date + SEED_OFFSET))
    _f4s_kind=$((_f4w_kind + SEED_OFFSET))
    # The name's floor, ui/Theme.qml's 20 characters at about 8 px each, so the seeded row must leave it room.
    local _f4_name_min=160
    (( _f4s_mode + _f4s_size + _f4s_date + _f4s_kind + _f4_name_min <= _f4_aw )) \
        || fail "columnautofit: seeded widths $_f4s_mode+$_f4s_size+$_f4s_date+$_f4s_kind plus name minimum $_f4_name_min do not fit list area $_f4_aw"
    local _f4seeds _f4wants
    _f4seeds=$(printf '{"mode":%s,"size":%s,"date":%s,"kind":%s}' "$_f4s_mode" "$_f4s_size" "$_f4s_date" "$_f4s_kind")
    _f4wants=$(printf '{"mode":%s,"size":%s,"date":%s,"kind":%s}' "$_f4w_mode" "$_f4w_size" "$_f4w_date" "$_f4w_kind")
    kill_flea
    "$flea_bin" --ui-state "{\"columnWidths\":$_f4seeds}" >/dev/null \
        || fail "columnautofit: could not seed stored widths to $_f4seeds"
    launch "$dir"
    wait_listing 405
    settle
    # The relaunch opens at the top while W was fitted at the bottom, so G returns to the fitted window first.
    # Sample input: ipc listingWindowState | jq -r '.held' answers "12".
    local _f4_held_before _f4_held_after
    _f4_held_before=$(ipc listingWindowState | jq -r '.held')
    [[ "$_f4_held_before" =~ ^[0-9]+$ ]] \
        || fail "columnautofit: the held window answered '$_f4_held_before' after relaunch, not a row"
    key G >/dev/null
    settle
    for _attempt in $(seq 1 60); do
        # Sample input: ipc listingWindowState | jq -r '.held' answers "12".
        _f4_held_after=$(ipc listingWindowState | jq -r '.held')
        [[ "$_f4_held_after" =~ ^[0-9]+$ && "$_f4_held_after" != "$_f4_held_before" ]] && break
        sleep 0.05
    done
    [[ "$_f4_held_before" =~ ^[0-9]+$ && "$_f4_held_after" =~ ^[0-9]+$ && "$_f4_held_after" != "$_f4_held_before" ]] \
        || fail "columnautofit: the held window never left the first screenful after relaunch, held is $_f4_held_after from $_f4_held_before"
    local drawn_before=""
    _seed_ok=0
    for _attempt in $(seq 1 60); do
        _seed_ok=1
        drawn_before=""
        for _f4key in mode size date kind; do
            # Sample input: "$(ipc headerCellRect "$_f4key")" prints "602|118".
            IFS='|' read -r _x _drawn <<< "$(ipc headerCellRect "$_f4key")"
            # Sample input: jq -r --arg k "size" '.[$k] // empty' over '{"size":148}' answers "148".
            _want=$(jq -r --arg k "$_f4key" '.[$k] // empty' <<< "$_f4seeds")
            [[ "$_drawn" == "$_want" ]] || _seed_ok=0
            [[ "$_drawn" =~ ^[0-9]+$ && "$_drawn" != "0" ]] && drawn_before="$drawn_before $_f4key"
        done
        (( _seed_ok == 1 )) && break
        sleep 0.05
    done
    (( _seed_ok == 1 )) \
        || fail "columnautofit: seeding stored widths to $_f4seeds did not draw, widths $(ipc columnWidths)"
    key -k F4 >/dev/null
    settle
    # Sample input: "$(ipc columnWidths)" prints '{"mode":95,"size":125,"date":140,"kind":105}' once the second F4 lands.
    local widths_once
    widths_once=$(ipc columnWidths)
    printf 'COLUMNAUTOFIT f4=%s\n' "$widths_once"
    shot columnautofit-f4
    local drawn_after=""
    for _f4key in mode size date kind; do
        # Sample input: "$(ipc headerCellRect "$_f4key")" prints "602|70".
        IFS='|' read -r _x _drawn <<< "$(ipc headerCellRect "$_f4key")"
        [[ "$_drawn" =~ ^[0-9]+$ && "$_drawn" != "0" ]] || continue
        drawn_after="$drawn_after $_f4key"
    done
    [[ "$drawn_after" == "$drawn_before" ]] \
        || fail "columnautofit: F4 changed the drawn column set from [$drawn_before ] to [$drawn_after ]"
    for _f4key in mode size date kind; do
        # Sample input: "$(ipc headerCellRect "$_f4key")" prints "602|70".
        IFS='|' read -r _x _drawn <<< "$(ipc headerCellRect "$_f4key")"
        # Sample input: jq -r --arg k "size" '.[$k] // empty' over '{"size":125}' answers "125".
        _want=$(jq -r --arg k "$_f4key" '.[$k] // empty' <<< "$_f4wants")
        [[ "$_drawn" == "$_want" ]] \
            || fail "columnautofit: F4 drew $_f4key at $_drawn, want the fitted $_want"
        _have=$(jq -r --arg k "$_f4key" '.[$k] // empty' <<< "$widths_once")
        [[ "$_have" == "$_want" ]] \
            || fail "columnautofit: F4 stored $_f4key as '$_have', want the fitted $_want"
    done
    [[ "$(ipc sortMark)" == "$before_mark" ]] \
        || fail "columnautofit: the second F4 sorted, mark is $(ipc sortMark)"
    _stored=""
    _f4j_ok=0
    for _attempt in $(seq 1 60); do
        _stored=""
        _f4j_ok=1
        for _f4key in mode size date kind; do
            # Sample input: jq -er --arg k "size" '.columnWidths[$k] // empty' over ui.json answers "125".
            _have=$(jq -er --arg k "$_f4key" '.columnWidths[$k] // empty' "$XDG_STATE_HOME/flea/ui.json" 2>/dev/null || true)
            _want=$(jq -r --arg k "$_f4key" '.[$k] // empty' <<< "$_f4wants")
            [[ "$_have" == "$_want" ]] || _f4j_ok=0
        done
        (( _f4j_ok == 1 )) && break
        sleep 0.05
    done
    (( _f4j_ok == 1 )) \
        || fail "columnautofit: the second F4 never reached ui.json, widths $(ipc columnWidths)"
    # A third F4 on fitted widths must change nothing, so F4 is a fit and never a toggle.
    key -k F4 >/dev/null || fail "columnautofit: the third F4 was never sent"
    settle
    # Sample input: "$(ipc columnWidths)" prints '{"mode":95,"size":125,"date":140,"kind":105}'.
    local widths_twice
    widths_twice=$(ipc columnWidths)
    [[ "$widths_twice" == "$widths_once" ]] \
        || fail "columnautofit: a third F4 moved $widths_once to $widths_twice"

    kill_flea
}
