# Native material candidates with a fixture foot beside Flea; sourced by tests/ui.sh, never a headless suite.

glasspick_client() {
    local pid="$1" end=$((SECONDS + 20)) clients client
    [[ "$pid" =~ ^[1-9][0-9]*$ ]] || fail "glasspick: invalid owned pid $pid"
    while (( SECONDS < end )); do
        flea_process_owned "$pid" || fail "glasspick: cannot inspect owned process $pid"
        clients=$(hyprctl clients -j) || fail "glasspick: cannot read native clients"
        client=$(jq -ec --argjson pid "$pid" '[.[] | select(.pid == $pid)] | select(length == 1) | .[0]' <<< "$clients") || client=""
        if [[ -n "$client" ]]; then printf '%s\n' "$client"; return; fi
        sleep 0.1
    done
    fail "glasspick: owned process $pid mapped no unique native window"
}

glasspick_address() {
    local address
    address=$(glasspick_client "$1" | jq -er '.address') || fail "glasspick: no owned address for $1"
    [[ "$address" =~ ^0x[0-9a-fA-F]+$ ]] || fail "glasspick: invalid address $address"
    printf '%s\n' "$address"
}

# Properties are limited to owned fixture windows, with numeric values and the Lua dispatcher used by Omarchy dev.
glasspick_opacity() {
    local pid="$1" active="$2" inactive="$3" address prop value
    address=$(glasspick_address "$pid") || return 1
    [[ "$active" =~ ^(1|0\.[0-9]+)$ && "$inactive" =~ ^(1|0\.[0-9]+)$ ]] || fail "glasspick: invalid fixture opacity"
    for prop in opacity opacity_inactive opacity_override opacity_inactive_override; do
        case "$prop" in
            opacity) value="$active" ;;
            opacity_inactive) value="$inactive" ;;
            *) value=1 ;;
        esac
        flea_process_owned "$pid" || fail "glasspick: fixture $pid lost ownership before opacity"
        hypr_window_opacity "$address" "$prop" "$value" || fail "glasspick: cannot set $prop on owned fixture $pid"
    done
}

glasspick_place() {
    local pid="$1" x="$2" y="$3" width="$4" height="$5" address client end=$((SECONDS + 20))
    address=$(glasspick_address "$pid") || return 1
    hypr_window_float "$address" on || fail "glasspick: cannot float owned window $address"
    hypr_window_resize "$address" "$width" "$height" || fail "glasspick: cannot size owned window $address"
    hypr_window_move_absolute "$address" "$x" "$y" || fail "glasspick: cannot move owned window $address"
    while (( SECONDS < end )); do
        client=$(glasspick_client "$pid") || return 1
        jq -e --argjson x "$x" --argjson y "$y" --argjson w "$width" --argjson h "$height" \
            '.floating and .at == [$x, $y] and .size == [$w, $h]' <<< "$client" >/dev/null && return
        sleep 0.1
    done
    fail "glasspick: $address did not reach its capture geometry"
}

glasspick_stop() {
    local pid="$1" ownership end=$((SECONDS + 10))
    [[ -n "$pid" ]] || return 0
    if flea_process_owned "$pid"; then
        kill "$pid" || return 1
    else
        ownership=$?
        [[ "$ownership" == 2 ]] && return 0
        printf 'FAIL glasspick: refusing to stop unowned or unreadable fixture %s\n' "$pid" >&2
        return 1
    fi
    while (( SECONDS < end )); do
        if flea_process_owned "$pid"; then
            sleep 0.1
        else
            ownership=$?
            [[ "$ownership" == 2 ]] || return 1
            wait "$pid" 2>/dev/null || true
            return 0
        fi
    done
    printf 'FAIL glasspick: owned fixture %s survived teardown\n' "$pid" >&2
    return 1
}

glasspick_cleanup() {
    local status=$? clients
    ( kill_flea ) || status=1
    glasspick_stop "$glasspick_term_pid" || status=1
    glasspick_stop "$glasspick_back_pid" || status=1
    if [[ -n "$glasspick_old_focus" ]]; then
        clients=$(hyprctl clients -j) || status=1
        if jq -e --arg address "$glasspick_old_focus" 'any(.[]; .address == $address)' <<< "${clients:-[]}" >/dev/null; then
            hypr_window_focus "$glasspick_old_focus" || status=1
        fi
    fi
    exit "$status"
}

