# GPUI overrides for UISH-08; seeded from the unchanged oracle at 2faf8d099.
case_clickedge() {
    local dir="$fixture_root/clickedge" mode
    local bindir="$fixture_root/clickedge-bin"
    # The band block drives the pointer through tests/ui-marquee.sh, whose helpers read these; a fail must not leave the button down.
    local marquee_checks=0 marquee_button_down=false marquee_ctrl_down=false before_band after_band
    local band_rows=4 end_polls=100 end_poll_s=0.05 band_tail_min=3 band_glide_px=0 band_tail band_end band_end_tol=1
    export YDOTOOL_SOCKET="${YDOTOOL_SOCKET:-$XDG_RUNTIME_DIR/.ydotool_socket}"
    [[ -S "$YDOTOOL_SOCKET" ]] || fail "clickedge: no ydotoold socket at $YDOTOOL_SOCKET"
    trap '( marquee_release ) >/dev/null 2>&1 || true' EXIT
    sandbox_scratch "$dir"
    sandbox_scratch "$bindir"
    local i
    for i in $(seq -w 1 150); do printf 'body\n' > "$dir/f$i.txt"; done
    # Outside the listed folder: the stub appends on every open, and a write inside it re-reads under the clicks.
    local opened="$fixture_root/clickedge-opened.log"
    : > "$opened"
    {
      printf '#!/bin/sh\n'
      printf '[ "$1" = open ] || exec /usr/bin/gio "$@"\n'
      printf 'printf "OPENED %%s\\n" "$2" >> %q\n' "$opened"
    } > "$bindir/$open_handoff"
    chmod +x "$bindir/$open_handoff"

    local saved_path="$PATH"
    export PATH="$bindir:$PATH"
    launch "$dir"
    export PATH="$saved_path"
    wait_listing 150
    clickedge_slow_name() {
        local ax ay aw ah whole_last slow slow_name before_slow after_blank after_slow
        key -k Home >/dev/null || fail "clickedge: key Home was rejected"
        settle
        read -r ax ay aw ah <<< "$(ipc listAreaRect)"
        whole_last=$(last_whole_row "$(ipc total)" "$ay" "$ah")
        (( whole_last >= 1 )) || fail "clickedge: $mode found no whole rows, last $whole_last"
        slow=$((whole_last - 1))
        slow_name=$(ipc visibleRowName "$slow")
        [[ -n "$slow_name" ]] || fail "clickedge: $mode slow row $slow names nothing"
        before_slow=$(ipc viewContentY)
        click_row_name "$slow" left
        settle
        [[ "$(ipc selectedIndices)" == "$slow" ]] || fail "clickedge: $mode row $slow is not the sole selection"
        sleep "$slow_click_gap_s"
        if [[ "$mode" == grid ]]; then
            # v0.3.12 GridArea.qml:403-412 limits a tile to its art and painted caption, so its non-name click belongs in the art slot.
            local tx ty tw th nx ny nw nh cx cy wx wy ww wh
            # Sample input: rowThumbRect prints "412 237 64 64" (x y width height).
            read -r tx ty tw th <<< "$(ipc rowThumbRect "$slow")"
            # Sample input: rowNameRect prints "400 310 90 40" (x y width height).
            read -r nx ny nw nh <<< "$(ipc rowNameRect "$slow")"
            [[ "$tx $ty $tw $th $nx $ny $nw $nh" =~ ^-?[0-9]+(\ -?[0-9]+){7}$ ]] \
                || fail "clickedge: grid row $slow has no art and name rectangles"
            (( tw > 0 && th > 0 )) || fail "clickedge: grid row $slow has an empty art slot"
            cx=$((tx + tw / 2))
            cy=$((ty + th / 2))
            (( cx < nx || cx >= nx + nw || cy < ny || cy >= ny + nh )) \
                || fail "clickedge: grid row $slow art centre lies inside its name"
            # Sample input: window_box prints "40 80 1000 720" (x y width height).
            read -r wx wy ww wh < <(window_box) || fail "native window coordinates unavailable"
            omarchy-drive click "$((wx + cx))" "$((wy + cy))" left >/dev/null \
                || fail "clickedge: grid row $slow art click was rejected"
        else
            click_row_beside_name "$slow" left
        fi
        settle
        sleep "$slow_click_gap_s"
        after_blank=$(ipc viewContentY)
        printf 'CLICKEDGE %s non-name row=%s before=%s after=%s live=%s\n' "$mode" "$slow" "$before_slow" "$after_blank" "$(ipc renameEditorLive)"
        [[ "$(ipc renameEditorLive)" == "false" ]] || fail "clickedge: $mode a slow non-name click opened rename on row $slow"
        [[ "$(ipc renamingIndex)" == "-1" ]] || fail "clickedge: $mode a slow non-name click armed rename on row $slow"
        [[ "$(ipc selectedIndices)" == "$slow" ]] || fail "clickedge: $mode the non-name click changed the sole selection"
        [[ "$after_blank" == "$before_slow" ]] || fail "clickedge: $mode the non-name click scrolled $before_slow to $after_blank"
        click_row_name "$slow" left
        settle
        sleep "$slow_click_gap_s"
        click_row_name "$slow" left
        for _attempt in $(seq 1 100); do
            [[ "$(ipc renameEditorLive)" == "true" ]] && break
            sleep 0.05
        done
        after_slow=$(ipc viewContentY)
        printf 'CLICKEDGE %s rename row=%s before=%s after=%s live=%s\n' "$mode" "$slow" "$before_slow" "$after_slow" "$(ipc renameEditorLive)"
        [[ "$(ipc renameEditorLive)" == "true" ]] || fail "clickedge: $mode the slow click never opened rename on row $slow"
        [[ "$(ipc renamingIndex)" == "$slow" ]] || fail "clickedge: $mode the slow click renamed another row"
        [[ "$after_slow" == "$before_slow" ]] || fail "clickedge: $mode the slow click scrolled $before_slow to $after_slow"
    }
    for mode in list columns; do
        if [[ "$mode" != list ]]; then
            click_chrome "$mode"
            settle
            [[ "$(ipc viewMode)" == "$mode" ]] || fail "clickedge: the chrome drew '$(ipc viewMode)', not $mode"
        fi
        key -k Home >/dev/null || fail "clickedge: key Home was rejected"
        settle
        [[ "$(ipc viewContentY)" == "0" ]] || fail "clickedge: $mode did not start at the top, contentY $(ipc viewContentY)"
        local visible target last before after_first after_double target_name want
        visible=$(ipc visibleRows)
        [[ "$visible" =~ ^[1-9][0-9]*$ ]] || fail "clickedge: $mode has no visible row count, got [$visible]"
        # visibleRows counts the cut bottom row too, so last is the last whole row, never the cut one.
        local ex ey ew eh
        read -r ex ey ew eh <<< "$(ipc listAreaRect)"
        [[ "$ex $ey $ew $eh" =~ ^[0-9]+\ [0-9]+\ [0-9]+\ [0-9]+$ ]] || fail "clickedge: $mode has no listing area, got [$ex $ey $ew $eh]"
        last=$(last_whole_row "$visible" "$ey" "$eh")
        (( last >= 1 )) || fail "clickedge: $mode found no whole rows, last $last"
        target=$((last - 1))
        # A cut row's centre can lie outside the list, so click inside its drawn part.
        click_drawn() {
            local index="$1"; shift
            local qrx qry qrw qrh qax qay qaw qah qcx qwx qwy qww qwh bottom y
            read -r qrx qry qrw qrh <<< "$(ipc rowRect "$index")"
            [[ -n "$qrx" ]] || fail "clickedge: $mode row $index has no rect"
            read -r qax qay qaw qah <<< "$(ipc listAreaRect)"
            bottom=$((qay + qah))
            y=$(((qry + bottom) / 2))
            (( y > qry && y < bottom )) || fail "clickedge: $mode row $index has no drawn part, top $qry bottom $bottom"
            qcx=$((qrx + qrw / 2))
            read -r qwx qwy qww qwh < <(window_box) || fail "clickedge: native window coordinates unavailable"
            omarchy-drive click "$((qcx + qwx))" "$((y + qwy))" "$@" >/dev/null
        }
        [[ -n "$(ipc rowCentre "$target")" ]] || fail "clickedge: $mode row $target has no centre, visible $visible"
        [[ -n "$(ipc rowCentre "$last")" ]] || fail "clickedge: $mode row $last has no centre, visible $visible"
        target_name=$(ipc visibleRowName "$target")
        [[ -n "$target_name" ]] || fail "clickedge: $mode row $target names nothing"
        before=$(ipc viewContentY)
        click_row "$target" left
        settle
        after_first=$(ipc viewContentY)
        printf 'CLICKEDGE %s first before=%s after=%s cursor=%s\n' "$mode" "$before" "$after_first" "$(ipc cursor)"
        [[ "$(ipc cursor)" == "$target" ]] || fail "clickedge: $mode a left click did not move the cursor to $target, it is $(ipc cursor)"
        [[ "$after_first" == "$before" ]] || fail "clickedge: $mode the first click scrolled $before to $after_first"
        : > "$opened"
        click_row "$target" left --double
        for _attempt in $(seq 1 100); do
            grep -q "^OPENED $dir/$target_name$" "$opened" && break
            sleep 0.05
        done
        after_double=$(ipc viewContentY)
        printf 'CLICKEDGE %s double opened=%q before=%s after=%s\n' "$mode" "$(cat "$opened")" "$before" "$after_double"
        grep -q "^OPENED $dir/$target_name$" "$opened" || fail "clickedge: $mode a double click did not open $target_name, log $(cat "$opened")"
        [[ "$after_double" == "$before" ]] || fail "clickedge: $mode the double click scrolled $before to $after_double"
        local before_menu after_menu
        before_menu=$(ipc viewContentY)
        click_row "$last" right
        settle
        after_menu=$(ipc viewContentY)
        printf 'CLICKEDGE %s menu before=%s after=%s visible=%s cursor=%s\n' "$mode" "$before_menu" "$after_menu" "$(ipc contextMenuVisible)" "$(ipc cursor)"
        [[ "$(ipc contextMenuVisible)" == "true" ]] || fail "clickedge: $mode a right click opened no menu"
        [[ "$after_menu" == "$before_menu" ]] || fail "clickedge: $mode a right click scrolled $before_menu to $after_menu"
        key -k Escape >/dev/null || fail "clickedge: key Escape was rejected"
        settle
        [[ "$(ipc contextMenuVisible)" == "false" ]] || fail "clickedge: $mode Escape left the menu open"
        # A click on the cut row scrolls by exactly the cut pixels, so the double click's second tap still lands on it.
        key -k Home >/dev/null || fail "clickedge: key Home was rejected"
        settle
        [[ "$(ipc viewContentY)" == "0" ]] || fail "clickedge: $mode Home did not return to the top, contentY $(ipc viewContentY)"
        local ax ay aw ah
        read -r ax ay aw ah <<< "$(ipc listAreaRect)"
        [[ "$ax $ay $aw $ah" =~ ^[0-9]+\ [0-9]+\ [0-9]+\ [0-9]+$ ]] || fail "clickedge: $mode has no listing area, got [$ax $ay $aw $ah]"
        local cut=-1 crx cry crw crh ci
        for (( ci = 0; ci < visible + 2; ci++ )); do
            [[ -n "$(ipc rowRect "$ci")" ]] || break
            read -r crx cry crw crh <<< "$(ipc rowRect "$ci")"
            if (( cry + crh > ay + ah && cry < ay + ah )); then cut=$ci; break; fi
        done
        if (( cut < 0 )); then
            printf 'CLICKEDGE %s cut skipped, every row is whole\n' "$mode"
        else
            local cut_amount=$((cry + crh - ay - ah)) cut_name before_cut after_cut drift
            cut_name=$(ipc visibleRowName "$cut")
            [[ -n "$cut_name" ]] || fail "clickedge: $mode cut row $cut names nothing"
            before_cut=$(ipc viewContentY)
            click_drawn "$cut" left
            settle
            after_cut=$(ipc viewContentY)
            printf 'CLICKEDGE %s cut click row=%s before=%s after=%s want=%s\n' "$mode" "$cut" "$before_cut" "$after_cut" "$((before_cut + cut_amount))"
            [[ "$(ipc cursor)" == "$cut" ]] || fail "clickedge: $mode a click on the cut row did not move the cursor to $cut"
            drift=$((after_cut - before_cut - cut_amount))
            (( drift >= -1 && drift <= 1 )) || fail "clickedge: $mode the cut click scrolled $before_cut to $after_cut, want $((before_cut + cut_amount))"
            : > "$opened"
            click_drawn "$cut" left --double
            for _attempt in $(seq 1 100); do
                grep -q "^OPENED $dir/$cut_name$" "$opened" && break
                sleep 0.05
            done
            printf 'CLICKEDGE %s cut double opened=%q contentY=%s\n' "$mode" "$(cat "$opened")" "$(ipc viewContentY)"
            grep -q "^OPENED $dir/$cut_name$" "$opened" || fail "clickedge: $mode a double click on the cut row did not open $cut_name, log $(cat "$opened")"
        fi
        # A slow click renames only on the name, and neither target scrolls.
        clickedge_slow_name
        # An in-place error expands the editor, and the reveal carries context 0 so it stays under the pointer.
        key -M ctrl -k a -m ctrl -k BackSpace >/dev/null || fail "clickedge: $mode clearing the rename draft failed"
        key "bad/name" >/dev/null || fail "clickedge: $mode typing the slash name failed"
        local before_error after_error rename_err
        before_error=$(ipc viewContentY)
        key -k Return >/dev/null || fail "clickedge: $mode submitting the slash name failed"
        settle
        rename_err=$(ipc renameState | jq -er .error)
        [[ "$rename_err" == *"A name cannot"* ]] || fail "clickedge: $mode the slash name did not raise the in-place error, got $rename_err"
        after_error=$(ipc viewContentY)
        printf 'CLICKEDGE %s rename-error before=%s after=%s err=%q\n' "$mode" "$before_error" "$after_error" "$rename_err"
        [[ "$after_error" == "$before_error" ]] || fail "clickedge: $mode the in-place error scrolled $before_error to $after_error"
        key -k Escape >/dev/null || fail "clickedge: key Escape was rejected"
        settle
        [[ "$(ipc renameEditorLive)" == "false" ]] || fail "clickedge: $mode Escape left the rename open"
        printf 'CLICKEDGE %s rename cancelled row=%s contentY=%s\n' "$mode" "$(ipc cursor)" "$(ipc viewContentY)"
        # A band release never scrolls: the band starts on the bare ground below the last row, where marquee_begin_below checks the pointer landed.
        key -k End >/dev/null || fail "clickedge: key End was rejected"
        local band_last band_top band_want band_prev=-1 band_now band_settled=false band_held band_cx band_cy
        local brx bry brw brh bay
        band_last=$(( $(ipc total) - 1 ))
        band_top=$((band_last - band_rows + 1))
        (( band_top >= 1 )) || fail "clickedge: $mode holds too few rows for a $band_rows-row band, last $band_last"
        for _attempt in $(seq 1 "$end_polls"); do
            band_now=$(ipc viewContentY)
            if [[ -n "$(ipc rowRect "$band_last")" && "$band_now" != 0 && "$band_now" == "$band_prev" ]]; then band_settled=true; break; fi
            band_prev=$band_now
            sleep "$end_poll_s"
        done
        [[ "$band_settled" == true ]] || fail "clickedge: $mode End never settled on row $band_last, contentY $band_now"
        # End parks the last row flush; one wheel notch over it goes on to the true end of travel, where the footer is the bare ground the band starts from.
        local band_flush=$band_now band_wx band_wy band_wcx band_wcy
        read -r band_wx band_wy _ _ < <(window_box) || fail "clickedge: $mode owned window is unavailable"
        read -r band_wcx band_wcy <<< "$(ipc rowCentre "$band_last")"
        [[ "$band_wcx $band_wcy" =~ ^[0-9]+\ [0-9]+$ ]] || fail "clickedge: $mode row $band_last has no centre for the wheel, got [$band_wcx $band_wcy]"
        omarchy-drive move "$((band_wx + band_wcx))" "$((band_wy + band_wcy))" >/dev/null || fail "clickedge: $mode the pointer could not move onto row $band_last"
        omarchy-drive scroll down 1 >/dev/null || fail "clickedge: $mode the wheel notch was rejected"
        band_settled=false
        for _attempt in $(seq 1 "$end_polls"); do
            band_now=$(ipc viewContentY)
            if [[ "$band_now" =~ ^[0-9]+$ ]] && (( band_now > band_flush )) && [[ "$band_now" == "$band_prev" ]]; then band_settled=true; break; fi
            band_prev=$band_now
            sleep "$end_poll_s"
        done
        [[ "$band_settled" == true ]] || fail "clickedge: $mode the wheel never settled past the flush end $band_flush, contentY $band_now"
        # The notch must reach the true end of travel, not stop part way into the footer or clamp short of it.
        band_end=$(ipc viewEndY)
        [[ "$band_end" =~ ^-?[0-9]+$ ]] || fail "clickedge: $mode the view's end of travel is unreadable, got [$band_end]"
        (( band_now - band_end <= band_end_tol && band_end - band_now <= band_end_tol )) || fail "clickedge: $mode the wheel settled at contentY $band_now, not the end of travel $band_end"
        read -r ax ay aw ah <<< "$(ipc listAreaRect)"
        read -r brx bry brw brh <<< "$(ipc rowRect "$band_last")"
        [[ "$ax $ay $aw $ah $brx $bry $brw $brh" =~ ^[0-9]+(\ [0-9]+){7}$ ]] || fail "clickedge: $mode band geometry unavailable"
        band_tail=$((ay + ah - (bry + brh)))
        (( brh > 0 && band_tail >= band_tail_min )) || fail "clickedge: $mode the tail below row $band_last is $band_tail px, under the $band_tail_min an exact glide needs"
        read -r _ bry _ _ <<< "$(ipc rowRect "$band_top")"
        [[ "$bry" =~ ^[0-9]+$ ]] && (( bry >= ay )) || fail "clickedge: $mode band top row $band_top is not drawn whole, top [$bry] view top $ay"
        read -r band_cx band_cy <<< "$(ipc rowCentre "$band_top")"
        [[ "$band_cx $band_cy" =~ ^[0-9]+\ [0-9]+$ ]] || fail "clickedge: $mode row $band_top has no centre, got [$band_cx $band_cy]"
        band_want=$(seq -s, "$band_top" "$band_last")
        before_band=$(ipc viewContentY)
        marquee_begin_below "$band_last" false false "$band_glide_px"
        marquee_to "$band_cx" "$band_cy"
        marquee_state '.active and .tracking' "clickedge $mode has a live rubber band"
        marquee_expect selectedIndices "$band_want" "clickedge $mode band marks rows $band_top to $band_last while held"
        band_held=$(ipc viewContentY)
        [[ "$band_held" == "$before_band" ]] || fail "clickedge: $mode the held band scrolled $before_band to $band_held"
        marquee_release
        settle
        marquee_expect selectedIndices "$band_want" "clickedge $mode release keeps the banded rows"
        marquee_expect cursor "$band_top" "clickedge $mode release moves the cursor to the row the band ended on"
        after_band=$(ipc viewContentY)
        printf 'CLICKEDGE %s band from=%s to=%s tail=%s before=%s after=%s selected=%s\n' "$mode" "$band_last" "$band_top" "$band_tail" "$before_band" "$after_band" "$(ipc selectedIndices)"
        [[ "$after_band" == "$before_band" ]] || fail "clickedge: $mode the band release scrolled $before_band to $after_band"
        # The next mode and the grid part assume the top of the listing with nothing marked and the cursor on row 0.
        key -k Escape >/dev/null || fail "clickedge: key Escape was rejected"
        key -k Home >/dev/null || fail "clickedge: key Home was rejected"
        marquee_expect selectionCount 0 "clickedge $mode band marks are cleared for the next step"
        marquee_expect cursor 0 "clickedge $mode cursor is back on row 0 for the next step"
        marquee_expect viewContentY 0 "clickedge $mode view is back at the top for the next step"
        printf 'CLICKEDGE %s ok target=%s last=%s\n' "$mode" "$target" "$last"
    done
    mode=grid
    click_chrome "$mode"
    settle
    [[ "$(ipc viewMode)" == "$mode" ]] || fail "clickedge: the chrome did not reach $mode"
    clickedge_slow_name
    key -k Escape >/dev/null || fail "clickedge: key Escape was rejected"
    settle
    [[ "$(ipc renameEditorLive)" == "false" ]] || fail "clickedge: $mode Escape left the rename open"
    kill_flea
}

