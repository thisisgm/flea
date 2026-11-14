# FX-WORDING: packaged builds use the existing release sentence, and signed OPR keeps its check state.
CASES+=("settings-about-local-package 1100x700 surface:SettingsPanel"
        "settings-about-opr-package 1100x700 surface:SettingsPanel")

wording_about_package() {
    local signed=$1 expected=$2 bin=$C/wording/bin attempt rows poll_limit=50 poll_seconds=0.1
    mkdir -p "$bin" || return 1
    export VISUAL_WORDING_SIGNATURE=$signed
    printf '%s\n' '#!/bin/sh' 'case "$1" in' \
        '-Qqo) echo flea ;;' \
        '-Qi) printf "Name : flea\\nVersion : 0.4.2-2\\nValidated By : %s\\n" "$VISUAL_WORDING_SIGNATURE" ;;' \
        '*) exit 1 ;;' 'esac' > "$bin/pacman" || return 1
    printf '%s\n' '#!/bin/sh' 'exit 2' > "$bin/checkupdates" || return 1
    chmod +x "$bin/pacman" "$bin/checkupdates" || return 1
    export PATH="$bin:$PATH"
    settings_section about || return 1
    for attempt in $(seq 1 "$poll_limit"); do
        rows=$(ipc settingsModel) || { state_failure "settingsModel did not answer"; return 1; }
        # Sample input: [{"id":"updateFlea","value":"Release build"}]
        jq -e --arg expected "$expected" 'any(.[]; .id == "updateFlea" and .value == $expected)' <<< "$rows" > /dev/null && break
        sleep "$poll_seconds"
    done
    jq -e --arg expected "$expected" 'any(.[]; .id == "updateFlea" and .value == $expected)' <<< "$rows" > /dev/null \
        || { state_failure "Update Flea never read '$expected'; last rows: $rows"; return 1; }
    if [ "$signed" = None ]; then
        jq -e 'any(.[]; .kind == "hint" and .label == "Omarchy update installs newer releases.")' <<< "$rows" > /dev/null \
            || { state_failure "the packaged update note is missing; last rows: $rows"; return 1; }
    fi
    snap
}

case_settings_about_local_package() ( [ "$VISUAL_FRONTEND" = gpui ] || return 77; wording_about_package None 'Release build' )
case_settings_about_opr_package() ( [ "$VISUAL_FRONTEND" = gpui ] || return 77; wording_about_package Signature 'Up to date · 0.4.2' )
