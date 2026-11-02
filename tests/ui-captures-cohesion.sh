# Every focus-drawing control at rest, hover, focus, press and disabled, one theme per native case; sourced by ui.sh.
. "$repo/tests/lib/pointer-glide.sh"

# Seconds a hover or press poll waits for the control to report its state.
coh_wait_s=3
# Seconds a dialog or an IPC reader may take to show, and the pause between two polls.
coh_open_wait_s=15
coh_poll_s=0.1
# EPOCHREALTIME without its dot counts microseconds; a worker limit in seconds is compared in that unit.
coh_usec_per_s=1000000
# The window every surface is shot in, the pointer's offset from a control for the approach move, and the margin around a region.
coh_win_w=1100
coh_win_h=700
coh_approach_px=6
coh_pad_px=24
# Text size 14 as ipc metrics answers it: body, small, title and row height.
coh_metrics="13 12 14 37"
# Down steps a menu walk takes, and the single stop of a surface with no walk.
coh_menu_stops=5
coh_still_stops=1
# Tab stops one keyboard walk takes at most, so a walk that leaves the surface still ends.
coh_tab_limit=14
# Pointer targets one surface takes at most, one per distinct control look.
coh_target_limit=8
# Seconds one theme may take, leaving teardown time inside the native runner's 900-second limit.
coh_budget_s=840
# Seconds budget expiry allows the existing EXIT teardown before forcibly ending the surface's process group.
coh_teardown_s=30

coh_ydotool() { YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool "$@" >/dev/null 2>&1; }

# A private UI copy adds only the capture reader; the product components and their handlers stay unchanged.
coh_capture_ui() {
    local target="$fixture_root/coh-capture-ui"
    sandbox_scratch "$target"
    cp -a "$repo/ui/." "$target/" || fail "cohesion: the UI fixture could not be copied"
    cp "$repo/tests/ui-captures-reader.qml" "$target/IpcControls.qml" || fail "cohesion: the capture reader could not be installed"
    printf '%s\n' "$target"
}

# A fixture HOME per theme in a marked root beside the picker's fixture: the live palette, gruvbox or catppuccin-latte, at the box's text size.
coh_home() {
    local theme="$1" home="$fixture_root/coh-$1/home" state_theme
    [[ -d "$home" ]] && { printf '%s\n' "$home"; return 0; }
    sandbox_make "$fixture_root/coh-$theme" >/dev/null
    fixture_home_make "$home" >/dev/null
    state_theme="$home/.local/state/omarchy/current"
    if [[ "$theme" != live ]]; then
        cp "/usr/share/omarchy/themes/$theme/colors.toml" "$state_theme/theme/colors.toml" || fail "cohesion: no $theme palette"
        printf '%s\n' "$theme" > "$state_theme/theme.name"
    fi
    printf '%s\n' "$home"
}

coh_launch() {
    local state="$1" real_home="$HOME" home
    home=$(coh_home "$coh_theme")
    if [[ "$coh_theme" != live ]]; then
        # Sample input, one line of colors.toml: foreground = "#d4be98"
        real_foreground=$(grep -E '^foreground' "$home/.local/state/omarchy/current/theme/colors.toml" | grep -oE '#[0-9A-Fa-f]{6}')
    fi
    seed_ui_state "$fixture_root/coh-state-$coh_theme-$RANDOM$RANDOM" "$state"
    export HOME="$home"
    launch "$coh_dir"
    export HOME="$real_home"
    wait_listing "$(find "$coh_dir" -mindepth 1 -maxdepth 1 ! -name '.*' | wc -l)"
    cap_resize "$coh_win_w" "$coh_win_h"
    [[ "$(ipc metrics)" == "$coh_metrics" ]] || fail "cohesion: metrics $(ipc metrics), not text size 14"
}