glasspick_focus() {
    local pid="$1" focused="$2" mode="$3" address state end=$((SECONDS + 15))
    address=$(glasspick_address "$pid") || return 1
    hypr_window_focus "$address" || fail "glasspick: cannot focus owned capture window"
    while (( SECONDS < end )); do
        state=$(ipc glass) || fail "glasspick: no material receipt"
        jq -e --argjson focused "$focused" --arg mode "$mode" '.automatic and .focused == $focused and .mode == $mode' <<< "$state" >/dev/null && return
        sleep 0.1
    done
    fail "glasspick: mode/focus did not reach $mode/$focused: $state"
}

glasspick_foot_log() {
    local log="$1" result
    [[ -f "$log" && -r "$log" ]] || fail "glasspick: Foot log is unavailable: $log"
    # Sample input: "foot.ini:3: [colors]: invalid section name: colors".
    if LC_ALL=C grep -Ein '(^|[[:space:]])(err(or)?|fatal):|foot[.]ini:[0-9]+:|invalid section name|failed to (parse|load|read).*config' "$log" >&2; then
        fail "glasspick: Foot config/startup diagnostics refuse capture: $log"
    else
        result=$?
        [[ "$result" == 1 ]] || fail "glasspick: cannot inspect Foot log: $log"
    fi
}

glasspick_foot_check() {
    local fixture_home="$1" config="$2" log="$2/foot-check.log" result=0
    env HOME="$fixture_home" XDG_CONFIG_HOME="$fixture_home/.config" XDG_DATA_HOME="$fixture_home/.local/share" \
        foot --check-config --config="$config/foot.ini" > "$log" 2>&1 || result=$?
    if [[ "$result" != 0 ]]; then
        cat "$log" >&2
        fail "glasspick: Foot config check failed (exit $result): $log"
    fi
    glasspick_foot_log "$log"
    printf 'GLASSPICK foot-config=ok\n'
}

glasspick_shot() {
    local name="$1" fleapid="$2" termpid="$3" a b x y right bottom width height pad=12 png="$evidence_dir/$1.png"
    a=$(glasspick_client "$fleapid") && b=$(glasspick_client "$termpid") || fail "glasspick: pair geometry unavailable"
    x=$(jq -nr --argjson a "$a" --argjson b "$b" '[$a.at[0], $b.at[0]] | min')
    y=$(jq -nr --argjson a "$a" --argjson b "$b" '[$a.at[1], $b.at[1]] | min')
    right=$(jq -nr --argjson a "$a" --argjson b "$b" '[$a.at[0]+$a.size[0], $b.at[0]+$b.size[0]] | max')
    bottom=$(jq -nr --argjson a "$a" --argjson b "$b" '[$a.at[1]+$a.size[1], $b.at[1]+$b.size[1]] | max')
    x=$((x - pad)); y=$((y - pad))
    width=$((right - x + pad)); height=$((bottom - y + pad))
    [[ ! -e "$png" && ! -L "$png" ]] || fail "glasspick: refusing existing evidence $png"
    grim -g "$x,$y ${width}x${height}" "$png" || fail "glasspick: pair capture failed"
    [[ -s "$png" ]] || fail "glasspick: capture is empty"
    ipc glass > "$evidence_dir/$name.json" || fail "glasspick: material sidecar failed"
    printf 'SHOT %s\n' "$png"
    glasspick_shots=$((glasspick_shots + 1))
}

