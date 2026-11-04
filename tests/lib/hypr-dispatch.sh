#!/usr/bin/env bash
# Only these typed functions build compositor commands; every window uses its validated address.
_hypr_refuse() {
    printf '%s: refused argument %q (%s)\n' "$1" "$2" "$3" >&2
    return 1
}

# Sample input: _hypr_arity hypr_window_resize 'ADDR W H' 0xabc 800 480.
_hypr_arity() {
    local helper="$1" signature="$2"
    local -a expected
    shift 2
    read -r -a expected <<< "$signature"
    [[ $# == ${#expected[@]} ]] || _hypr_refuse "$helper" "$*" "expected $signature"
}

# Sample input: hypr_window_focus 0xAbC, validated before adding the address: prefix.
_hypr_address() {
    [[ "$2" =~ ^0x[[:xdigit:]]+$ ]] || _hypr_refuse "$1" "$2" 'expected 0x followed by hex digits'
}

# Sample input: hypr_window_move -1920, signed coordinates allow monitors left of or above the origin.
_hypr_coordinate() {
    [[ "$2" =~ ^-?[0-9]+$ ]] || _hypr_refuse "$1" "$2" 'expected an integer coordinate'
}

# Sample input: hypr_window_resize 800, dimensions are nonnegative because they describe an extent.
_hypr_extent() {
    [[ "$2" =~ ^[0-9]+$ ]] || _hypr_refuse "$1" "$2" 'expected a nonnegative integer extent'
}

# Sample input: _hypr_reply_ok 1 'window not found' 0xabc 800 480, called by the typed helper it reports on.
_hypr_reply_ok() {
    local status="$1" answer="$2"
    shift 2
    if [[ "$status" == 0 && "$answer" == ok ]]; then
        return 0
    fi
    printf '%s %s: the compositor answered "%s" (exit %s)\n' "${FUNCNAME[1]}" "$*" "$answer" "$status" >&2
    return 1
}

# Sample input: hypr_workspace_focus 21, the current Lua workspace focus with a validated id.
hypr_workspace_focus() {
    _hypr_arity hypr_workspace_focus 'ID' "$@" || return 1
    _hypr_coordinate hypr_workspace_focus "$1" || return 1
    local answer status=0
    answer=$(hyprctl dispatch "hl.dsp.focus({ workspace = \"$1\" })" 2>&1) || status=$?
    _hypr_reply_ok "$status" "$answer" "$@"
}

hypr_window_focus() {
    _hypr_arity hypr_window_focus 'ADDR' "$@" || return 1
    _hypr_address hypr_window_focus "$1" || return 1
    local answer status=0
    answer=$(hyprctl dispatch "hl.dsp.focus({ window = \"address:$1\" })" 2>&1) || status=$?
    _hypr_reply_ok "$status" "$answer" "$@"
}

hypr_window_float() {
    _hypr_arity hypr_window_float 'ADDR on|off' "$@" || return 1
    _hypr_address hypr_window_float "$1" || return 1
    [[ "$2" == on || "$2" == off ]] || {
        _hypr_refuse hypr_window_float "$2" 'expected on or off'
        return 1
    }
    local answer status=0
    answer=$(hyprctl dispatch "hl.dsp.window.float({ action = \"$2\", window = \"address:$1\" })" 2>&1) || status=$?
    _hypr_reply_ok "$status" "$answer" "$@"
}

hypr_window_resize() {
    _hypr_arity hypr_window_resize 'ADDR W H' "$@" || return 1
    _hypr_address hypr_window_resize "$1" || return 1
    _hypr_extent hypr_window_resize "$2" || return 1
    _hypr_extent hypr_window_resize "$3" || return 1
    local answer status=0
    answer=$(hyprctl dispatch "hl.dsp.window.resize({ x = $2, y = $3, exact = true, window = \"address:$1\" })" 2>&1) || status=$?
    _hypr_reply_ok "$status" "$answer" "$@"
}

hypr_window_move() {
    _hypr_arity hypr_window_move 'ADDR X Y' "$@" || return 1
    _hypr_address hypr_window_move "$1" || return 1
    _hypr_coordinate hypr_window_move "$2" || return 1
    _hypr_coordinate hypr_window_move "$3" || return 1
    local answer status=0
    answer=$(hyprctl dispatch "hl.dsp.window.move({ x = $2, y = $3, window = \"address:$1\" })" 2>&1) || status=$?
    _hypr_reply_ok "$status" "$answer" "$@"
}

# Sample input: hypr_window_resize_absolute 0xabc 800 480, sends relative = false where hypr_window_resize sends exact = true.
hypr_window_resize_absolute() {
    _hypr_arity hypr_window_resize_absolute 'ADDR W H' "$@" || return 1
    _hypr_address hypr_window_resize_absolute "$1" || return 1
    _hypr_extent hypr_window_resize_absolute "$2" || return 1
    _hypr_extent hypr_window_resize_absolute "$3" || return 1
    local answer status=0
    answer=$(hyprctl dispatch "hl.dsp.window.resize({ x = $2, y = $3, relative = false, window = \"address:$1\" })" 2>&1) || status=$?
    _hypr_reply_ok "$status" "$answer" "$@"
}

# Sample input: hypr_window_move_absolute 0xabc 40 -20, sends relative = false where hypr_window_move sends no relative key.
hypr_window_move_absolute() {
    _hypr_arity hypr_window_move_absolute 'ADDR X Y' "$@" || return 1
    _hypr_address hypr_window_move_absolute "$1" || return 1
    _hypr_coordinate hypr_window_move_absolute "$2" || return 1
    _hypr_coordinate hypr_window_move_absolute "$3" || return 1
    local answer status=0
    answer=$(hyprctl dispatch "hl.dsp.window.move({ x = $2, y = $3, relative = false, window = \"address:$1\" })" 2>&1) || status=$?
    _hypr_reply_ok "$status" "$answer" "$@"
}

# Sample input: hypr_window_opacity 0xabc opacity_inactive 0.96, with only fixture opacity properties supported.
hypr_window_opacity() {
    _hypr_arity hypr_window_opacity 'ADDR PROP VALUE' "$@" || return 1
    _hypr_address hypr_window_opacity "$1" || return 1
    case "$2" in
        opacity|opacity_inactive)
            [[ "$3" =~ ^(1|0\.[0-9]+)$ ]] || {
                _hypr_refuse hypr_window_opacity "$3" 'expected 1 or a decimal between 0 and 1'
                return 1
            } ;;
        opacity_override|opacity_inactive_override)
            [[ "$3" == 1 ]] || {
                _hypr_refuse hypr_window_opacity "$3" 'expected an enabled opacity override'
                return 1
            } ;;
        *) _hypr_refuse hypr_window_opacity "$2" 'expected a supported opacity property'; return 1 ;;
    esac
    local answer status=0
    answer=$(hyprctl dispatch "hl.dsp.window.set_prop({window = \"address:$1\", prop = \"$2\", value = \"$3\"})" 2>&1) || status=$?
    _hypr_reply_ok "$status" "$answer" "$@"
}

