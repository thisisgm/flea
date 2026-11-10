# GPUI overrides for UISH-17; seeded from the unchanged oracle at 2faf8d099.
case_pdffocus() {
    local dir="$fixture_root/pdffocus" state="$fixture_root/pdffocus-state" preset mode last_row selected_row cx cy wx wy ww wh addr
    sandbox_scratch "$dir"
    sandbox_scratch "$state"
    mkdir -p "$state/flea"
    printf 'listing cursor guard\n' > "$dir/a.txt"
    magick \( -size 440x1100 xc:white -fill black "${flea_magick_font_args[@]}" -pointsize 24 -annotate +30+45 PAGEONE \) \
           \( -size 440x1100 xc:white -fill black "${flea_magick_font_args[@]}" -pointsize 24 -annotate +30+45 PAGETWO \) \
           \( -size 440x1100 xc:white -fill black "${flea_magick_font_args[@]}" -pointsize 24 -annotate +30+45 PAGETHREE \) "$dir/manual.pdf"
    [[ -s "$dir/manual.pdf" ]] || fail "PDF fixture generation failed"
    export XDG_STATE_HOME="$state"
    for preset in default vim mac windows; do
        jq -n --arg preset "$preset" '{view:"list",keys:$preset,preview:{column:true,loadOn:"automatic"}}' > "$state/flea/ui.json"
        launch "$dir"
        wait_listing 2
        [[ "$(ipc keymapPreset)" == "$preset" ]] || fail "PDF preset did not load: $preset"
        open_row manual.pdf
        pdf_controls
        # Close by Space, then Enter, then Escape and Ctrl+Tab in separate real activations.
        key -k space >/dev/null
        [[ "$(ipc previewOpen)" == false ]] || fail "PDF Space did not activate Close"
        open_row manual.pdf
        # Shift+Tab from the open, with no control picked, enters the strip at Close and draws its line there alone.
        pdf_expect true '.pages == 3 and .focused and .control == -1' "reopens with no control picked"
        key -M shift -k Tab -m shift >/dev/null
        pdf_expect true ".control == 5 and $pdf_framed == [5]" "Shift+Tab enters at Close"
        key -k Return >/dev/null
        [[ "$(ipc previewOpen)" == false ]] || fail "PDF Enter did not activate Close"
        open_row manual.pdf
        key -k Escape >/dev/null
        [[ "$(ipc previewOpen)" == false ]] || fail "PDF Escape did not close Quick Look"
        open_row manual.pdf
        key -M ctrl -k Tab -m ctrl >/dev/null
        [[ "$(ipc previewOpen)" == false ]] || fail "PDF Ctrl+Tab did not return to list"
        [[ "$(ipc pdfState false)" == null ]] || fail "PDF List unexpectedly has an inline preview"
        click_chrome columns
        settle
        pdf_expect false '.pages == 3' "inline document loaded"
        key -M ctrl -k Tab -m ctrl >/dev/null
        pdf_expect false '.focused and .page == 0' "Ctrl+Tab enters inline PDF on the first page"
        key l >/dev/null
        pdf_expect false '.page == 1 and .scrollY > 0' "l scrolls the next page to the frame's top"
        key h >/dev/null
        pdf_expect false '.page == 0 and .scrollY == 0' "h scrolls back to the first page"
        key -k Down >/dev/null
        pdf_expect false '.scrollY > 0' "Down scrolls the pages"
        key -k Up >/dev/null
        pdf_expect false '.scrollY == 0' "Up scrolls the pages back"
        key -k Right >/dev/null
        pdf_expect false '.page == 1' "Right scrolls the next page to the frame's top"
        key e >/dev/null
        pdf_expect true '.focused and .page == 1 and .zoom == 1' "inline expansion keeps the page"
        [[ "$(ipc previewExpanded)" == true ]] || fail "PDF inline expand did not fill the window"
        key e >/dev/null
        pdf_expect true '.page == 1 and .zoom == 1' "collapse retains the page"
        [[ "$(ipc previewExpanded)" == false ]] || fail "PDF expanded view did not collapse"
        key -k Escape >/dev/null
        pdf_expect false '(.focused | not)' "expanded Escape returns focus"
        key -M ctrl -k Tab -m ctrl >/dev/null
        pdf_expect false '.focused' "Ctrl+Tab enters inline PDF"
        key -M ctrl -k Tab -m ctrl >/dev/null
        pdf_expect false '(.focused | not)' "Ctrl+Tab exits inline PDF"
        printf 'PDF preset=%s overlay=ok inline=ok focus=ok activation=ok disabled=ok pages=ok zoom=ok scroll=ok isolation=ok\n' "$preset"
        kill_flea
    done
    # All three listing views use the same overlay; prove their native entry paths separately.
    for mode in list grid columns; do
        jq -n --arg mode "$mode" '{view:$mode,keys:"default",preview:{column:true,loadOn:"automatic"}}' > "$state/flea/ui.json"
        launch "$dir"
        wait_listing 2
        if [[ "$mode" == grid ]]; then
            [[ "$(ipc viewMode)" == grid ]] || fail "PDF grid entry started in $(ipc viewMode), not grid"
            last_row=$(( $(ipc total) - 1 ))
            # The hidden list has model 0 in grid, so rowAt reads loading there; visibleRowName reads the shown view.
            [[ "$last_row" -ge 1 && "$(ipc visibleRowName "$last_row")" == manual.pdf ]] \
                || fail "PDF grid fixture order put $(ipc visibleRowName "$last_row") last, not manual.pdf"
            key -k End >/dev/null
            settle
            selected_row=$(ipc visibleRowName "$(ipc cursor)")
            [[ "$(ipc cursor)" == "$last_row" && "$selected_row" == manual.pdf ]] \
                || fail "PDF grid End selected row $(ipc cursor): $selected_row, not manual.pdf at $last_row"
            key -k space >/dev/null
            settle
        else
            open_row manual.pdf
        fi
        pdf_expect true '.focused and .pages == 3' "$mode Quick Look entry"
        key e >/dev/null
        [[ "$(ipc previewExpanded)" == true ]] || fail "PDF e did not expand in $mode"
        # Reuse cardsizes.sh's addressed Hyprland resize, with exact owned PID and geometry checks.
        addr=$(hyprctl -j clients | jq -er --argjson pid "$(flea_pid)" '.[] | select(.pid == $pid) | .address')
        [[ "$addr" =~ ^0x[0-9a-fA-F]+$ ]] || fail "PDF cannot identify owned window"
        omarchy-drive window float flea >/dev/null
        local resize_width=800 resize_height=480
        hypr_window_resize "$addr" "$resize_width" "$resize_height" || fail "PDF could not resize owned window"
        omarchy-drive window center flea >/dev/null
        settle
        read -r wx wy ww wh < <(window_box) || fail "native window coordinates unavailable"
        [[ "$ww $wh" == '800 480' ]] || fail "PDF specimen viewport is $ww $wh"
        shot "pdf-$mode-800x480"
        read -r cx cy <<< "$(ipc pdfState true | jq -r '.controls[1].centre')"
        [[ "$cx" =~ ^[0-9]+$ && "$cy" =~ ^[0-9]+$ ]] || fail "PDF missing native Next centre"
        # Hover and a press on Next read back off the mark; the pointer never draws the keyboard frame, and a press released off the mark turns no page.
        pdf_pointer_on "$((wx + cx))" "$((wy + cy))"
        pdf_expect true ".controls[1].hovered and (.controls[1].pressed | not) and $pdf_framed == []" "$mode pointer hovers Next"
        YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool click 0x40 >/dev/null 2>&1 || fail "PDF pointer press on Next failed"
        pdf_expect true ".controls[1].pressed and .page == 0 and $pdf_framed == []" "$mode pointer presses Next"
        YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool mousemove -x 0 -y 140 >/dev/null 2>&1 || fail "PDF pointer move off Next failed"
        YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool click 0x80 >/dev/null 2>&1 || fail "PDF pointer release failed"
        pdf_expect true '(.controls[1].pressed | not) and .page == 0 and .control == -1' "$mode press released off Next turns no page"
        omarchy-drive click "$((wx + cx))" "$((wy + cy))" left >/dev/null
        pdf_expect true '.page == 1' "$mode pointer Next"
        # Expanded, the toolbar lies on the chrome strip: at 800 px Next sits on a crumb and zoom in on a view button.
        [[ "$(ipc path)" == "$dir" && "$(ipc viewMode)" == "$mode" ]] \
            || fail "PDF $mode pointer Next reached the chrome beneath: path $(ipc path), view $(ipc viewMode)"
        # Sample input: pdfState .controls[] entries like {"name":"Zoom in","enabled":true,"visible":true,"centre":"412 38","hovered":false,"pressed":false,"ring":false}.
        read -r cx cy <<< "$(ipc pdfState true | jq -r '.controls[] | select(.name == "Zoom in") | .centre')"
        [[ "$cx" =~ ^[0-9]+$ && "$cy" =~ ^[0-9]+$ ]] || fail "PDF missing native Zoom in centre"
        omarchy-drive click "$((wx + cx))" "$((wy + cy))" left >/dev/null
        pdf_expect true '.zoom == 1.25' "$mode pointer zoom in"
        [[ "$(ipc path)" == "$dir" && "$(ipc viewMode)" == "$mode" ]] \
            || fail "PDF $mode pointer zoom in reached the chrome beneath: path $(ipc path), view $(ipc viewMode)"
        key -k Escape >/dev/null
        pdf_overlay_gone "$mode"
        if [[ "$mode" != columns ]]; then
            [[ "$(ipc pdfState false)" == null ]] || fail "PDF $mode unexpectedly has an inline preview"
            shot "pdf-listing-$mode-800x480"
            printf 'PDF view=%s native_entry=ok pointer=ok no_inline=ok viewport=800x480\n' "$mode"
            kill_flea
            continue
        fi
        pdf_expect false '.pages == 3' "$mode inline document"
        key -M ctrl -k Tab -m ctrl >/dev/null
        pdf_expect false '.focused' "$mode inline focus entry"
        pdf_expect false '(.frame | split(" ") | map(tonumber)) as $frame | $frame[2] > 0 and $frame[3] > 0' "$mode inline frame is drawn"
        shot "pdf-inline-$mode-800x480"
        key -k Escape >/dev/null
        pdf_expect false '(.focused | not)' "$mode inline focus return"
        printf 'PDF view=%s native_entry=ok pointer=ok viewport=%sx%s\n' "$mode" "$ww" "$wh"
        kill_flea
    done
}