# Window pixels to the screen through the same frame-carrying uinput path the drag and unlock captures use.
coh_pointer_to() {
    local x="$1" y="$2" wx wy ww wh ax ay outputs
    read -r wx wy ww wh < <(window_box) || fail "cohesion: native window coordinates unavailable"
    ax=$((wx + x)); ay=$((wy + y))
    outputs=$(hyprctl monitors -j) || fail "cohesion: native output geometry unavailable"
    # Sample input: [{"disabled":false,"x":0,"y":0,"width":2560,"height":1440,"scale":1,"transform":0}].
    if ! jq -e --argjson x "$ax" --argjson y "$ay" 'any(.[]; .disabled != true and
        (.scale // 1) > 0 and $x >= .x and $y >= .y and
        $x < .x + ((if ((.transform // 0) % 2) == 1 then .height else .width end) / (.scale // 1)) and
        $y < .y + ((if ((.transform // 0) % 2) == 1 then .width else .height end) / (.scale // 1)))' <<< "$outputs" >/dev/null; then
        printf 'COH_POINTER_OUTSIDE %s %s local=%s,%s target=%s,%s window=%s outputs=%s\n' \
            "$coh_theme" "${name:-unknown}" "$x" "$y" "$ax" "$ay" "$wx $wy $ww $wh" "$(jq -c . <<< "$outputs")"
        fail "cohesion: pointer target is outside every output"
    fi
    pointer_glide "$ax" "$ay" || fail "cohesion: pointer move failed: target=$ax,$ay window=$wx,$wy,$ww,$wh outputs=$(jq -c . <<< "$outputs")"
}

# The left button is held between a press and its release; coh_held is read by the surface subshell's EXIT trap.
coh_held=0
coh_release_held() {
    trap '' HUP INT TERM
    (( coh_held )) || return 0
    coh_held=0
    coh_ydotool click 0x80 || :
}

coh_trash_cleanup() {
    local status="$1"
    coh_release_held
    trash_cleanup "$status"
}

# The status bar's empty left corner, where nothing draws a hover.
coh_park() {
    local wx wy ww wh
    read -r wx wy ww wh < <(window_box) || fail "cohesion: native window coordinates unavailable"
    coh_pointer_to "$coh_approach_px" "$((wh - coh_approach_px))"
    settle
}

# A cancel motion from chrome starts WindowDrag; restore the viewport before any later pointer move.
coh_cancel_press() {
    local before after restored wx wy ww wh address end
    before=$(window_box) || fail "cohesion: pre-release window coordinates unavailable"
    read -r wx wy ww wh <<< "$before"
    coh_pointer_to "$coh_approach_px" "$((wh - coh_approach_px))"
    coh_ydotool click 0x80 || fail "cohesion: pointer release failed"
    coh_held=0
    settle
    after=$(window_box) || fail "cohesion: post-release window coordinates unavailable"
    if [[ "$before" != "$after" ]]; then
        cap_resize "$ww" "$wh"
        address=$(window_addr_of_pid "$(flea_pid)") || fail "cohesion: owned window address unavailable"
        hypr_window_move "$address" "$wx" "$wy" || fail "cohesion: window position restore failed"
        end=$((SECONDS + coh_wait_s))
        restored=$(window_box) || fail "cohesion: restored window coordinates unavailable"
        while [[ "$restored" != "$before" ]] && (( SECONDS < end )); do
            sleep "$coh_poll_s"
            restored=$(window_box) || fail "cohesion: restored window coordinates unavailable"
        done
        printf 'COH_DRAG_RESET %s %s before=%s displaced=%s restored=%s\n' \
            "$coh_theme" "$name" "$before" "$after" "$restored"
        [[ "$restored" == "$before" ]] || fail "cohesion: window geometry was not restored: expected=$before actual=$restored"
    fi
}

# Sample manifest line: "cohesion-live-permissions-focus-0<TAB>live<TAB>permissions<TAB>focus<TAB>0<TAB>DialogButton|Open|900|700", the sidecar being NAME.json.
coh_shot() {
    local name="$1" surface="$2" phase="$3" index="$4" target="$5" sidecar
    sidecar=$(ipc cohesionControls) && [[ -n "$sidecar" ]] || fail "cohesion: the reader gave no sidecar for $name"
    shot "$name"
    printf '%s\n' "$sidecar" > "$evidence_dir/$name.json"
    printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$name" "$coh_theme" "$surface" "$phase" "$index" "$target" >> "$evidence_dir/coh-manifest.tsv"
}

# Sample input: {"focus":null,"controls":[{"kind":"DialogButton","name":"Open","x":900,"y":700,"hovered":true,"hoverable":true}]}
# Press scaling moves the top-left, so identity is the kind, name and original hit point inside the current control.
coh_wait_flag() {
    local target="$1" flag="$2" sig end=$((SECONDS + coh_wait_s))
    sig=$(jq -r '"\(.kind)|\(.name)|\(.x)|\(.y)"' <<< "$target")
    while (( SECONDS < end )); do
        ipc cohesionControls | jq -e --argjson target "$target" --arg flag "$flag" '
            [.controls[] | select(.kind == $target.kind and .name == $target.name and .[$flag]
                and .x <= $target.targetX and $target.targetX < .x + .w
                and .y <= $target.targetY and $target.targetY < .y + .h)] | length > 0' >/dev/null 2>&1 && return 0
        sleep "$coh_poll_s"
    done
    printf 'COH_MISS %s %s never reported %s\n' "$coh_theme" "$sig" "$flag"
    return 1
}

# Polls one IPC reader until it answers WANT.
coh_until() {
    local reader="$1" want="$2" end=$((SECONDS + coh_open_wait_s))
    while (( SECONDS < end )); do
        [[ "$(ipc "$reader")" == "$want" ]] && { settle; return 0; }
        sleep "$coh_poll_s"
    done
    fail "cohesion: $reader never answered $want"
}

# Any dialog button showing, for the dialogs without a reader of their own.
coh_dialog_up() {
    local end=$((SECONDS + coh_open_wait_s))
    while (( SECONDS < end )); do
        ipc cohesionControls | jq -e '[.controls[] | select(.kind == "DialogButton")] | length > 0' >/dev/null 2>&1 && { settle; return 0; }
        sleep "$coh_poll_s"
    done
    fail "cohesion: no dialog button ever showed"
}

coh_open_permissions() { menus_file_menu notes.txt key; menus_choose permissions; cap_permissions_settled; }
coh_open_delete() {
    menus_file_menu notes.txt key
    menus_choose deletePermanently
    menus_expect menuDialogState '.confirmation.opened' "the delete confirmation opens"
}
coh_open_network() { key a >/dev/null; menus_expect dialogOpen '. == true' "the network dialog opens"; }
coh_open_openwith() { openwith_open_dialog notes.txt; }
coh_open_convert() { menus_file_menu photo.png key; menus_choose convert; coh_dialog_up; }
coh_open_copyto_empty() {
    menus_file_menu notes.txt key
    menus_choose copyTo
    coh_dialog_up
    key -M ctrl a -m ctrl >/dev/null
    key -k BackSpace >/dev/null
    settle
}
coh_open_newfile() {
    click_background
    menus_choose newFile
    menus_expect menuDialogState '.opened and .action == "newFile"' "New File opens"
}
coh_open_properties() { menus_file_menu notes.txt key; menus_choose properties; coh_dialog_up; }
coh_open_collide() {
    seek_row_named notes.txt || fail "cohesion: no notes.txt row"
    key y >/dev/null
    seek_row_named to || fail "cohesion: no to row"
    key -k Return >/dev/null
    wait_path "$coh_dir/to"
    wait_listing 1
    key p >/dev/null
    menus_expect collideState '.opened' "the paste collision opens"
}
coh_open_keymap() { key '?' >/dev/null; coh_until keymapSheetOpen true; }
coh_open_pathbar() { key : >/dev/null; coh_until pathBarOpen true; }
coh_open_rename() { seek_row_named notes.txt || fail "cohesion: no notes.txt row"; key r >/dev/null; coh_until renameEditorLive true; }
coh_open_settings() { settings_open_key; coh_until settingsOpen true; }
coh_open_qlpdf() { seek_row_named manual.pdf || fail "cohesion: no manual.pdf row"; key -k space >/dev/null; wait_preview_state pdf; settle; }
coh_open_qlmd() { seek_row_named guide.md || fail "cohesion: no guide.md row"; key -k space >/dev/null; coh_until previewOpen true; sleep 1; }
coh_open_menu() { menus_file_menu notes.txt key; }
coh_open_main() { settle; }
# The Trash strip needs ui-trash.sh's private session bus and GIO Trash, so this opener launches Flea itself.
coh_open_trash_self() {
    local root real_home="$HOME" home
    [[ "$(realpath -e "$(command -v gio)")" == /usr/bin/gio ]] || fail "cohesion: product gio resolves to a stub"
    sandbox_require "$fixture_root"
    trash_box=$(mktemp -d "$fixture_root/coh-trash.XXXXXXXX") || fail "cohesion: trash fixture creation failed"
    printf 'native private Trash\n' > "$trash_box/.flea-test-sandbox"
    export XDG_DATA_HOME="$trash_box/data" XDG_CONFIG_HOME="$trash_box/config"
    export XDG_STATE_HOME="$trash_box/state" XDG_CACHE_HOME="$trash_box/cache"
    for root in "$XDG_DATA_HOME" "$XDG_CONFIG_HOME" "$XDG_STATE_HOME" "$XDG_CACHE_HOME"; do
        trash_guard "$root"
        mkdir -p "$root" || fail "cohesion: trash root creation failed"
    done
    home=$(coh_home "$coh_theme")
    # The text size lives in the user shell.toml, which an XDG_CONFIG_HOME override would otherwise hide.
    if [[ -f "$home/.config/omarchy/shell.toml" ]]; then
        mkdir -p "$XDG_CONFIG_HOME/omarchy" && cp "$home/.config/omarchy/shell.toml" "$XDG_CONFIG_HOME/omarchy/shell.toml"
    fi
    if [[ "$coh_theme" != live ]]; then
        real_foreground=$(grep -E '^foreground' "$home/.local/state/omarchy/current/theme/colors.toml" | grep -oE '#[0-9A-Fa-f]{6}')
    fi
    "$flea_bin" --ui-state '{"view":"list","keys":"default","preview":{"column":false},"menu":{"hidden":[]}}' >/dev/null \
        || fail "cohesion: trash preferences could not be stored inside the fixture"
    payload="$trash_box/payload"
    trash_guard "$payload"
    mkdir "$payload" || fail "cohesion: trash payload creation failed"
    trash_start_bus
    trap 'coh_trash_cleanup $?' EXIT
    printf 'alpha\n' > "$payload/alpha.txt"
    printf 'beta\n' > "$payload/beta.txt"
    export HOME="$home"
    launch "$payload"
    export HOME="$real_home"
    wait_listing 2
    cap_resize "$coh_win_w" "$coh_win_h"
    [[ "$(ipc metrics)" == "$coh_metrics" ]] || fail "cohesion: trash metrics $(ipc metrics), not text size 14"
    trash_move alpha.txt 0 1
    trash_move beta.txt 1 0
    trash_rail
    trash_wait '.opened and .total == 2 and (.busy == false)'
    settle
}
coh_open_colpdf() { seek_row_named manual.pdf || fail "cohesion: no manual.pdf row"; coh_until previewColumnState pdf; coh_until columnPdfLoaded true; }

# One real wheel step wakes the PDF bar; a pointer in its lane keeps it drawn through slow screenshots.
coh_pdf_receipt() {
    local found exposed reader
    # Sample input: {"captureReader":"porttp4d","controls":[{"surface":"PreviewColumn","kind":"ViewportScrollBar","value":"vertical","exposed":true}]}.
    read -r found exposed reader < <(jq -r '[any(.controls[]; .surface == "PreviewColumn" and .kind == "ViewportScrollBar" and .value == "vertical"),
        any(.controls[]; .surface == "PreviewColumn" and .kind == "ViewportScrollBar" and .value == "vertical" and .exposed), .captureReader // "missing"] | @tsv' <<< "$observed")
    printf 'COH_WAKE %s cols-pdf bar=%s after=%s frame="%s" found=%s reader=%s observed=%s\n' \
        "$coh_theme" "$exposed" "$(( $(date +%s%3N) - started ))" "$frame" "$found" "$reader" "$observed"
}

coh_wake_colpdf() {
    local started deadline observed frame fx fy fw fh tx ty
    started=$(date +%s%3N)
    deadline=$((started + coh_wait_s * 1000))
    frame=$(ipc columnFrameRect) || fail "cohesion: the PDF frame reader failed"
    # Sample input: columnFrameRect answers "810 40 275 560", x y width height in window pixels.
    read -r fx fy fw fh <<< "$frame"
    [[ "$fx" =~ ^-?[0-9]+$ && "$fy" =~ ^-?[0-9]+$ && "$fw" =~ ^[0-9]+$ && "$fh" =~ ^[0-9]+$ ]] \
        && (( fw > 2 && fh > 2 )) || fail "cohesion: invalid PDF frame [$frame]"
    observed=$(ipc cohesionControls) || fail "cohesion: the PDF bar reader failed"
    # Sample input: {"captureReader":"porttp4d","controls":[],"previewColumns":[],"scrollBars":[]}.
    if ! jq -e '.captureReader == "porttp4d"' <<< "$observed" >/dev/null; then
        coh_pdf_receipt
        fail "cohesion: stale capture reader; expected porttp4d, see COH_HARNESS and COH_WAKE"
    fi
    # Sample input: {"controls":[{"surface":"PreviewColumn","kind":"ViewportScrollBar","value":"vertical","exposed":true,"hovered":true}]}.
    coh_pdf_bar=$(jq -c '.controls[] | select(.surface == "PreviewColumn" and .kind == "ViewportScrollBar" and .value == "vertical" and .exposed and .hovered)' <<< "$observed")
    [[ -z "$coh_pdf_bar" ]] || { coh_pdf_receipt; return 0; }
    coh_pointer_to "$((fx + fw / 2))" "$((fy + fh / 2))"
    omarchy-drive scroll down 1 >/dev/null || fail "cohesion: the PDF wake wheel step failed"
    while (( $(date +%s%3N) < deadline )); do
        observed=$(ipc cohesionControls) || fail "cohesion: the PDF wake reader failed"
        # Sample input: {"controls":[{"surface":"PreviewColumn","kind":"ViewportScrollBar","value":"vertical","exposed":true,"targetX":1078,"targetY":115}]}.
        coh_pdf_bar=$(jq -c '.controls[] | select(.surface == "PreviewColumn" and .kind == "ViewportScrollBar" and .value == "vertical" and .exposed)' <<< "$observed")
        if [[ -n "$coh_pdf_bar" ]]; then
            # Sample input: {"targetX":1078,"targetY":115}; jq answers "1078<TAB>115".
            read -r tx ty < <(jq -r '[.targetX,.targetY] | @tsv' <<< "$coh_pdf_bar")
            coh_pointer_to "$tx" "$ty"
            if coh_wait_flag "$coh_pdf_bar" hovered; then
                observed=$(ipc cohesionControls) || fail "cohesion: the PDF hover receipt reader failed"
                coh_pdf_receipt
                return 0
            fi
            break
        fi
        sleep "$coh_poll_s"
    done
    observed=$(ipc cohesionControls) || fail "cohesion: the failed PDF wake reader failed"
    coh_pdf_receipt
    fail "cohesion: cols-pdf reports no exposed pointer target or live lane hover after one wheel step"
}

# The transfer card stays up because the owned backend is paused mid-copy; the pause and the window end with the surface's subshell.
coh_transfer_bytes=$((2 * 1024 * 1024 * 1024))
coh_open_transfer_self() {
    local real_home="$HOME" home menu_box cancel
    home=$(coh_home "$coh_theme")
    operations_footer_fixture "cohesion-$coh_theme"
    if [[ -f "$home/.config/omarchy/shell.toml" ]]; then
        mkdir -p "$XDG_CONFIG_HOME/omarchy" && cp "$home/.config/omarchy/shell.toml" "$XDG_CONFIG_HOME/omarchy/shell.toml"
    fi
    if [[ "$coh_theme" != live ]]; then
        real_foreground=$(grep -E '^foreground' "$home/.local/state/omarchy/current/theme/colors.toml" | grep -oE '#[0-9A-Fa-f]{6}')
    fi
    permissions_listing="$menu_box/payload"
    menus_guard "$permissions_listing/a-first.txt"
    printf 'original a-first.txt\n' > "$permissions_listing/a-first.txt" || fail "cohesion: transfer fixture failed"
    menus_guard "$permissions_listing/photo.heic"
    truncate -s "$coh_transfer_bytes" "$permissions_listing/photo.heic" || fail "cohesion: transfer fixture could not be sized"
    operations_stopped=""
    trap 'permissions_resume_stopped "$operations_stopped"; kill_flea' EXIT
    export HOME="$home"
    launch "$permissions_listing"
    export HOME="$real_home"
    wait_listing 2
    cap_resize "$coh_win_w" "$coh_win_h"
    [[ "$(ipc metrics)" == "$coh_metrics" ]] || fail "cohesion: transfer metrics $(ipc metrics), not text size 14"
    click_row 0 left
    key -M shift -k Down -m shift >/dev/null || fail "cohesion: transfer selection failed"
    operations_copy_to "$menu_box/destination" live
    menus_expect statusActivityState '.activities[0].running and .transferCard.visible' "cohesion: the transfer card is up"
    operations_pause_backend
    settle
    # Sample input: the reader answer above; the region is the card's own Cancel with its margin.
    cancel=$(ipc cohesionControls | jq -r --argjson pad "$coh_pad_px" '[.controls[] | select(.kind == "DialogButton" and .name == "Cancel")][0]
        | if . == null then "" else "\(.x - $pad) \(.y - $pad) \(.x + .w + $pad) \(.y + .h + $pad)" end')
    [[ -n "$cancel" ]] || fail "cohesion: the transfer card shows no Cancel"
    region="$cancel"
}

# One surface: open it, walk its keyboard stops, then point at one control of each look, hover and held press.
coh_run_surface() {
    local name="$1" opener="$2" state="$3" step="$4" region="${5:-}" n="cohesion-$coh_theme-$1"
    local k=0 limit="$coh_tab_limit" first="" sig st targets count i t cx cy tsig rx0 ry0 rx1 ry1 missed=0 surface="" coh_pdf_bar=""
    [[ "$opener" == *_self ]] || coh_launch "$state"
    "$opener"
    printf 'COH_GEOMETRY %s %s window=%s outputs=%s\n' "$coh_theme" "$name" "$(window_box)" "$(hyprctl monitors -j | jq -c .)"
    coh_park
    # A menu keeps the keyboard on itself and moves its highlight, so its walk is a fixed count of Down steps.
    [[ "$step" == down ]] && limit="$coh_menu_stops"
    [[ "$step" == none ]] && limit="$coh_still_stops"
    for ((k = 0; k < limit; k++)); do
        st=$(ipc cohesionControls) || fail "cohesion: the reader failed"
        # Sample input: {"focus":{"kind":"DialogButton","name":"Open","x":900,"y":700,"via":"owner"},"controls":[...]}
        sig=$(jq -r '.focus | if . == null then "none" else "\(.kind)|\(.name)|\(.x)|\(.y)" end' <<< "$st")
        [[ "$step" == tab ]] && (( k > 0 )) && [[ "$sig" == "$first" ]] && break
        (( k == 0 )) && first="$sig"
        coh_shot "$n-focus-$k" "$name" focus "$k" "$sig"
        case "$step" in down) key -k Down >/dev/null ;; tab) key -k Tab >/dev/null ;; esac
        settle
    done
    # Sample input: the focus sidecars NAME.json above, each {"focus":{"x":900,"y":700,"w":80,"h":30},"controls":[...]}.
    [[ -n "$region" ]] || region=$(jq -rs --argjson pad "$coh_pad_px" --arg whole "0 0 $coh_win_w $coh_win_h" '[.[] | .focus // empty | select(.w > 0)] |
        if length == 0 then $whole else "\((map(.x) | min) - $pad) \((map(.y) | min) - $pad) \((map(.x + .w) | max) + $pad) \((map(.y + .h) | max) + $pad)" end' \
        "$evidence_dir/$n"-focus-*.json)
    read -r rx0 ry0 rx1 ry1 <<< "$region"
    case "$name" in
        permissions) surface=PermissionsDialog ;; network) surface=NetworkDialog ;; settings) surface=SettingsPanel ;;
        menu) surface=ContextMenu ;; ql-pdf|ql-md) surface=Preview ;; cols-pdf) surface=PreviewColumn ;;
        keymap) surface=KeymapSheet ;; openwith) surface=OpenWithDialog ;; convert) surface=ConvertDialog ;;
        copyto-empty|newfile|properties) surface=MenuActionDialog ;; delete) surface=TrashConfirm ;;
        collide) surface=CollideConfirm ;; trash) surface=TrashView ;; transfer) surface=TransferCard ;;
    esac
    [[ "$name" != cols-pdf ]] || coh_wake_colpdf
    # One control per distinct look: kind, role, value and enabled state.
    st=$(ipc cohesionControls) || fail "cohesion: the target reader failed"
    targets=$(jq -c --arg surface "$surface" --argjson x0 "$rx0" --argjson y0 "$ry0" --argjson x1 "$rx1" --argjson y1 "$ry1" --argjson cap "$coh_target_limit" '
        [.controls[] | select(.exposed and .w > 2 and .h > 2 and .surface == $surface
            and .targetX >= $x0 and .targetY >= $y0 and .targetX < $x1 and .targetY < $y1)]
        | unique_by([.kind, .primary, .destructive, .member, .value, .enabled]) | .[:$cap]' <<< "$st")
    count=$(jq length <<< "$targets")
    printf 'COH_TARGETS %s %s surface=%s region=%s available=%s selected=%s\n' "$coh_theme" "$name" "$surface" "$region" \
        "$(jq -c '[.controls[] | {surface,kind,name,exposed,targetX,targetY}]' <<< "$st")" "$targets"
    (( count > 0 )) || fail "cohesion: $name reports no exposed pointer target"
    for ((i = 0; i < count; i++)); do
        t=$(jq -c ".[$i]" <<< "$targets")
        if [[ "$name" == cols-pdf ]] && jq -e '.kind == "ViewportScrollBar" and .value == "vertical"' <<< "$t" >/dev/null; then
            coh_wake_colpdf
            t="$coh_pdf_bar"
        fi
        tsig=$(jq -r '"\(.kind)|\(.name)|\(.x)|\(.y)"' <<< "$t")
        cx=$(jq -r .targetX <<< "$t")
        cy=$(jq -r .targetY <<< "$t")
        coh_pointer_to "$cx" "$cy"
        if [[ "$(jq -r .enabled <<< "$t")" != true ]]; then
            coh_shot "$n-disabled-$i" "$name" disabled "$i" "$tsig"
            coh_park
            continue
        fi
        # A kind that reports hover must show it before the shot; a kind with no hovered property has no flag to wait for.
        if [[ "$(jq -r .hoverLive <<< "$t")" == true ]] && ! coh_wait_flag "$t" hovered; then
            missed=$((missed + 1)); coh_park; continue
        fi
        settle
        coh_shot "$n-hover-$i" "$name" hover "$i" "$tsig"
        case "$(jq -r .kind <<< "$t")" in MenuRow|SidebarRow) coh_park; continue ;; esac
        [[ "$(jq -r .pressLive <<< "$t")" == true ]] || { coh_park; continue; }
        coh_held=1
        coh_ydotool click 0x40 || fail "cohesion: pointer press failed"
        if ! coh_wait_flag "$t" pressed; then
            missed=$((missed + 1))
        else
            settle
            coh_shot "$n-press-$i" "$name" press "$i" "$tsig"
        fi
        coh_cancel_press
        # A release that acted anyway closes the surface; reopen it for the next control.
        if ! ipc cohesionControls | jq -e --arg sig "$tsig" '[.controls[] | select("\(.kind)|\(.name)|\(.x)|\(.y)" == $sig)] | length > 0' >/dev/null 2>&1; then
            printf 'COH_REOPEN %s %s after %s\n' "$coh_theme" "$name" "$tsig"
            [[ "$opener" == *_self ]] && break
            coh_launch "$state"
            "$opener"
        fi
        coh_park
    done
    printf 'COH_DONE %s %s focus=%s targets=%s missed=%s\n' "$coh_theme" "$name" "$k" "$count" "$missed"
    (( missed == 0 )) || fail "cohesion: $name never reached $missed hover or press states"
}

