# Source after the shared flea-ci-visual lane cases to register the plugin SOURCE regression and the AirDrop plugin.
CASES+=("settings-plugin-source-row 1100x700 surface:SettingsPanel")
# F16 (0.4.1): boards 0401-airdrop-menu and 0401-airdrop-page, with a stand-in owly on PATH; the page's two refusals.
CASES+=("airdrop-menu-off 1100x700 surface:IpcMenuState"
        "airdrop-menu-devices 1100x700 surface:IpcMenuState"
        "airdrop-page 1100x700 surface:SettingsPanel"
        "airdrop-page-unsupported 1100x700 surface:SettingsPanel"
        "airdrop-page-missing 1100x700 surface:SettingsPanel")

# Opens Settings, Plugins, then the page of plugin ID, from a launched window.
plugin_page_open() {
    local id=$1 target cursor attempt
    local rail_limit=10 pane_limit=50 poll_seconds=0.1
    key ,
    wait_ipc settingsOpen true || return 1
    [ "$(ipc settingsSide)" = rail ] || key -k Tab
    for attempt in $(seq 1 "$rail_limit"); do
        [ "$(ipc settingsSection)" = plugins ] && break
        key j
    done
    wait_ipc settingsSection plugins || return 1
    key -k Tab
    wait_ipc settingsSide pane || return 1
    for attempt in $(seq 1 "$pane_limit"); do
        # Sample input: [{"id":"example.checksums","kind":"plugin"}]
        target=$(ipc settingsModel | jq -er --arg id "$id" 'map(.id) | index($id)') && break
        sleep "$poll_seconds"
    done
    [ -n "$target" ] && [ "$target" != null ] || return 1
    for attempt in $(seq 1 "$pane_limit"); do
        cursor=$(ipc settingsCursor)
        [ "$cursor" = "$target" ] && break
        if [ "$cursor" -gt "$target" ]; then key -k Up; else key -k Down; fi
    done
    wait_ipc settingsCursor "$target" || return 1
    key -k Return
}

case_settings_plugin_source_row() (
    [ "$VISUAL_FRONTEND" = gpui ] || return 77
    local root origin attempt pane_limit=50 poll_seconds=0.1
    # Under the case fixture like the gh315 cases, so reset_fixture removes it before the next case.
    root=$C/plugin-source
    mkdir -p "$root" || return 1
    : > "$root/.flea-test-sandbox" || return 1
    origin="$root/a long local plugin parent/another long parent/keep the final folder/checksums"
    mkdir -p "$origin" "$root/data" || return 1
    cp -a /src/examples/plugins/checksums/. "$origin/" || return 1
    export XDG_DATA_HOME="$root/data" XDG_STATE_HOME="/tmp/vstate/$case_name"
    "$flea_bin" plugin add "$origin" >> "$case_notes" 2>&1 || return 1
    launch list "$W" || return 1
    plugin_page_open example.checksums || return 1
    for attempt in $(seq 1 "$pane_limit"); do
        # Sample input: [{"kind":"fact","label":"Installed from","value":"/tmp/plugin/checksums"}]
        ipc settingsModel | jq -e --arg origin "$origin" \
            'any(.[]; .kind == "fact" and .label == "Installed from" and .value == $origin)' >/dev/null && break
        sleep "$poll_seconds"
    done
    # Sample input: [{"kind":"fact","label":"Installed from","value":"/tmp/plugin/checksums"}]
    ipc settingsModel | jq -e --arg origin "$origin" \
        'any(.[]; .kind == "fact" and .label == "Installed from" and .value == $origin)' >/dev/null || return 1
    snap
)

# The flyout and the page fade in, so a snap taken sooner catches them half drawn.
airdrop_settle_seconds=0.3