case_columns() {
    local dir="$fixture_root/columns"
    sandbox_scratch "$dir"
    mkdir -p "$dir/inner"
    printf 'body\n' > "$dir/inner/deep.txt"
    # One row per preview state this case asserts, made by real tools so the facts are real facts.
    magick -size 640x480 xc:navy "$dir/shot.png"
    printf 'one\ntwo\nthree\n' > "$dir/notes.txt"
    ln -s inner "$dir/link"
    # Unsupported needs binary content; an unfamiliar suffix on readable text still previews as text.
    printf '\000\001\002\003\377\376\375\374' > "$dir/core.dump"
    # An archive too, because the archive tile is the one preview state with no live coverage at all.
    mkdir -p "$dir/inner/pack"
    printf 'packed\n' > "$dir/inner/pack/a.txt"
    printf 'packed\n' > "$dir/inner/pack/b.txt"
    bsdtar -a -c -f "$dir/backup.tar.zst" -C "$dir/inner/pack" . 2>/dev/null
    sandbox_require "$dir/inner/pack"
    sandbox_under "$SANDBOX_PATH" "$dir" || fail "columns: archive fixture escaped its own sandbox"
    rm -rf -- "$SANDBOX_PATH"
    # The pdf logic is covered in tests/js/facts.js, but PdfDocument, the page render, the page count
    # and the error state had never met a real file anywhere in this suite. Each page is built in its
    # own parentheses: without them magick applies one -draw to the whole list and both pages come out
    # identical, measured on the built file before this was written.
    magick \( -size 400x1200 xc:white -fill black -draw "rectangle 40,40 120,80" \) \
           \( -size 400x1200 xc:white -fill black -draw "rectangle 40,40 360,520" \) \
           "$dir/manual.pdf"
    [[ -s "$dir/manual.pdf" ]] || fail "magick produced no manual.pdf"
    # A real header over a truncated body, so the document is unreadable rather than absent; poppler
    # calls the same 200 bytes "Couldn't read xref table".
    head -c 200 "$dir/manual.pdf" > "$dir/broken.pdf"

    launch "$dir"
    wait_listing 8
    click_chrome columns
    settle
    [[ "$(ipc viewMode)" == "columns" ]] || fail "columns: the chrome button did not switch the view"

    # Like the grid, a column view has no columns to head, so the strip collapses.
    (( $(ipc headerTop) == $(ipc chromeHeight) )) || fail "columns: the column header did not collapse"

    # Row 0 is the directory "inner", so the third pane is its contents and not a preview.
    # The hidden list has model 0 in columns, so rowAt reads loading there; visibleRowName reads the shown view.
    [[ "$(ipc visibleRowName 0)" == "inner" ]] || fail "columns: row 0 is $(ipc visibleRowName 0), not the directory"

    # The archive tile: an exact count, an unpacked total, and the entries the frame could name.
    seek_row_named "backup.tar.zst"
    settle
    [[ "$(ipc previewColumnState)" == "archive" ]] \
        || fail "columns: an archive previews as $(ipc previewColumnState)"
    archive_facts=$(ipc previewFacts)
    # Preview board rule 1: Size above says what it weighs packed, so Entries carries both counts.
    [[ "$archive_facts" == *"Entries=3, "*" out"* ]] \
        || fail "columns: the archive states $archive_facts, not the three members it holds"
    [[ "$archive_facts" == *"Kind="*"Size="*"Modified="*"Entries="* ]] \
        || fail "columns: the archive tile's labels are wrong, got $archive_facts"

    seek_row_named "shot.png"
    settle
    [[ "$(ipc previewColumnState)" == "image" ]] \
        || fail "columns: a png previews as $(ipc previewColumnState)"
    # The canvas's Image tile, label for label and in its own order.
    # The label sequence is the canvas's contract; the values are checked one at a time, because a
    # glob pattern carrying the multiplication sign does not match under this suite's own locale.
    local facts
    facts=$(ipc previewFacts)
    [[ "$(fact_labels "$facts")" == "Kind|Size|Modified|Pixels" ]] \
        || fail "columns: the image states $(fact_labels "$facts"), not the canvas's own four rows"
    [[ "$facts" == "Kind=PNG image|"* ]] || fail "columns: the image kind is wrong in $facts"
    [[ "$facts" == *"640"*"480"* ]] || fail "columns: the image pixels are wrong in $facts"
    printf 'COLUMNS image=%s\n' "$facts"
    shot columns-image

    seek_row_named "notes.txt"
    settle
    [[ "$(ipc previewColumnState)" == "text" ]] || fail "columns: notes.txt previews as $(ipc previewColumnState)"
    facts=$(ipc previewFacts)
    [[ "$(fact_labels "$facts")" == "Kind|Size|Modified|Lines" ]] \
        || fail "columns: the text states $(fact_labels "$facts")"
    [[ "$facts" == *"Lines=3"* ]] || fail "columns: the line count is wrong in $facts"
    printf 'COLUMNS text=%s\n' "$facts"

    # The near-white count is what proves a page rendered: the page is the only light surface in the
    # app, and the broken document gives the same tile with no page in it as the control.
    local whole="100%x100%+0+0" near_white="((r+g+b)/3) > 0.9"
    local blank_white page1_white page2_white centre cx cy wx wy i
    seek_row_named "broken.pdf"
    settle
    # A PdfDocument opens asynchronously, so a state read taken at once catches Loading and proves
    # nothing; both polls below are the wait and the assertion in one.
    for i in $(seq 1 25); do
        [[ "$(ipc previewColumnState)" == "error" ]] && break
        sleep 0.2
    done
    [[ "$(ipc previewColumnState)" == "error" ]] \
        || fail "columns: a truncated pdf previews as $(ipc previewColumnState), not error"
    shot columns-pdf-broken
    blank_white=$(count_pixels "$evidence_dir/columns-pdf-broken.png" "$whole" "$near_white")

    seek_row_named "manual.pdf"
    settle
    for i in $(seq 1 25); do
        (( $(ipc columnPdfPages) == 2 )) && break
        sleep 0.2
    done
    [[ "$(ipc previewColumnState)" == "pdf" ]] \
        || fail "columns: manual.pdf previews as $(ipc previewColumnState), not pdf"
    [[ "$(ipc columnPdfLoaded)" == "true" ]] \
        || fail "columns: no PdfDocument was built for manual.pdf"
    (( $(ipc columnPdfPages) == 2 )) \
        || fail "columns: manual.pdf reports $(ipc columnPdfPages) pages, not the two it has"
    (( $(ipc columnPdfPage) == 0 )) \
        || fail "columns: a freshly opened document sits on page $(ipc columnPdfPage), not the first"
    facts=$(ipc previewFacts)
    [[ "$facts" == *"Pages=2"* ]] || fail "columns: the pdf tile states $facts, not two pages"
    printf 'COLUMNS pdf=%s\n' "$facts"
    shot columns-pdf-page1
    page1_white=$(count_pixels "$evidence_dir/columns-pdf-page1.png" "$whole" "$near_white")
    # A factor rather than a tuned threshold: page one is 99 per cent white across the whole frame
    # and the control carries only chrome, so the gap is a magnitude and not a margin.
    (( page1_white > 2 * blank_white )) \
        || fail "columns: the pdf frame drew $page1_white near-white pixels against $blank_white with no document, so no page rendered"

    # The column's PDF has no pager: with the keyboard in the preview, l scrolls the second page up to the frame's top.
    key -M ctrl -k Tab -m ctrl >/dev/null
    key l >/dev/null
    settle
    (( $(ipc pdfState false | jq '.scrollY | floor') > 0 )) \
        || fail "columns: l left the pdf at its top, scrolled to $(ipc pdfState false | jq .scrollY)"
    # PdfPageImage renders async, and a pixel count also moves on the gap, so page two counts only once its own image lands.
    for i in $(seq 1 20); do
        [[ "$(ipc pdfState false | jq '.landed | index(1) != null')" == "true" ]] && break
        sleep 0.2
    done
    [[ "$(ipc pdfState false | jq '.landed | index(1) != null')" == "true" ]] \
        || fail "columns: page two never rendered after l, the landed pages are $(ipc pdfState false | jq -c .landed)"
    settle
    shot columns-pdf-page2
    page2_white=$(count_pixels "$evidence_dir/columns-pdf-page2.png" "$whole" "$near_white")
    (( page2_white != page1_white )) \
        || fail "columns: the scrolled pdf draws the same $page1_white near-white pixels as page one, so the scroll changed nothing"
    key -M ctrl -k Tab -m ctrl >/dev/null
    printf 'COLUMNS pdf pixels blank=%s page1=%s page2=%s\n' "$blank_white" "$page1_white" "$page2_white"

    seek_row_named "link"
    settle
    [[ "$(ipc previewColumnState)" == "symlink" ]] || fail "columns: link previews as $(ipc previewColumnState)"
    facts=$(ipc previewFacts)
    [[ "$(fact_labels "$facts")" == "Kind|Target|Points at|Mode" ]] \
        || fail "columns: the symlink states $(fact_labels "$facts")"
    [[ "$facts" == "Kind=Symbolic link|Target=inner|Points at=Folder|"* ]] \
        || fail "columns: the symlink facts are $facts"
    printf 'COLUMNS symlink=%s\n' "$facts"

    seek_row_named "core.dump"
    settle
    facts=$(ipc previewFacts)
    [[ "$(ipc previewColumnState)" == "unsupported" ]] \
        || fail "columns: core.dump previews as $(ipc previewColumnState), not unsupported"
    [[ "$(fact_labels "$facts")" == "Kind|Size|Modified|Mode" ]] \
        || fail "columns: an unpreviewable row states $(fact_labels "$facts")"
    printf 'COLUMNS unsupported=%s state=%s\n' "$facts" "$(ipc previewColumnState)"
    shot columns-unsupported

    # Toggling Hidden files re-asks the shown ancestors under the new key, or every column goes blank.
    key . >/dev/null
    settle
    [[ "$(ipc showHidden)" == "true" ]] || fail "columns: . did not flip showHidden on"
    [[ -n "$(ipc columnParentRowCentre 0)" ]] || fail "columns: toggling Hidden files left the parent column with no rows"
    key . >/dev/null
    settle
    [[ "$(ipc showHidden)" == "false" ]] || fail "columns: a second . did not flip back off"
    [[ -n "$(ipc columnParentRowCentre 0)" ]] || fail "columns: toggling Hidden files back left the parent column with no rows"

    click_chrome list
    settle
    [[ "$(ipc viewMode)" == "list" ]] || fail "columns: the list button did not switch back"
    kill_flea
}