case_renamefirst() {
    local dir="$fixture_root/renamefirst"
    sandbox_scratch "$dir"
    : > "$dir/a-first.txt"
    local bindir="$fixture_root/renamefirst-bin"
    sandbox_scratch "$bindir"
    cat > "$bindir/gio" <<EOS
#!/bin/sh
# The catalogue legs block until the release exists; every other leg answers at once.
if [ "\$1" = info ] || [ "\$1" = mime ]; then
  while [ ! -e "$dir/release" ]; do sleep 0.05; done
fi
if [ "\$1 \$2" = "mount -li" ]; then exit 0; fi
exit 0
EOS
    chmod +x "$bindir/gio"

    local fixture_home="$fixture_root/renamefirst-home"
    fixture_home_make "$fixture_home"
    local state="$fixture_root/renamefirst-state"
    seed_ui_state "$state" '{"view":"list","keys":"default"}'
    local real_home="$HOME" saved_path="$PATH"
    export PATH="$bindir:$PATH"
    export HOME="$fixture_home"
    launch "$dir"
    export HOME="$real_home"
    export PATH="$saved_path"
    wait_listing 1

    click_row 0 right
    settle
    [[ "$(ipc contextMenuVisible)" == "true" ]] || fail "renamefirst: right click opened no menu"
    # The snapshot answers at once; only the catalogue behind it blocks.
    local attempt
    for attempt in $(seq 1 100); do
        [[ "$(ipc menuState | jq -r '.snapshotReady')" == "true" ]] && break
        sleep 0.05
    done
    [[ "$(ipc menuState | jq -r '.snapshotReady')" == "true" ]] \
        || fail "renamefirst: the menu never snapshotted"
    local rename_index rx ry wx wy ww wh
    rename_index=$(menu_row_index "Rename") || fail "renamefirst: the open menu has no Rename row"
    read -r rx ry <<< "$(ipc contextMenuRowCentre "$rename_index")"
    [[ -n "$ry" ]] || fail "renamefirst: the Rename row has no on-screen centre"
    read -r wx wy ww wh < <(window_box) || fail "native window coordinates unavailable"
    omarchy-drive click "$((wx + rx))" "$((wy + ry))" left >/dev/null
    # The editor opens at once, ahead of the blocked catalogue.
    local opened=""
    for attempt in $(seq 1 100); do
        if [[ "$(ipc renamingIndex)" == "0" && "$(ipc renameEditorLive)" == "true" ]]; then opened="yes"; break; fi
        sleep 0.05
    done
    [[ "$opened" == "yes" ]] \
        || fail "renamefirst: Rename did not open the editor, renamingIndex is $(ipc renamingIndex)"
    shot renamefirst-editing

    # An outside create lands while the field is open; the held watch keeps the draft.
    : > "$dir/x"
    key "a-renamed" >/dev/null
    key -k Return >/dev/null
    settle
    [[ -e "$dir/a-renamed.txt" ]] || fail "renamefirst: the commit did not land first try"
    [[ ! -e "$dir/a-first.txt" ]] || fail "renamefirst: the old name is still on disk"
    : > "$dir/release"
    shot renamefirst-renamed
}

case_renamelife() { case_renamedesign life; }

case_renamefar() { case_renamedesign far; }