# A stand-in owly first on PATH: MODE ready is an adapter that works, broken one that cannot, none no owly at all.
airdrop_owly() {
    local mode=$1 bin=$C/airdrop/bin
    mkdir -p "$bin" || return 1
    : > "$C/airdrop/.flea-test-sandbox" || return 1
    export LAUNCH_STATE=',"plugins":{"flea.airdrop":{"enabled":true}}'
    if [ "$mode" = none ]; then
        # A host owly would draw a real adapter, so the missing case fails rather than snap the wrong page.
        if command -v owly > /dev/null; then
            echo "airdrop_owly none: owly is already on PATH at $(command -v owly)" >&2
            return 1
        fi
        return 0
    fi
    {
        printf '#!/bin/sh\n'
        printf 'case "$1" in\n'
        if [ "$mode" = ready ]; then
            printf 'doctor) echo %s; exit 0 ;;\n' "'{\"device\":\"8086:2725\",\"driver\":\"iwlwifi\",\"hardware\":true,\"status\":\"tested\",\"missing\":[]}'"
        else
            printf 'doctor) echo %s; exit 1 ;;\n' "'{\"device\":\"8086:24fd\",\"driver\":\"iwlwifi\",\"hardware\":true,\"status\":\"broken\",\"missing\":[]}'"
        fi
        printf 'radio) echo "awdl0 is up: fe80::1%%awdl0 (02:00:00:00:00:01). Ctrl-C takes it down." >&2; exec sleep 600 ;;\n'
        printf 'browse) echo %s; echo %s; echo %s; exec sleep 30 ;;\n' \
            "'3a9f   \"iPhone\" ([fe80::1%7]:8770)'" "'0f1e   \"MacBook Air\" ([fe80::2%7]:8770)'" "'77c2   \"iPad mini\" ([fe80::3%7]:8770)'"
        printf '*) exec sleep 600 ;;\nesac\n'
    } > "$bin/owly" || return 1
    chmod +x "$bin/owly" || return 1
    export PATH="$bin:$PATH"
}

airdrop_menu() {
    cursor_to notes.txt || return 1
    key m
    wait_ipc contextMenuVisible true || return 1
    menu_choose "Send with AirDrop" || return 1
}

case_airdrop_menu_off() (
    [ "$VISUAL_FRONTEND" = gpui ] || return 77
    airdrop_owly ready || return 1
    launch list "$W" || return 1
    airdrop_menu || return 1
    wait_ipc contextMenuSubmenuEntries 'Turn On AirDrop' || return 1
    sleep "$airdrop_settle_seconds"
    snap
)

# Turn On AirDrop closes the menu; the next file menu lists the devices as each answers, phones under the phone mark.
case_airdrop_menu_devices() (
    [ "$VISUAL_FRONTEND" = gpui ] || return 77
    airdrop_owly ready || return 1
    launch list "$W" || return 1
    airdrop_menu || return 1
    wait_ipc contextMenuSubmenuEntries 'Turn On AirDrop' || return 1
    wait_ipc_has menuState '"submenu":true' || return 1
    key j
    wait_ipc_has menuState '"submenuCursor":0' || return 1
    key -k Return
    wait_ipc contextMenuVisible false || return 1
    airdrop_menu || return 1
    wait_ipc contextMenuSubmenuEntries 'iPhone|MacBook Air|iPad mini' || return 1
    sleep "$airdrop_settle_seconds"
    snap
)

airdrop_page() {
    launch list "$W" || return 1
    plugin_page_open flea.airdrop || return 1
    # Sample input: [{"kind":"fact","label":"Wi-Fi adapter","value":"Intel AX210, works with AirDrop"}]
    wait_ipc_has settingsModel "$1" || return 1
    sleep "$airdrop_settle_seconds"
    snap
}

case_airdrop_page() (
    [ "$VISUAL_FRONTEND" = gpui ] || return 77
    airdrop_owly ready || return 1
    airdrop_page 'Intel AX210, works with AirDrop'
)

case_airdrop_page_unsupported() (
    [ "$VISUAL_FRONTEND" = gpui ] || return 77
    airdrop_owly broken || return 1
    airdrop_page 'Intel 8265, cannot do AirDrop'
)

case_airdrop_page_missing() (
    [ "$VISUAL_FRONTEND" = gpui ] || return 77
    airdrop_owly none || return 1
    airdrop_page 'Needs the owly package'
)

# FX-PREVIEW (0.4.2): heads of untyped names (#377), Latin-1, UTF-16 and a UTF-8 byte order mark (S1A-7, S1A-8).
CASES+=("ql-sniff-script 1100x700 ql:code"
        "cols-sniff-notes 1100x700 surface:PreviewColumn"
        "ql-text-latin1 1100x700 ql:text"
        "ql-text-utf16 1100x700 ql:text"
        "ql-markdown-bom 1100x700 surface:IpcPreviewState")

