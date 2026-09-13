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

pdf_controls() {
    local overlay="$1" before ignored
    pdf_expect "$overlay" '.pages == 3 and .focused and .control == 1 and (.controls[0].enabled | not)' "initial focus"
    key -k Return >/dev/null
    pdf_expect "$overlay" '.page == 1' "Enter activates Next"
    key -k space >/dev/null
    pdf_expect "$overlay" '.page == 2 and (.controls[1].enabled | not)' "Space activates Next"
    key -k space >/dev/null
    pdf_expect "$overlay" '.page == 2' "disabled Next refuses Space"
    key -M shift -k Tab -m shift >/dev/null
    pdf_expect "$overlay" '.control == 0' "reverse focus skips disabled Next"
    key -k space >/dev/null
    pdf_expect "$overlay" '.page == 1' "Space activates Previous"
    key -k Tab >/dev/null
    pdf_expect "$overlay" '.control == 1' "Tab reaches Next"
    key -k Tab >/dev/null
    pdf_expect "$overlay" '.control == 3' "Tab skips disabled Zoom Out"
    key -k space >/dev/null
    pdf_expect "$overlay" '.zoom == 1.25' "Space activates Zoom In"
    key -M shift -k Tab -m shift >/dev/null
    pdf_expect "$overlay" '.control == 2' "reverse reaches enabled Zoom Out"
    key -k Return >/dev/null
    pdf_expect "$overlay" '.zoom == 1' "Enter activates Zoom Out"
    key -k Tab -k Tab >/dev/null
    pdf_expect "$overlay" '.control == 4' "Tab reaches Expand"
    key -k Tab >/dev/null
    if [[ "$overlay" == true ]]; then
        pdf_expect true '.control == 5' "Tab reaches Close"
        key -k Tab >/dev/null
        pdf_expect true '.control == 0' "forward wraps within Quick Look"
        key -M shift -k Tab -m shift >/dev/null
        pdf_expect true '.control == 5' "reverse wraps within Quick Look"
    else
        pdf_expect false '.control == 0' "forward wraps within inline PDF"
        key -M shift -k Tab -m shift >/dev/null
        pdf_expect false '.control == 4' "reverse wraps within inline PDF"
    fi
    key l >/dev/null
    pdf_expect "$overlay" '.page == 2' "l pages forward"
    key h >/dev/null
    pdf_expect "$overlay" '.page == 1' "h pages back"
    key -k Right >/dev/null
    pdf_expect "$overlay" '.page == 2' "Right pages forward"
    key -k Left >/dev/null
    pdf_expect "$overlay" '.page == 1' "Left pages back"
    key '+' >/dev/null
    pdf_expect "$overlay" '.zoom == 1.25' "plus zooms in"
    key -k Down >/dev/null
    pdf_expect "$overlay" '.scrollY > 0' "Down scrolls page"
    key -k Up >/dev/null
    pdf_expect "$overlay" '.scrollY == 0' "Up scrolls page back"
    before=$(ipc cursor)
    for ignored in j k r d; do
        key -- "$ignored" >/dev/null
        pdf_expect "$overlay" '.scrollY == 0 and .page == 1 and .focused' "listing key $ignored ignored"
    done
    [[ "$(ipc cursor)" == "$before" ]] || fail "PDF listing keys changed listing cursor"
    key -- '-' >/dev/null
    pdf_expect "$overlay" '.zoom == 1' "minus zooms out"
}

