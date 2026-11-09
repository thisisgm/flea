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
