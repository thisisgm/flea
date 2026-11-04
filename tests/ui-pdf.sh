# Sourced by ui.sh: PDF controls are exercised through native keys and pointer input only.
pdf_expect() {
    local overlay="$1" expression="$2" label="$3" observed deadline=$((SECONDS + 10))
    while (( SECONDS < deadline )); do
        observed=$(ipc pdfState "$overlay")
        if jq -e "$expression" <<< "$observed" >/dev/null; then return; fi
        sleep 0.1
    done
    fail "PDF $label: $observed"
}

# Quick Look closes on a fade that outlives previewOpen, so a shot of the listing waits until the overlay no longer draws.
# Sample input: previewSwapState {"column":null,"look":{...},"lookVisible":false}.
pdf_overlay_gone() {
    local label="$1" observed="" deadline=$((SECONDS + 10))
    while (( SECONDS < deadline )); do
        observed=$(ipc previewSwapState)
        if jq -e '.lookVisible == false' <<< "$observed" >/dev/null; then return; fi
        sleep 0.1
    done
    fail "PDF $label: Quick Look still draws after Escape: $observed"
}

# jq over pdfState: the indexes of the marks that draw the keyboard's own frame, [] when none does.
# Sample input: {"controls":[{"name":"Previous","ring":false},{"name":"Next","ring":true}]} answers [1].
pdf_framed='[(.controls // []) | to_entries[] | select(.value.ring) | .key]'

# A warp sends Qt no motion (see hover_row), so the pointer lands on a mark with one uinput pixel.
pdf_pointer_on() {
    omarchy-drive move "$1" "$2" >/dev/null || fail "PDF pointer move failed"
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool mousemove -x 1 -y 0 >/dev/null 2>&1 || fail "PDF pointer nudge failed"
}

# Quick Look's strip, driven by keys; the column's PDF has no strip and case_pdffocus drives its pages.
pdf_controls() {
    local before ignored
    # Quick Look opens on the document with no control picked and no keyboard frame; its keys act on the document there.
    pdf_expect true ".pages == 3 and .focused and .control == -1 and (.controls[0].enabled | not) and $pdf_framed == []" "opens with no control picked"
    key l >/dev/null
    pdf_expect true '.page == 1 and .control == -1 and .focused' "l pages forward with no control picked"
    key '+' >/dev/null
    pdf_expect true '.zoom == 1.25 and .control == -1 and .focused' "plus zooms in with no control picked"
    # A second plus marks Return as handled; a Return that activated any control would turn, zoom, expand or close.
    key -k Return >/dev/null
    key '+' >/dev/null
    pdf_expect true ".zoom == 1.5 and .page == 1 and .control == -1 and .focused and $pdf_framed == []" "Return with no control picked activates nothing"
    [[ "$(ipc previewExpanded)" == false ]] || fail "PDF Return with no control picked expanded Quick Look"
    key h >/dev/null
    key -- '-' >/dev/null
    key -- '-' >/dev/null
    pdf_expect true '.page == 0 and .zoom == 1 and .control == -1' "h and minus act with no control picked"
    # Tab enters the strip at Next.
    key -k Tab >/dev/null
    pdf_expect true ".pages == 3 and .focused and .control == 1 and (.controls[0].enabled | not) and $pdf_framed == [1]" "initial focus"
    key -k Return >/dev/null
    pdf_expect true '.page == 1' "Enter activates Next"
    # Space closes Quick Look for every kind, so Enter activates the focused control there.
    key -k Return >/dev/null
    pdf_expect true '.page == 2 and (.controls[1].enabled | not)' "Return activates Next"
    key -k Return >/dev/null
    pdf_expect true '.page == 2' "disabled Next refuses Return"
    key -M shift -k Tab -m shift >/dev/null
    pdf_expect true '.control == 0' "reverse focus skips disabled Next"
    key -k Return >/dev/null
    pdf_expect true '.page == 1' "Return activates Previous"
    key -k Tab >/dev/null
    pdf_expect true '.control == 1' "Tab reaches Next"
    key -k Tab >/dev/null
    pdf_expect true '.control == 3' "Tab skips disabled Zoom Out"
    key -k Return >/dev/null
    pdf_expect true '.zoom == 1.25' "Return activates Zoom In"
    key -M shift -k Tab -m shift >/dev/null
    pdf_expect true '.control == 2' "reverse reaches enabled Zoom Out"
    key -k Return >/dev/null
    pdf_expect true '.zoom == 1' "Enter activates Zoom Out"
    key -k Tab -k Tab >/dev/null
    pdf_expect true '.control == 4' "Tab reaches Expand"
    key -k Tab >/dev/null
    pdf_expect true '.control == 5' "Tab reaches Close"
    key -k Tab >/dev/null
    pdf_expect true '.control == 0' "forward wraps within Quick Look"
    key -M shift -k Tab -m shift >/dev/null
    pdf_expect true '.control == 5' "reverse wraps within Quick Look"
    key l >/dev/null
    pdf_expect true '.page == 2' "l pages forward"
    key h >/dev/null
    pdf_expect true '.page == 1' "h pages back"
    key -k Right >/dev/null
    pdf_expect true '.page == 2' "Right pages forward"
    key -k Left >/dev/null
    pdf_expect true '.page == 1' "Left pages back"
    key '+' >/dev/null
    pdf_expect true '.zoom == 1.25' "plus zooms in"
    key -k Down >/dev/null
    pdf_expect true '.scrollY > 0' "Down scrolls page"
    key -k Up >/dev/null
    pdf_expect true '.scrollY == 0' "Up scrolls page back"
    before=$(ipc cursor)
    for ignored in j k r d; do
        key -- "$ignored" >/dev/null
        pdf_expect true '.scrollY == 0 and .page == 1 and .focused' "listing key $ignored ignored"
    done
    [[ "$(ipc cursor)" == "$before" ]] || fail "PDF listing keys changed listing cursor"
    key -- '-' >/dev/null
    pdf_expect true '.zoom == 1' "minus zooms out"
}

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