# The caller disables errexit while this runner waits; job control gives only its worker and watchdog separate process groups.
coh_bounded() {
    local name="$1" limit="$2" remaining=$((coh_deadline - SECONDS)) started=${EPOCHREALTIME/./} finished worker="" watchdog="" status monitor=0 signal end group
    local previous_traps interrupted=0
    local -a pending_groups=()
    shift 2
    if (( remaining <= 0 )); then
        printf 'COH_SKIPPED %s %s over the %ss budget\n' "$coh_theme" "$name" "$coh_budget_s"
        return 124
    fi
    (( limit > 0 && limit < remaining )) || limit=$remaining
    previous_traps=$(trap -p HUP INT TERM)
    trap 'trap "" HUP INT TERM; interrupted=129; (( ${#pending_groups[@]} == 0 )) || kill -TERM -- "${pending_groups[@]}" 2>/dev/null || :; pending_groups=()' HUP
    trap 'trap "" HUP INT TERM; interrupted=130; (( ${#pending_groups[@]} == 0 )) || kill -TERM -- "${pending_groups[@]}" 2>/dev/null || :; pending_groups=()' INT
    trap 'trap "" HUP INT TERM; interrupted=143; (( ${#pending_groups[@]} == 0 )) || kill -TERM -- "${pending_groups[@]}" 2>/dev/null || :; pending_groups=()' TERM
    [[ $- == *m* ]] && monitor=1
    set -m
    ( set +m; set -e; trap - EXIT HUP INT TERM; trap 'trap "" HUP INT TERM; exit 124' TERM; "$@" ) < /dev/null &
    worker=$!
    pending_groups+=("-$worker")
    (
        set +m
        trap - EXIT HUP INT TERM
        command sleep "$limit"
        printf 'COH_SKIPPED %s %s cut after %ss (theme remaining %ss)\n' "$coh_theme" "$name" "$limit" "$remaining"
        kill -TERM -- "-$worker" 2>/dev/null || :
        command sleep "$coh_teardown_s"
        kill -KILL -- "-$worker" 2>/dev/null || :
    ) < /dev/null &
    watchdog=$!
    pending_groups+=("-$watchdog")
    (( monitor )) || set +m
    status=$interrupted
    if (( interrupted == 0 )); then
        wait "$worker"
        status=$?
    fi
    finished=${EPOCHREALTIME/./}
    (( ${#pending_groups[@]} == 0 )) || kill -TERM -- "${pending_groups[@]}" 2>/dev/null || :
    pending_groups=()
    # The native window belongs to launch/theme teardown; only these two recorded child groups are drained here.
    for group in "$worker" "$watchdog"; do
        if kill -0 -- "-$group" 2>/dev/null; then
            for signal in TERM KILL; do
                # TERM was sent once; a second one can re-enter EXIT and skip the held release.
                [[ "$signal" != KILL ]] || kill -KILL -- "-$group" 2>/dev/null || :
                end=$(( ${EPOCHREALTIME/./} + coh_teardown_s * coh_usec_per_s ))
                while kill -0 -- "-$group" 2>/dev/null && (( ${EPOCHREALTIME/./} < end )); do
                    command sleep "$coh_poll_s"
                done
                kill -0 -- "-$group" 2>/dev/null || break
            done
            if kill -0 -- "-$group" 2>/dev/null; then
                printf 'COH_GROUP_FAILED %s %s pgid=%s survived teardown\n' "$coh_theme" "$name" "$group"
                (( status != 0 )) || status=1
            else
                printf 'COH_GROUP %s %s pgid=%s remaining=none\n' "$coh_theme" "$name" "$group"
            fi
        fi
    done
    if ! kill -0 "$worker" 2>/dev/null; then wait "$worker" 2>/dev/null || :; fi
    if ! kill -0 "$watchdog" 2>/dev/null; then wait "$watchdog" 2>/dev/null || :; fi
    trap - HUP INT TERM
    eval "$previous_traps"
    if (( interrupted != 0 )); then
        printf 'COH_INTERRUPTED %s %s signal=%s\n' "$coh_theme" "$name" "$((interrupted - 128))"
        # The caller handles the original signal after its traps are restored and both child groups are stopped.
        kill -"$((interrupted - 128))" "$BASHPID"
        return "$interrupted"
    fi
    if (( status == 0 && finished - started >= limit * coh_usec_per_s )); then
        printf 'COH_SKIPPED %s %s returned success after its %ss limit\n' "$coh_theme" "$name" "$limit"
        status=124
    fi
    return "$status"
}

coh_surface_work() { coh_held=0; trap coh_release_held EXIT; coh_run_surface "$@"; }

# The worker runs with errexit on, outside an if-condition that would mask failing steps; the caller's errexit is restored.
coh_surface() {
    local status=0 errexit=0 started=$SECONDS
    local trash_box="" payload="" trash_checks=0 trash_case_label=cohesion
    local trash_parent_bus_id="" trash_private_bus_id="" trash_bus_address="" trash_bus_pid="" trash_provider_pid=""
    [[ $- == *e* ]] && errexit=1
    set +e
    coh_bounded "$1" 0 coh_surface_work "$@"
    status=$?
    if (( errexit )); then set -e; fi
    printf 'COH_TIMING %s %s seconds=%s exit=%s\n' "$coh_theme" "$1" "$((SECONDS - started))" "$status"
    (( status == 0 )) && return 0
    printf 'COH_FAILED %s %s\n' "$coh_theme" "$1"
    coh_failed+=" $coh_theme/$1"
}

# The picker is the portal's own process, so its walk runs in Python against the picker's IPC. Each run takes a fresh root, so no picker state carries over.
coh_picker_work() {
    local home
    kill_flea
    home=$(coh_home "$coh_theme")
    FLEA_BIN="$flea_bin" FLEA_UI="$flea_ui" python3 "$repo/tests/ui-captures-cohesion-picker.py" \
        "$fixture_root/coh-$coh_theme/picker-$RANDOM$RANDOM" "$home" "$evidence_dir" "$coh_theme"
}

coh_picker() {
    local status=0 errexit=0 started=$SECONDS
    [[ $- == *e* ]] && errexit=1
    set +e
    coh_bounded picker 180 coh_picker_work
    status=$?
    if (( errexit )); then set -e; fi
    printf 'COH_TIMING %s picker seconds=%s exit=%s\n' "$coh_theme" "$((SECONDS - started))" "$status"
    (( status == 0 )) && return 0
    printf 'COH_FAILED %s picker status %s\n' "$coh_theme" "$status"
    coh_failed+=" $coh_theme/picker"
}

coh_fixture() {
    local dir="$1"
    sandbox_scratch "$dir"
    printf 'notes\n' > "$dir/notes.txt"
    printf 'other\n' > "$dir/other.txt"
    mkdir -p "$dir/to"
    printf 'destination\n' > "$dir/to/notes.txt"
    printf '# Guide\n\nA paragraph with a [link](https://example.com).\n\n- one\n- two\n' > "$dir/guide.md"
    chmod 0644 "$dir/notes.txt" "$dir/other.txt" || fail "cohesion: fixture modes failed"
    magick -size 320x200 gradient:'#336699-#99ccff' "$dir/photo.png" || fail "cohesion: magick produced no photo.png"
    magick \( -size 400x560 xc:white -fill black "${flea_magick_font_args[@]}" -pointsize 40 -annotate +40+80 'PAGEONE' \) \
           \( -size 400x560 xc:white -fill black "${flea_magick_font_args[@]}" -pointsize 40 -annotate +40+80 'PAGETWO' \) \
           "$dir/manual.pdf" || fail "cohesion: magick produced no manual.pdf"
}

# Full themes use the per-theme selectors; COH_SURFACES may still group a smaller cross-theme capture.
case_cap_cohesion() {
    local coh_dir="$fixture_root/cohesion" coh_theme coh_failed="" live_foreground="$real_foreground" menus_checks=0 s coh_deadline
    local list='{"keys":"default","view":"list","menu":{"hidden":[]}}'
    local cols='{"keys":"default","view":"columns","columnsLimit":3,"preview":{"column":true,"loadOn":"automatic"},"menu":{"hidden":[]}}'
    local themes="${COH_THEMES:-live gruvbox catppuccin-latte}" want="${COH_SURFACES:-all}"
    local -a theme_words=($themes)
    if [[ "$want" == all ]] && (( ${#theme_words[@]} > 1 )); then
        fail "cohesion: split full capture into ui:cap_cohesion_live ui:cap_cohesion_gruvbox ui:cap_cohesion_latte ui:cap_cohesion_kanagawa (one 900s slot each)"
    fi
    local flea_ui
    flea_ui=$(coh_capture_ui) || fail "cohesion: the UI fixture could not be prepared"
    # Sample input: sha256sum answers "0123456789abcdef...  tests/ui-captures-reader.qml"; the first field names the source.
    printf 'COH_HARNESS helper=%s reader=%s ui=%s\n' "$(sha256sum "$repo/tests/ui-captures-cohesion.sh" | cut -d' ' -f1)" \
        "$(sha256sum "$flea_ui/IpcControls.qml" | cut -d' ' -f1)" "$flea_ui"
    coh_fixture "$coh_dir"
    mkdir -p "$evidence_dir" || fail "cohesion: the evidence directory could not be created"
    : > "$evidence_dir/coh-manifest.tsv"
    for coh_theme in "${theme_words[@]}"; do
        coh_deadline=$((SECONDS + coh_budget_s))
        # The list comes in on fd 3 so no surface reads it off stdin, and it leads with one surface of each pane class for a run cut by its budget.
        while read -r -u 3 s opener state step region; do
            [[ "$want" == all || " $want " == *" $s "* ]] || continue
            [[ "$s" == picker ]] && { coh_picker; continue; }
            [[ "$region" == - ]] && region=""
            coh_surface "$s" "$opener" "${!state}" "$step" "${region//,/ }"
        done 3<<'EOF'
permissions coh_open_permissions list tab -
network coh_open_network list tab -
settings coh_open_settings list tab -
window coh_open_main list tab 0,0,1100,40
trash coh_open_trash_self list tab -
transfer coh_open_transfer_self list none -
menu coh_open_menu list down -
rail coh_open_main list none 0,40,195,670
ql-pdf coh_open_qlpdf list tab -
picker - - - -
delete coh_open_delete list tab -
openwith coh_open_openwith list tab -
properties coh_open_properties list tab -
collide coh_open_collide list tab -
convert coh_open_convert list tab -
copyto-empty coh_open_copyto_empty list tab -
newfile coh_open_newfile list tab -
keymap coh_open_keymap list none -
pathbar coh_open_pathbar list tab -
rename coh_open_rename list tab -
ql-md coh_open_qlmd list tab -
cols-pdf coh_open_colpdf cols none 640,40,1100,670
EOF
        kill_flea
        real_foreground="$live_foreground"
    done
    printf 'CAP_COHESION shots=%s failed=%s\n' "$(wc -l < "$evidence_dir/coh-manifest.tsv")" "${coh_failed:-none}"
    [[ -z "$coh_failed" ]] || fail "cohesion: surfaces failed:$coh_failed"
}

# One theme per case, so a minipc slot runs the themes it has time for.
case_cap_cohesion_live() { COH_THEMES=live case_cap_cohesion; }
case_cap_cohesion_gruvbox() { COH_THEMES=gruvbox case_cap_cohesion; }
case_cap_cohesion_latte() { COH_THEMES=catppuccin-latte case_cap_cohesion; }
case_cap_cohesion_kanagawa() { COH_THEMES=kanagawa case_cap_cohesion; }
case_cap_listfocus() { COH_THEMES="live gruvbox catppuccin-latte kanagawa" COH_SURFACES="convert openwith" case_cap_cohesion; }
# The kanagawa primary, where the accent equals the foreground: only the dialogs that carry a primary.
case_cap_cohesion_kgprim() { COH_THEMES=kanagawa COH_SURFACES="permissions newfile collide" case_cap_cohesion; }
# The PDF viewer marks, accent line against foreground hairline: the Quick Look and column PDF viewers.
case_cap_cohesion_pdfmarks() { COH_THEMES="live gruvbox catppuccin-latte" COH_SURFACES="ql-pdf cols-pdf" case_cap_cohesion; }