case_pdffocus() {
    local dir="$fixture_root/pdffocus" state="$fixture_root/pdffocus-state" preset mode cx cy wx wy ww wh addr
    sandbox_scratch "$dir"
    sandbox_scratch "$state"
    mkdir -p "$state/flea"
    printf 'listing cursor guard\n' > "$dir/a.txt"
    magick \( -size 440x354 xc:white -fill black -font Liberation-Sans -pointsize 24 -annotate +30+45 PAGEONE \) \
           \( -size 440x354 xc:white -fill black -font Liberation-Sans -pointsize 24 -annotate +30+45 PAGETWO \) \
           \( -size 440x354 xc:white -fill black -font Liberation-Sans -pointsize 24 -annotate +30+45 PAGETHREE \) "$dir/manual.pdf"
    [[ -s "$dir/manual.pdf" ]] || fail "PDF fixture generation failed"
    export XDG_STATE_HOME="$state"
    for preset in default vim mac windows; do
        jq -n --arg preset "$preset" '{view:"list",keys:$preset,preview:{column:true,loadOn:"automatic"}}' > "$state/flea/ui.json"
        launch "$dir"
        wait_listing 2
        [[ "$(ipc keymapPreset)" == "$preset" ]] || fail "PDF preset did not load: $preset"
        open_row manual.pdf
        pdf_controls true
        # Close by Space, then Enter, then Escape and Ctrl+Tab in separate real activations.
        key -k space >/dev/null
        [[ "$(ipc previewOpen)" == false ]] || fail "PDF Space did not activate Close"
        open_row manual.pdf
        key -M shift -k Tab -m shift -k Return >/dev/null
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
        pdf_controls false
        pdf_expect false '.control == 4 and .page == 1' "inline Expand retains focus"
        key '+' >/dev/null
        key -k Return >/dev/null
        pdf_expect true '.focused and .page == 1 and .zoom == 1.25' "inline expansion keeps page and zoom"
        [[ "$(ipc previewExpanded)" == true ]] || fail "PDF inline Expand did not fill the window"
        key e >/dev/null
        pdf_expect true '.page == 1 and .zoom == 1.25' "collapse retains page and zoom"
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
        open_row manual.pdf
        pdf_expect true '.focused and .pages == 3' "$mode Quick Look entry"
        key e >/dev/null
        [[ "$(ipc previewExpanded)" == true ]] || fail "PDF e did not expand in $mode"
        # Reuse cardsizes.sh's addressed Hyprland resize, with exact owned PID and geometry checks.
        addr=$(hyprctl -j clients | jq -er --argjson pid "$(flea_pid)" '.[] | select(.pid == $pid) | .address')
        [[ "$addr" =~ ^0x[0-9a-fA-F]+$ ]] || fail "PDF cannot identify owned window"
        omarchy-drive window float flea >/dev/null
        hyprctl dispatch "hl.dsp.window.resize({ x = 800, y = 480, exact = true, window = \"address:$addr\" })" >/dev/null
        omarchy-drive window center flea >/dev/null
        settle
        read -r wx wy ww wh < <(window_box) || fail "native window coordinates unavailable"
        [[ "$ww $wh" == '800 480' ]] || fail "PDF specimen viewport is $ww $wh"
        shot "pdf-$mode-800x480"
        read -r cx cy <<< "$(ipc pdfState true | jq -r '.controls[1].centre')"
        [[ "$cx" =~ ^[0-9]+$ && "$cy" =~ ^[0-9]+$ ]] || fail "PDF missing native Next centre"
        omarchy-drive click "$((wx + cx))" "$((wy + cy))" left >/dev/null
        pdf_expect true '.page == 1' "$mode pointer Next"
        key -k Escape >/dev/null
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
        pdf_expect false '(.frame | split(" ") | map(tonumber)) as $frame |
            (.toolbar | split(" ") | map(tonumber)) as $toolbar |
            $frame[2] > 0 and $frame[3] > 0 and $toolbar[3] > 0 and $toolbar[1] >= ($frame[1] + $frame[3])' \
            "$mode inline controls stay below the page frame"
        shot "pdf-inline-$mode-800x480"
        key -k Escape >/dev/null
        pdf_expect false '(.focused | not)' "$mode inline focus return"
        printf 'PDF view=%s native_entry=ok pointer=ok viewport=%sx%s\n' "$mode" "$ww" "$wh"
        kill_flea
    done
}