case_views() {
    local menus_checks=0
    # Four fixtures directly inside the sandbox root, which is what its guard allows; the fourth is the
    # grid again after the columns view, so a column kept alive under the grid is proven to plan nothing.
    local root="$fixture_root" pass mode dir r lit fx fy fw fh sx sy sw sh cx cy wx wy p1 p2 p3 p4 changed before i
    for pass in grid list columns again; do views_fixture "$root/views-$pass"; done
    # 44 fillers sort ahead of the fixtures, as the battery's own case directories do, so the seek has to walk past its old 40-row cap.
    for i in $(seq -w 1 44); do mkdir -p "$root/a-filler-$i"; done
    launch "$root"
    wait_listing "$(ls "$root" | wc -l)"
    for pass in grid list columns again; do
        dir="$root/views-$pass"
        mode=$pass
        [[ "$pass" == "again" ]] && mode=grid
        # Sought in the list, where j walks one row; the grid's j walks a tile row. The target view is on before Return.
        switch_view list
        seek_row_named "views-$pass"
        switch_view "$mode"
        key -k Return >/dev/null
        wait_listing 406
        [[ "$(ipc path)" == "$dir" ]] || fail "$mode: Return on the $mode row opened $(ipc path)"
        key -k Home >/dev/null
        settle
        sleep 2
        # $pass, not $mode: the "again" pass runs the grid a second time and shot keeps every capture.
        shot "views-$pass-thumbs"
        # Directories sort first, so the fixture's four media files are rows 2 to 5 in this order.
        # The hidden list has model 0 in grid and columns, so rowAt reads loading there; visibleRowName reads the shown view.
        local names=(a-clip.mp4 b-clip.mp4 c-pic.png d-pic.jpg)
        for r in 2 3 4 5; do
            [[ "$(ipc visibleRowName "$r")" == "${names[r - 2]}" ]] || fail "$mode: row $r is $(ipc visibleRowName "$r"), not ${names[r - 2]}"
            [[ "$(ipc rowThumbReady "$r")" == "true" ]] || fail "$mode: row $r ($(ipc visibleRowName "$r")) has no decoded thumbnail"
            lit=$(lit_in_rect "$evidence_dir/views-$pass-thumbs.png" $(ipc rowThumbRect "$r"))
            (( lit > 30 )) || fail "$mode: row $r's thumbnail box painted $lit lit pixels"
        done
        if [[ "$mode" == "columns" ]]; then
            # The child column's hero: polled across one draw, the way case_background polls the pane's own.
            lit=0
            # Each attempt keeps its own capture: shot refuses to overwrite evidence, so one name
            # across the poll failed the case the moment the first draw came up blank.
            for _attempt in $(seq 1 "$mark_poll_shots"); do
                shot "views-child-hero-$_attempt"
                lit=$(lit_in_rect "$evidence_dir/views-child-hero-$_attempt.png" $(ipc columnChildMarkRect))
                (( lit > 0 )) && break
                sleep "$mark_poll_s"
            done
            (( lit > 0 )) || fail "columns: the child column's hero painted nothing (state $(ipc columnChildEmpty))"
            key j >/dev/null
            settle
            read -r fx fy <<< "$(ipc columnChildRowCentre 1)"
            [[ -n "$fy" ]] || fail "columns: the child column shows no row 1 for sub"
            local context_path context_cursor
            context_path=$(ipc path)
            context_cursor=$(ipc cursor)
            read -r wx wy _ww _wh < <(window_box) || fail "native window coordinates unavailable"
            omarchy-drive click "$((wx + fx))" "$((wy + fy))" right >/dev/null
            sleep 1
            shot views-columns-child-context
            [[ "$(ipc path)" == "$context_path" && "$(ipc cursor)" == "$context_cursor" && "$(ipc contextMenuVisible)" == true ]] \
                || fail "columns: a neighbour menu moved the cursor or columns: path $(ipc path), cursor $(ipc cursor)"
            menus_expect menuState ".snapshotReady and .snapshotPath == \"$dir/sub\" and .snapshotRowName == \"s2.txt\"" 'the neighbour menu acts on its clicked file'
            key -k Escape >/dev/null
            settle
            [[ "$(ipc path)" == "$dir" ]] || fail "columns: dismissing the neighbour menu moved the columns"
            seek_row_named "c-pic.png"
            # Ready, not shown: thumbShown is true while the picture still loads. Polled the way the thumbnail rows are.
            for _attempt in $(seq 1 30); do
                [[ "$(ipc columnFrameReady)" == "true" ]] && break
                sleep 0.1
            done
            [[ "$(ipc previewColumnState)" == "image" && "$(ipc columnFrameReady)" == "true" ]] || fail "columns: the cursor on c-pic.png shows $(ipc previewColumnState), frame ready $(ipc columnFrameReady), thumb shown $(ipc columnThumbShown)"
            shot views-columns-frame
            # Inset past the border and the hairline, so the frame's own outline cannot light the count.
            read -r fx fy fw fh <<< "$(ipc columnFrameRect)"
            lit=$(lit_in_rect "$evidence_dir/views-columns-frame.png" "$((fx + 6))" "$((fy + 6))" "$((fw - 12))" "$((fh - 12))")
            (( lit > 200 )) || fail "columns: the preview frame's interior painted $lit lit pixels for c-pic.png"
            seek_row_named "a-clip.mp4"
            sleep 1
            [[ "$(ipc previewColumnState)" == "video" ]] || fail "columns: the cursor on a-clip.mp4 shows $(ipc previewColumnState)"
            read -r fx fy fw fh <<< "$(ipc columnFrameRect)"
            (( fw > 0 && fh > 0 )) || fail "columns: the frame has no box"
            read -r cx cy <<< "$(ipc columnPlayCentre)"
            omarchy-drive click "$((wx + cx))" "$((wy + cy))" left >/dev/null
            sleep 1.2
            p1=$(ipc columnMediaPosition)
            shot views-video-1
            sleep 1.5
            p2=$(ipc columnMediaPosition)
            shot views-video-2
            [[ "$(ipc columnMediaPlaying)" == "true" && "$(ipc columnPlayerLoaded)" == "true" ]] || fail "columns: play left playing $(ipc columnMediaPlaying), player $(ipc columnPlayerLoaded)"
            (( p2 > p1 )) || fail "columns: the position did not advance, $p1 then $p2"
            # The fixture clip is colour bars with a moving line about 750 px long, so a moved line differs by thousands of pixels and a frozen frame by none.
            changed=$(magick \( "$evidence_dir/views-video-1.png" -crop "${fw}x${fh}+${fx}+${fy}" +repage \) \
                \( "$evidence_dir/views-video-2.png" -crop "${fw}x${fh}+${fx}+${fy}" +repage \) \
                -compose difference -composite -threshold 10% -format "%[fx:int(mean*w*h+0.5)]" info:)
            (( changed > 1000 )) || fail "columns: the video frame changed $changed pixels in 1.5 s of playback"
            omarchy-drive click "$((wx + cx))" "$((wy + cy))" left >/dev/null
            sleep 0.8
            p3=$(ipc columnMediaPosition)
            sleep 1
            p4=$(ipc columnMediaPosition)
            [[ "$(ipc columnMediaPlaying)" == "false" && "$p3" == "$p4" ]] || fail "columns: pause left playing $(ipc columnMediaPlaying), position $p3 then $p4"
            # Select all keeps the cursor and the path, so the only thing that can end the player is the strip going away.
            before="$(ipc cursor)|$(ipc path)"
            key -M ctrl -k a -m ctrl >/dev/null
            settle
            [[ "$(ipc cursor)|$(ipc path)" == "$before" ]] || fail "columns: select all moved the cursor or the path, $before to $(ipc cursor)|$(ipc path)"
            [[ "$(ipc previewColumnState)" == "multi" && "$(ipc columnPlayerLoaded)" == "false" ]] || fail "columns: a multi-selection left the player $(ipc columnPlayerLoaded) in state $(ipc previewColumnState)"
            # ui/js/Focus.js unwinds Escape in order: a standing status message goes before the
            # marks do, so the selection ends on the press after the one that clears the bar.
            local _press
            for _press in 1 2 3; do
                [[ "$(ipc selectionCount)" == "0" ]] && break
                key -k Escape >/dev/null
                settle
            done
            [[ "$(ipc selectionCount)" == "0" ]] || fail "columns: Escape never ended the selection, $(ipc selectionCount) rows still marked"
            [[ "$(ipc previewColumnState)" == "video" && "$(ipc columnPlayerLoaded)" == "false" ]] || fail "columns: back on the video with $(ipc columnPlayerLoaded) player, state $(ipc previewColumnState)"
            omarchy-drive click "$((wx + cx))" "$((wy + cy))" left >/dev/null
            sleep 1
            key j >/dev/null
            sleep 1
            [[ "$(ipc columnPlayerLoaded)" == "false" ]] || fail "columns: moving the cursor away left a player behind"
            printf 'VIEWS columns hero=%s play=%s..%s changed=%s\n' "$lit" "$p1" "$p2" "$changed"
        fi
        key -k Home >/dev/null
        settle
        key -k Return >/dev/null
        settle
        [[ "$(ipc path)" == "$dir/empty" && "$(ipc emptyShown)" == "true" ]] || fail "$mode: Return on row 0 did not enter the empty directory ($(ipc path), empty $(ipc emptyShown))"
        lit=0
        for _attempt in $(seq 1 "$mark_poll_shots"); do
            shot "views-$pass-empty-$_attempt"
            lit=$(lit_in_rect "$evidence_dir/views-$pass-empty-$_attempt.png" $(ipc emptyMarkRect))
            (( lit > 0 )) && break
            sleep "$mark_poll_s"
        done
        (( lit > 0 )) || fail "$mode: the empty directory's hero painted nothing"
        # Its box is the listing slot (a lazy view reports a local origin, once drawing the hero over the sidebar); in columns the whole pane at 2-3 columns, else the free area.
        read -r sx sy sw sh <<< "$(ipc listAreaRect)"
        local ex=$sx ew=$sw span columns
        if [[ "$mode" == "columns" ]]; then
            # This pass pins the whole-pane side of the rule, so it must show 2 or 3 columns; 4 and 5 are pinned below.
            columns=$(ipc columnCount)
            [[ "$columns" =~ ^[23]$ ]] || fail "columns: the views pass shows '$columns' columns, not 2 or 3"
            # fail inside the substitution exits only the subshell, so its status carries the failure here.
            span=$(columns_hero_span "$sx" "$sw" "$columns") || exit 1
            read -r ex ew <<< "$span"
        fi
        rect_is "$(ipc emptyStateRect)" "$ex" "$sy" "$ew" "$sh" 1 \
            || fail "$mode: the hero's box is $(ipc emptyStateRect), not the slot $ex $sy $ew $sh"
        read -r mx my mw mh <<< "$(ipc emptyMarkRect)"
        local want_centre=$((ex + ew / 2)) mark_centre=$((mx + mw / 2))
        (( mark_centre >= want_centre - 2 && mark_centre <= want_centre + 2 )) \
            || fail "$mode: the hero's mark centres at $mark_centre, not $want_centre"
        key -k Backspace >/dev/null
        sleep 1
        key -k Backspace >/dev/null
        sleep 1
        [[ "$(ipc path)" == "$root" ]] || fail "$mode: two Backspaces did not return to the root, path $(ipc path)"
        printf 'VIEWS %s thumbs=ok hero=%s\n' "$pass" "$lit"
    done
    # Four columns pin the free-area side of the count <= 3 threshold, the way the loop's 3 columns pin the whole-pane side.
    seed_ui_state "$fixture_root/views-hero4-state" '{"view":"columns","columnsLimit":4}'
    launch "$root/views-columns/empty"
    cap_resize 2560 1400
    wait_listing 0
    local count4
    count4=$(ipc columnCount)
    [[ "$count4" == "4" && "$(ipc emptyShown)" == "true" ]] || fail "columns: limit 4 drew $count4 columns, empty $(ipc emptyShown)"
    read -r sx sy sw sh <<< "$(ipc listAreaRect)"
    read -r ex ew <<< "$(columns_hero_span "$sx" "$sw" "$count4")"
    rect_is "$(ipc emptyStateRect)" "$ex" "$sy" "$ew" "$sh" 1 \
        || fail "columns: with 4 columns the hero's box is $(ipc emptyStateRect), not the free area $ex $sy $ew $sh"
    printf 'VIEWS columns4 hero=free area %s %s\n' "$ex" "$ew"
    # Five columns put the active column's left edge past where the hero would centre on the pane, so the hero takes the free area.
    seed_ui_state "$fixture_root/views-hero5-state" '{"view":"columns","columnsLimit":5}'
    launch "$root/views-columns/empty"
    cap_resize 2560 1400
    wait_listing 0
    local count5
    count5=$(ipc columnCount)
    [[ "$count5" == "5" && "$(ipc emptyShown)" == "true" ]] || fail "columns: limit 5 drew $count5 columns, empty $(ipc emptyShown)"
    read -r sx sy sw sh <<< "$(ipc listAreaRect)"
    read -r ex ew <<< "$(columns_hero_span "$sx" "$sw" "$count5")"
    rect_is "$(ipc emptyStateRect)" "$ex" "$sy" "$ew" "$sh" 1 \
        || fail "columns: with 5 columns the hero's box is $(ipc emptyStateRect), not the free area $ex $sy $ew $sh"
    printf 'VIEWS columns5 hero=free area %s %s\n' "$ex" "$ew"
    export XDG_STATE_HOME="$suite_state"
    kill_flea
}