hypr_cursor_move() {
    _hypr_arity hypr_cursor_move 'X Y' "$@" || return 1
    _hypr_coordinate hypr_cursor_move "$1" || return 1
    _hypr_coordinate hypr_cursor_move "$2" || return 1
    local answer status=0
    answer=$(hyprctl dispatch "hl.dsp.cursor.move({x = $1, y = $2})" 2>&1) || status=$?
    _hypr_reply_ok "$status" "$answer" "$@"
}

# Sample input: hypr_session_exec_script /home/flea-sandbox/iso/session-mount.ABCD/run.sh, never a shell expression.
hypr_session_exec_script() {
    _hypr_arity hypr_session_exec_script 'SCRIPT' "$@" || return 1
    [[ "$1" == /* && "$1" != *[!a-zA-Z0-9_./=-]* ]] || {
        _hypr_refuse hypr_session_exec_script "$1" 'expected an absolute script path without shell metacharacters'
        return 1
    }
    local answer status=0
    answer=$(hyprctl dispatch "hl.dsp.exec_cmd(\"/bin/bash '$1'\")" 2>&1) || status=$?
    _hypr_reply_ok "$status" "$answer" "$@"
}

# Sample input: hypr_session_exec_rules name:tui-proof FLEA-tui-proof /tmp/case/launch.sh, the script runs under window rules.
# The workspace rule is not silent: a silent one leaves the output on its first workspace, so its screencopy never shows the window.
hypr_session_exec_rules() {
    _hypr_arity hypr_session_exec_rules 'WORKSPACE OUTPUT SCRIPT' "$@" || return 1
    _hypr_workspace hypr_session_exec_rules "$1" || return 1
    _hypr_output_name hypr_session_exec_rules "$2" || return 1
    [[ "$3" == /* && "$3" != *[!a-zA-Z0-9_./=-]* ]] || {
        _hypr_refuse hypr_session_exec_rules "$3" 'expected an absolute script path without shell metacharacters'
        return 1
    }
    local answer status=0
    answer=$(hyprctl dispatch "hl.dsp.exec_cmd(\"/bin/bash '$3'\", { workspace = \"$1\", monitor = \"$2\", float = true })" 2>&1) || status=$?
    _hypr_reply_ok "$status" "$answer" "$@"
}

_hypr_output_name() {
    [[ "$2" =~ ^FLEA-[A-Za-z0-9._-]+$ ]] || _hypr_refuse "$1" "$2" 'expected an owned FLEA- output name'
}

_hypr_mode() {
    [[ "$2" =~ ^[0-9]+x[0-9]+@[0-9]+$ ]] || _hypr_refuse "$1" "$2" 'expected WIDTHxHEIGHT@HZ'
}

_hypr_option_path() {
    [[ "$2" =~ ^[a-z_]+(:[a-z_]+)+$ ]] || _hypr_refuse "$1" "$2" 'expected a Hyprland option path such as decoration:rounding'
}

_hypr_mods() {
    [[ -z "$2" || "$2" =~ ^(CTRL|SHIFT|ALT|SUPER)( (CTRL|SHIFT|ALT|SUPER))*$ ]] || _hypr_refuse "$1" "$2" 'expected modifier names separated by spaces'
}

_hypr_key_name() {
    [[ "$2" =~ ^[A-Za-z0-9_]+$ ]] || _hypr_refuse "$1" "$2" 'expected a key name without separators'
}

_hypr_workspace() {
    [[ "$2" =~ ^[A-Za-z0-9_:.-]+$ ]] || _hypr_refuse "$1" "$2" 'expected a workspace name or number without quotes, spaces or shell metacharacters'
}

hypr_window_send_shortcut() {
    _hypr_arity hypr_window_send_shortcut 'ADDR MODS KEY' "$@" || return 1
    _hypr_address hypr_window_send_shortcut "$1" || return 1
    _hypr_mods hypr_window_send_shortcut "$2" || return 1
    _hypr_key_name hypr_window_send_shortcut "$3" || return 1
    local answer status=0
    answer=$(hyprctl dispatch "hl.dsp.send_shortcut({ mods = \"$2\", key = \"$3\", window = \"address:$1\" })" 2>&1) || status=$?
    _hypr_reply_ok "$status" "$answer" "$@"
}

hypr_window_rounding() {
    _hypr_arity hypr_window_rounding 'ADDR N' "$@" || return 1
    _hypr_address hypr_window_rounding "$1" || return 1
    _hypr_extent hypr_window_rounding "$2" || return 1
    local answer status=0
    answer=$(hyprctl dispatch "hl.dsp.window.set_prop({window = \"address:$1\", prop = \"rounding\", value = \"$2\"})" 2>&1) || status=$?
    _hypr_reply_ok "$status" "$answer" "$@"
}

hypr_output_create() {
    _hypr_arity hypr_output_create 'NAME' "$@" || return 1
    _hypr_output_name hypr_output_create "$1" || return 1
    local answer status=0
    answer=$(hyprctl output create headless "$1" 2>&1) || status=$?
    _hypr_reply_ok "$status" "$answer" "$@"
}

hypr_output_remove() {
    _hypr_arity hypr_output_remove 'NAME' "$@" || return 1
    _hypr_output_name hypr_output_remove "$1" || return 1
    local answer status=0
    answer=$(hyprctl output remove "$1" 2>&1) || status=$?
    _hypr_reply_ok "$status" "$answer" "$@"
}

# Sample input: hypr_monitor_mode FLEA-ab12 2560x1440@60, sets scale 1 at the automatic position.
hypr_monitor_mode() {
    _hypr_arity hypr_monitor_mode 'NAME MODE' "$@" || return 1
    _hypr_output_name hypr_monitor_mode "$1" || return 1
    _hypr_mode hypr_monitor_mode "$2" || return 1
    local answer status=0
    answer=$(hyprctl eval "hl.monitor({ output = \"$1\", mode = \"$2\", position = \"auto\", scale = 1 })" 2>&1) || status=$?
    _hypr_reply_ok "$status" "$answer" "$@"
}

# Queries print the compositor's answer and never send a command, so they skip the reply check.
hypr_query_monitors() {
    _hypr_arity hypr_query_monitors '' "$@" || return 1
    hyprctl -j monitors all
}

hypr_query_clients() {
    _hypr_arity hypr_query_clients '' "$@" || return 1
    hyprctl -j clients
}

hypr_query_cursor() {
    _hypr_arity hypr_query_cursor '' "$@" || return 1
    hyprctl -j cursorpos
}

hypr_query_option() {
    _hypr_arity hypr_query_option 'NAME' "$@" || return 1
    _hypr_option_path hypr_query_option "$1" || return 1
    hyprctl -j getoption "$1"
}

hypr_query_version() {
    _hypr_arity hypr_query_version '' "$@" || return 1
    hyprctl version
}

hypr_query_help() {
    _hypr_arity hypr_query_help '' "$@" || return 1
    hyprctl --help
}

hypr_query_output_help() {
    _hypr_arity hypr_query_output_help '' "$@" || return 1
    hyprctl output --help
}

# Sample input: bash tests/lib/hypr-dispatch.sh window_resize 0xabc 800 480.
_hypr_program() {
    local operation="${1-}"
    [[ $# != 0 ]] || _hypr_refuse hypr-dispatch.sh "$operation" 'expected a typed operation' || return 1
    shift
    case "$operation" in
        workspace_focus) hypr_workspace_focus "$@" ;;
        window_focus) hypr_window_focus "$@" ;;
        window_float) hypr_window_float "$@" ;;
        window_resize) hypr_window_resize "$@" ;;
        window_move) hypr_window_move "$@" ;;
        window_resize_absolute) hypr_window_resize_absolute "$@" ;;
        window_move_absolute) hypr_window_move_absolute "$@" ;;
        window_opacity) hypr_window_opacity "$@" ;;
        cursor_move) hypr_cursor_move "$@" ;;
        session_exec_script) hypr_session_exec_script "$@" ;;
        session_exec_rules) hypr_session_exec_rules "$@" ;;
        window_send_shortcut) hypr_window_send_shortcut "$@" ;;
        window_rounding) hypr_window_rounding "$@" ;;
        output_create) hypr_output_create "$@" ;;
        output_remove) hypr_output_remove "$@" ;;
        monitor_mode) hypr_monitor_mode "$@" ;;
        query_monitors) hypr_query_monitors "$@" ;;
        query_clients) hypr_query_clients "$@" ;;
        query_cursor) hypr_query_cursor "$@" ;;
        query_option) hypr_query_option "$@" ;;
        query_version) hypr_query_version "$@" ;;
        query_help) hypr_query_help "$@" ;;
        query_output_help) hypr_query_output_help "$@" ;;
        *) _hypr_refuse hypr-dispatch.sh "$operation" 'unknown typed operation' ;;
    esac || return 1
    [[ "$operation" == query_* ]] || printf '%s\n' ok
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
    _hypr_program "$@"
fi
