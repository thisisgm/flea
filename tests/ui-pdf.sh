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
