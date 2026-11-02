# Source after the shared flea-ci-visual lane cases to register the plugin SOURCE regression.
CASES+=("settings-plugin-source-row 1100x700 surface:SettingsPanel")

case_settings_plugin_source_row() (
    [ "$VISUAL_FRONTEND" = gpui ] || return 77
    local root origin target cursor attempt
    local rail_limit=10 pane_limit=50 poll_seconds=0.1
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
        target=$(ipc settingsModel | jq -er 'map(.id) | index("example.checksums")') && break
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