fxpreview_files() {
    local d=$C/FxPreview
    mkdir -p "$d" || return 1
    printf '#!/bin/bash\necho hello from myscript\n' > "$d/myscript" && chmod 755 "$d/myscript" || return 1
    printf 'plain notes with no extension\nsecond line\n' > "$d/notes" || return 1
    printf 'caf\351 na\357ve\n' > "$d/latin1.txt" || return 1
    printf '\377\376h\000e\000l\000l\000o\000 \000u\000t\000f\0001\0006\000\n\000' > "$d/utf16.txt" || return 1
    printf '\357\273\277# bom markdown\n\nbody text\n' > "$d/bom.md" || return 1
    touch -d "$STAMP" "$d"/* || return 1
}

case_ql_sniff_script() ( fxpreview_files && quick_look "$C/FxPreview" myscript )
case_ql_text_latin1() ( fxpreview_files && quick_look "$C/FxPreview" latin1.txt )
case_ql_text_utf16() (
    fxpreview_files && quick_look_open "$C/FxPreview" utf16.txt || return 1
    wait_code_state false '.total == 1' || return 1
    snap
)

case_cols_sniff_notes() (
    fxpreview_files || return 1
    launch columns "$C/FxPreview" || return 1
    cursor_to notes || return 1
    wait_code_state true '.total == 2' || return 1
    snap
)

case_ql_markdown_bom() (
    fxpreview_files || return 1
    quick_look_open "$C/FxPreview" bom.md || return 1
    wait_ipc previewMarkdownView rendered || return 1
    sleep 0.5
    snap
)

# S7B-1 (0.4.2): Columns asks thumbnails at Row density's pitch, so the rows at the end of a long picture folder draw theirs.
CASES+=("cols-density-pictures-end $S view:columns")

case_cols_density_pictures_end() (
    [ "$VISUAL_FRONTEND" = gpui ] || return 77
    local root=$C/pictures last=119 decode_limit=100 poll_seconds=0.1 i attempt
    mkdir -p "$root" || return 1
    : > "$root/.flea-test-sandbox" || return 1
    for i in $(seq -f %03g 0 "$last"); do cp "$W/Media/photo.png" "$root/picture-$i.png" || return 1; done
    LAUNCH_STATE=',"density":"comfortable","display":{"textSize":{"mode":14}}' launch columns "$root" || return 1
    wait_ipc bodyPx 14 || return 1
    key G
    wait_ipc cursor "$last" || return 1
    # Status 1 is a decoded picture; the end row is where an ask at the fixed ROW_HEIGHT falls furthest short.
    for attempt in $(seq 1 "$decode_limit"); do
        [ "$(ipc rowIconStatus "$last")" = 1 ] && break
        sleep "$poll_seconds"
    done
    [ "$(ipc rowIconStatus "$last")" = 1 ] || { state_failure "Columns row $last drew no picture"; return 1; }
    snap
)

# Boards 0401-empty-omi-off, -fixed and -both-off, and 0401-tabs-icons-a: Settings > View's three rows, each off its default.
CASES+=("empty-omi-off 1100x700 state:empty surface:EmptyState"
        "empty-fixed 1100x700 state:empty surface:EmptyState"
        "empty-both-off 1100x700 state:empty surface:EmptyState"
        "tabs-icons 1100x700 surface:TabBar key:tabNew")
case_empty_omi_off() (
    [ "$VISUAL_FRONTEND" = gpui ] || return 77
    LAUNCH_STATE=',"emptyFolderOmi":false' launch list "$W/Empty" || return 1
    snap
)
case_empty_fixed() (
    [ "$VISUAL_FRONTEND" = gpui ] || return 77
    LAUNCH_STATE=',"emptyFolderRotate":false' launch list "$W/Empty" || return 1
    snap
)
case_empty_both_off() (
    [ "$VISUAL_FRONTEND" = gpui ] || return 77
    LAUNCH_STATE=',"emptyFolderOmi":false,"emptyFolderRotate":false' launch list "$W/Empty" || return 1
    snap
)
case_tabs_icons() (
    [ "$VISUAL_FRONTEND" = gpui ] || return 77
    LAUNCH_STATE=',"tabsAsIcons":true' launch list "$W" || return 1
    key t; wait_ipc tabCount 2 || return 1
    key t; wait_ipc tabCount 3 || return 1
    snap
)
# Empty Trash… from the rail's Trash menu reviews over the folder it was chosen from (board delete, PR #419).
CASES+=("rail-empty-trash 1100x700 surface:TrashConfirm")
case_rail_empty_trash() (
    [ "$VISUAL_FRONTEND" = gpui ] || return 77
    launch list "$W" || return 1
    rail_cursor_to Trash || return 1
    key m
    wait_ipc contextMenuVisible true || return 1
    menu_choose "Empty Trash" || return 1
    wait_ipc_has trashState 'Empty Trash?' || return 1
    sleep 0.5
    snap
)

# FX-KEYS (0.4.2, soak S2-6): board 0402-columns-menu-target, the cols-cursorfull state with beta.md right clicked.
CASES+=("cols-menu-target 1100x700 view:columns menu:row")

case_cols_menu_target() (
    [ "$VISUAL_FRONTEND" = gpui ] || return 77
    local total i target=
    launch columns "$W/A" || return 1
    key j
    wait_ipc cursor 1 || return 1
    wait_parent_column || return 1
    total=$(ipc total) && [[ $total =~ ^[0-9]+$ ]] || { state_failure "ipc total answered '$total'"; return 1; }
    for i in $(seq 0 $((total - 1))); do
        [[ "$(ipc visibleRowName "$i")" == beta.md ]] && { target=$i; break; }
    done
    [ -n "$target" ] || { state_failure "no beta.md row in $W/A"; return 1; }
    # Sample input: "412.5 118", the row's window-relative centre.
    click_at "$(ipc rowCentre "$target")" right || return 1
    wait_ipc contextMenuVisible true || return 1
    # The cursor stays on full: Finder rule 5 moves nothing on a right click.
    wait_ipc cursor 1 || return 1
    snap
)

# FX-POINTER (0.4.2): a right press in the path field opens the rename field's text menu, its rows and card (board rename).
CASES+=("text-menu-path 1100x700")

case_text_menu_path() (
    [ "$VISUAL_FRONTEND" = gpui ] || return 77
    launch list "$W" || return 1
    key :
    wait_ipc pathBarOpen true || return 1
    click_at "$(ipc pathCentre)" right || return 1
    snap
)

# FX-POLISH uses the existing Permissions, Jobs, keymap and Trash recipes.
CASES+=("permissions-readonly 1100x700 surface:PermissionsDialog"
        "archive-failed-reason 1100x700 surface:TransferCard"
        "keymap-keep-results 1100x700 surface:KeymapSheet"
        "trash-cursor-rest 1100x700 surface:TrashView"
        "trash-cursor-hover 1100x700 surface:TrashView"
        "trash-cursor-keyboard 1100x700 surface:TrashView"
        "trash-cursor-unfocused 1100x700 surface:TrashView")

fxpolish_files() {
    python3 /src/ci/visual/polish_fixture.py "$C/FxPolish"
}

case_permissions_readonly() (
    [ "$VISUAL_FRONTEND" = gpui ] || return 77
    fxpolish_files || return 1
    menu_dialog_open "$C/FxPolish" special.txt 'Permissions…' || return 1
    wait_ipc_has permissionsState 'No changes to apply' || return 1
    wait_ipc_has permissionsState 'Current mode 4644' || return 1
    snap
)

case_archive_failed_reason() (
    [ "$VISUAL_FRONTEND" = gpui ] || return 77
    fxpolish_files || return 1
    menu_dialog_open "$C/FxPolish" unsafe.zip Extract || return 1
    wait_ipc_has statusActivityState 'An entry in this archive would land outside its folder, so nothing was extracted.' || return 1
    snap
)

case_keymap_keep_results() (
    [ "$VISUAL_FRONTEND" = gpui ] || return 77
    launch list "$W" || return 1
    key '?'
    wait_ipc keymapSheetOpen true || return 1
    key keep
    wait_ipc_has keymapSheetResults 'Keep results' || return 1
    snap
)

fxpolish_trash() {
    [ "$VISUAL_FRONTEND" = gpui ] || return 77
    open_trash || return 1
    key -k Escape
    wait_ipc focusStop list || return 1
    park_pointer
}

case_trash_cursor_keyboard() ( fxpolish_trash && snap )
case_trash_cursor_rest() (
    fxpolish_trash || return $?
    key -k Tab
    wait_ipc focusStop rail || return 1
    snap
)
case_trash_cursor_unfocused() ( case_trash_cursor_rest )
case_trash_cursor_hover() (
    local x y ax ay
    fxpolish_trash || return $?
    key -k Tab
    wait_ipc focusStop rail || return 1
    read -r x y <<< "$(ipc trashRowCentre 0)"
    [ -n "$x" ] && [ -n "$y" ] || return 1
    read -r ax ay <<< "$(win_json | jq -r '.at | "\(.[0]) \(.[1])"')"
    omarchy-drive move "$(awk -v a="$ax" -v b="$x" 'BEGIN {printf "%.0f", a+b}')" "$(awk -v a="$ay" -v b="$y" 'BEGIN {printf "%.0f", a+b}')" > /dev/null || return 1
    nudge_pointer 3 || return 1
    snap
)