# EXIT runs inside this subshell while the case's local fixture state is still in scope.
case_cap_glasspick() (
    command -v foot >/dev/null || fail "glasspick: foot is required for the native comparison"
    command -v grim >/dev/null || fail "glasspick: grim is required for the owned pair crop"
    command -v magick >/dev/null || fail "glasspick: magick is required for the image fixture"
    local capture_root glasspick_term_pid="" glasspick_back_pid="" glasspick_old_focus glasspick_shots=0
    local theme backdrop mode fixture_home dir state config theme_dir live_name low=kanagawa source
    local bx by client address fleapid terminal_alpha real_foreground="$real_foreground" real_accent="$real_accent"
    local ipc_pid=""
    local -a flea_launch_wrap
    glasspick_old_focus=$(hyprctl activewindow -j | jq -r '.address // empty') || fail "glasspick: cannot retain previous focus"
    [[ -z "$glasspick_old_focus" || "$glasspick_old_focus" =~ ^0x[0-9a-fA-F]+$ ]] || fail "glasspick: invalid previous focus"
    capture_root=$(mktemp -d "$fixture_root/glasspick.XXXXXXXX") || fail "glasspick: cannot create capture root"
    [[ "$capture_root" == /*/* ]] || fail "glasspick: capture root must be absolute and non-empty"
    : > "$capture_root/$SANDBOX_MARKER"
    trap glasspick_cleanup EXIT
    mkdir -p "$evidence_dir" "$capture_root/folder/Alpha" "$capture_root/folder/Beta"
    dir="$capture_root/folder"
    printf 'Material comparison\n' > "$dir/notes.txt"
    magick -size 640x400 gradient:'#7aa2f7-#e0af68' "$dir/preview.png" || fail "glasspick: cannot draw image fixture"
    live_name=$(cat "$real_state_dir/theme.name") || fail "glasspick: no current theme name"
    [[ "$live_name" =~ ^[a-zA-Z0-9._-]+$ ]] || fail "glasspick: current theme has no safe evidence name"
    [[ "$live_name" != kanagawa ]] || low=vantablack
    printf 'GLASSPICK current=%s low-chroma=%s terminal=fixture-foot backdrops=owned-checkers\n' "$live_name" "$low"
    hyprctl -j getoption decoration:blur:enabled > "$evidence_dir/glasspick-blur.json" || fail "glasspick: blur state unavailable"
    jq -e '.bool == true or .int == 1' "$evidence_dir/glasspick-blur.json" >/dev/null || fail "glasspick: compositor must already have Glassy blur enabled"
    for theme in "$live_name" "$low"; do
        fixture_home="$capture_root/home-$theme"
        fixture_home_make "$fixture_home"
        theme_dir="$fixture_home/.local/state/omarchy/current"
        if [[ "$theme" != "$live_name" ]]; then
            source="/usr/share/omarchy/themes/$theme"
            cp "$source/colors.toml" "$theme_dir/theme/colors.toml" || fail "glasspick: missing $theme palette"
            if [[ -f "$source/shell.toml" ]]; then cp "$source/shell.toml" "$theme_dir/theme/shell.toml"; else : > "$theme_dir/theme/shell.toml"; fi
            printf '%s\n' "$theme" > "$theme_dir/theme.name"
        fi
        mkdir -p "$fixture_home/.config/omarchy" "$fixture_home/.local/share"
        printf '[font]\nbase-size = 14\n' > "$fixture_home/.config/omarchy/shell.toml"
        # The pick compares the default mode materials, so only the fixture copy loses a theme alpha override.
        python3 -B - "$theme_dir/theme/shell.toml" <<'PY' || fail "glasspick: cannot prepare default mode inputs"
from pathlib import Path
import re, sys
path = Path(sys.argv[1])
assert path.is_absolute() and path.is_file() and not path.is_symlink()
section, kept = '', []
for line in path.read_text().splitlines(True):
    header = re.match(r'\s*\[([^]]+)\]', line)
    if header:
        section = header.group(1).strip()
    key = re.match(r'\s*([A-Za-z0-9_.-]+)\s*=', line)
    if key and (section == '' and key.group(1) == 'flea.background-alpha' or section == 'flea' and key.group(1) == 'background-alpha'):
        continue
    kept.append(line)
path.write_text(''.join(kept))
PY
        real_foreground=$(grep -E '^foreground' "$theme_dir/theme/colors.toml" | grep -oE '#[0-9A-Fa-f]{6}')
        real_accent=$(grep -E '^accent' "$theme_dir/theme/colors.toml" | grep -oE '#[0-9A-Fa-f]{6}')
        flea_launch_wrap=(env HOME="$fixture_home" XDG_CONFIG_HOME="$fixture_home/.config" XDG_DATA_HOME="$fixture_home/.local/share")
        for backdrop in bright dark; do
            config="$capture_root/back-$theme-$backdrop"
            mkdir -p "$config"
            cp "$repo/tests/glasspick-backdrop.qml" "$config/shell.qml"
            env GLASSPICK_BACKDROP="$backdrop" HOME="$fixture_home" XDG_CONFIG_HOME="$fixture_home/.config" \
                XDG_CACHE_HOME="$capture_root/cache-back-$theme-$backdrop" qs -p "$config" > "$config/qs.log" 2>&1 &
            glasspick_back_pid=$!
            address=$(glasspick_address "$glasspick_back_pid")
            hypr_window_float "$address" on && hypr_window_resize "$address" 1584 740 \
                && omarchy-drive window center "$address" >/dev/null || fail "glasspick: backdrop could not be placed"
            glasspick_opacity "$glasspick_back_pid" 1 1
            client=$(glasspick_client "$glasspick_back_pid")
            read -r bx by < <(jq -r '.at | @tsv' <<< "$client")
            for mode in opaque transparent glassy; do
                state="$capture_root/state-$theme-$backdrop-$mode"
                seed_ui_state "$state" '{"keys":"default","view":"columns","places":{"autoHide":false},"preview":{"column":true},"updates":{"autoCheck":false}}'
                mkdir -p "$state/omarchy/toggles/hypr"
                [[ "$mode" == transparent ]] || printf 'return\n' > "$state/omarchy/toggles/hypr/window-$mode.lua"
                config="$capture_root/term-$theme-$backdrop-$mode"
                mkdir -p "$config"
                terminal_alpha=1
                [[ "$mode" != glassy ]] || terminal_alpha=0.65
                printf '[main]\nfont=monospace:size=11\n[colors-dark]\nforeground=%s\nbackground=%s\nalpha=%s\n' \
                    "${real_foreground#\#}" "$(grep -E '^background[[:space:]]*=' "$theme_dir/theme/colors.toml" | grep -oE '#[0-9A-Fa-f]{6}' | cut -c2-)" "$terminal_alpha" > "$config/foot.ini"
                glasspick_foot_check "$fixture_home" "$config"
                printf '#!/bin/sh\nprintf "Flea material comparison\\n\\nAlpha/\\nBeta/\\nnotes.txt\\npreview.png\\n\\n"\nread -r answer\n' > "$config/view.sh"
                env HOME="$fixture_home" XDG_CONFIG_HOME="$fixture_home/.config" XDG_DATA_HOME="$fixture_home/.local/share" \
                    foot --config="$config/foot.ini" --app-id=com.thisisgm.flea.glasspick.terminal /bin/sh "$config/view.sh" > "$config/foot.log" 2>&1 &
                glasspick_term_pid=$!
                glasspick_place "$glasspick_term_pid" "$((bx + 1144))" "$((by + 20))" 420 700
                if [[ "$mode" == transparent ]]; then glasspick_opacity "$glasspick_term_pid" 0.985 0.96
                else glasspick_opacity "$glasspick_term_pid" 1 1; fi
                launch "$dir"
                fleapid=$(flea_pid)
                ipc_pid="$fleapid"
                wait_listing 4
                glasspick_place "$fleapid" "$((bx + 20))" "$((by + 20))" 1100 700
                [[ "$(ipc metrics)" == '13 12 14 37' && "$(ipc viewMode)" == columns ]] || fail "glasspick: not Columns at text size 14"
                seek_row_named preview.png
                matrix_wait_column image preview.png
                printf 'GLASSPICK owned-keys=ok preview=preview.png\n'
                glasspick_focus "$fleapid" true "$mode"
                [[ "$mode" != transparent ]] || jq -e '.alpha == 1' <<< "$(ipc glass)" >/dev/null || fail "glasspick: focused Transparent is not opaque"
                settle
                glasspick_foot_log "$config/foot.log"
                glasspick_shot "glass-$mode-$theme-$backdrop" "$fleapid" "$glasspick_term_pid"
                if [[ "$mode" == transparent ]]; then
                    glasspick_focus "$glasspick_term_pid" false "$mode"
                    jq -e '.alpha == 1' <<< "$(ipc glass)" >/dev/null || fail "glasspick: unfocused Transparent is not opaque"
                    settle
                    glasspick_foot_log "$config/foot.log"
                    glasspick_shot "glass-$mode-$theme-$backdrop-unfocused" "$fleapid" "$glasspick_term_pid"
                fi
                kill_flea
                ipc_pid=""
                glasspick_stop "$glasspick_term_pid" || fail "glasspick: terminal could not stop"
                glasspick_term_pid=""
            done
            glasspick_stop "$glasspick_back_pid" || fail "glasspick: backdrop could not stop"
            glasspick_back_pid=""
        done
    done
    [[ "$glasspick_shots" == 16 ]] || fail "glasspick: incomplete capture matrix"
    printf 'GLASSPICK shots=%s\n' "$glasspick_shots"
)