case_formats() {
    local dir="$fixture_root/formats" name lit p1 p2 wx wy cx cy _attempt
    formats_fixture "$dir"
    launch "$dir"
    wait_listing 26
    switch_view columns
    read -r wx wy _ww _wh < <(window_box) || fail "native window coordinates unavailable"
    for name in p.jpg p.png p.webp p.heic; do
        column_expect "$name" image
        for _attempt in $(seq 1 40); do [[ "$(ipc columnFrameReady)" == "true" ]] && break; sleep 0.1; done
        [[ "$(ipc columnFrameReady)" == "true" ]] || fail "formats: $name never decoded in the frame"
        lit=$(column_frame_lit "$name")
        (( lit > 200 )) || fail "formats: $name's frame interior painted $lit lit pixels"
    done
    for name in v.mp4 v.mkv v.webm; do
        column_expect "$name" video
        for _attempt in $(seq 1 40); do [[ "$(ipc columnFrameReady)" == "true" ]] && break; sleep 0.1; done
        [[ "$(ipc columnFrameReady)" == "true" ]] || fail "formats: $name's first frame never decoded"
        read -r cx cy <<< "$(ipc columnPlayCentre)"
        omarchy-drive click "$((wx + cx))" "$((wy + cy))" left >/dev/null
        sleep 1.2
        p1=$(ipc columnMediaPosition)
        sleep 1
        p2=$(ipc columnMediaPosition)
        [[ "$(ipc columnMediaPlaying)" == "true" ]] && (( p2 > p1 )) || fail "formats: $name did not play, playing $(ipc columnMediaPlaying), position $p1 then $p2"
    done
    column_expect tone.wav audio
    read -r cx cy <<< "$(ipc columnPlayCentre)"
    omarchy-drive click "$((wx + cx))" "$((wy + cy))" left >/dev/null
    sleep 1.2
    p1=$(ipc columnMediaPosition)
    sleep 1
    p2=$(ipc columnMediaPosition)
    (( p2 > p1 )) || fail "formats: tone.wav did not play, position $p1 then $p2"
    column_expect manual.pdf pdf
    for _attempt in $(seq 1 40); do [[ "$(ipc columnPdfLoaded)" == "true" && "$(ipc columnPdfPages)" == "2" ]] && break; sleep 0.1; done
    [[ "$(ipc columnPdfPages)" == "2" && "$(ipc columnPdfPage)" == "0" ]] || fail "formats: manual.pdf shows $(ipc columnPdfPages) pages, page $(ipc columnPdfPage)"
    lit=$(column_frame_lit manual-p1)
    (( lit > 200 )) || fail "formats: manual.pdf's first page painted $lit lit pixels"
    # The column is a continuous stack: scroll to its second page, wheel to its bottom, then prove the end refuses more.
    capmarkdown_column_pointer formats-pdf-page-turn
    omarchy-drive scroll down 8 >/dev/null || fail 'formats: the PDF stack wheel failed'
    pdf_expect false '.pages == 2 and .scrollY > 0 and (.landed | index(1) != null)' 'formats second stacked page decoded after scrolling'
    # The bottom is reached once the last page's box ("x y w h") ends inside the frame's box.
    pdf_bottom='(.lastPage | test("^[0-9-]+ [0-9-]+ [0-9]+ [0-9]+$")) and ((.lastPage | split(" ") | map(tonumber)) as $p | (.frame | split(" ") | map(tonumber)) as $f | $p[1] + $p[3] <= $f[1] + $f[3] + 1)'
    for _attempt in 1 2 3 4 5 6; do
        jq -e "$pdf_bottom" <<< "$(ipc pdfState false)" >/dev/null 2>&1 && break
        omarchy-drive scroll down 8 >/dev/null || fail 'formats: the PDF stack bottom wheel failed'
        settle
    done
    jq -e "$pdf_bottom" <<< "$(ipc pdfState false)" >/dev/null 2>&1 \
        || fail "formats: the PDF stack never reached its bottom, $(ipc pdfState false)"
    p1=$(ipc pdfState false | jq -r '.scrollY')
    omarchy-drive scroll down 8 >/dev/null || fail 'formats: the PDF stack end wheel failed'
    settle
    p2=$(ipc pdfState false | jq -r '.scrollY')
    [[ "$p2" == "$p1" ]] || fail "formats: the PDF stack scrolled past its last page, $p1 to $p2"
    column_expect broken.pdf error
    [[ "$(ipc columnFailure)" == "This file could not be read." ]] || fail "formats: broken.pdf's sentence is '$(ipc columnFailure)'"
    column_expect sample.txt text
    for _attempt in $(seq 1 40); do [[ "$(ipc columnTextLines)" == "hello from flea|"* ]] && break; sleep 0.1; done
    [[ "$(ipc columnTextLines)" == "hello from flea|second line"* ]] || fail "formats: sample.txt's lines read '$(ipc columnTextLines)'"
    shot formats-sample-lines
    lit=$(lit_in_rect "$evidence_dir/formats-sample-lines.png" $(ipc columnLinesRect))
    (( lit > 50 )) || fail "formats: sample.txt's lines box painted $lit lit pixels"
    column_expect notes.md text
    for _attempt in $(seq 1 40); do [[ "$(ipc columnMarkdownText)" == "Notes"* ]] && break; sleep 0.1; done
    [[ "$(ipc columnMarkdownText)" == "Notes"* ]] || fail "formats: notes.md's rendered text reads '$(ipc columnMarkdownText)'"
    column_expect empty.txt text
    column_expect big.txt text
    for _attempt in $(seq 1 40); do [[ "$(ipc columnTextLines)" == "too large" ]] && break; sleep 0.1; done
    [[ "$(ipc columnTextLines)" == "too large" ]] || fail "formats: big.txt read '$(ipc columnTextLines | cut -c1-40)', not the too-large answer"
    column_expect main.rs code
    for _attempt in $(seq 1 40); do [[ "$(ipc columnTextLines)" == "fn main() {"* ]] && break; sleep 0.1; done
    [[ "$(ipc columnTextLines)" == "fn main() {"* ]] || fail "formats: main.rs's lines read '$(ipc columnTextLines)'"
    column_expect data.json code
    column_expect a.zip archive
    for _attempt in $(seq 1 40); do [[ "$(ipc columnArchiveNames)" == *"sample.txt"* ]] && break; sleep 0.1; done
    [[ "$(ipc columnArchiveNames)" == *"sample.txt"* && "$(ipc columnArchiveNames)" == *"notes.md"* ]] || fail "formats: a.zip's members read '$(ipc columnArchiveNames)'"
    shot formats-zip-members
    lit=$(lit_in_rect "$evidence_dir/formats-zip-members.png" $(ipc columnArchiveRect))
    (( lit > 50 )) || fail "formats: a.zip's member box painted $lit lit pixels"
    column_expect b.tar.zst archive
    for _attempt in $(seq 1 40); do [[ "$(ipc columnArchiveNames)" == *"main.rs"* ]] && break; sleep 0.1; done
    [[ "$(ipc columnArchiveNames)" == *"main.rs"* ]] || fail "formats: b.tar.zst's members read '$(ipc columnArchiveNames)'"
    column_expect empty.zip archive
    sleep 0.5
    [[ "$(ipc columnArchiveNames)" == "" && "$(ipc columnFailure)" == "" ]] || fail "formats: the empty archive reads members '$(ipc columnArchiveNames)', failure '$(ipc columnFailure)'"
    column_expect corrupt.zip error
    [[ "$(ipc columnFailure)" == "This archive could not be read." ]] || fail "formats: corrupt.zip's sentence is '$(ipc columnFailure)'"
    column_expect link-file symlink
    column_expect link-dir symlink
    column_expect link-broken symlink
    column_expect core.dump unsupported
    column_expect shut.jpg image
    sleep 1
    [[ "$(ipc columnFrameReady)" == "false" && "$(ipc columnThumbShown)" == "false" ]] || fail "formats: an unreadable image reports frame ready $(ipc columnFrameReady), thumb shown $(ipc columnThumbShown)"
    key -M ctrl -k a -m ctrl >/dev/null
    settle
    [[ "$(ipc previewColumnState)" == "multi" ]] || fail "formats: select all shows $(ipc previewColumnState), not multi"
    key -k Escape >/dev/null
    settle
    printf 'FORMATS images=4 videos=3 audio=1 pdf=2 text=4 code=2 archives=4 links=3 error=1 unsupported=1 multi=1\n'
    kill_flea
}
