# A first name search stays in this folder; content search and a prior Home scope do not change the next open.
CASES+=("search-root-folder 1100x700 key:search")

# A settled search gets this many reads, this far apart, to show its root: a 10 s ceiling.
search_root_attempts=100
search_root_interval=0.1

search_root_expect() {
    local path=$1 count=$2 state got_path="" got_total="" attempt
    for attempt in $(seq 1 "$search_root_attempts"); do
        state=$(ipc keyDeliveryState) || { state_failure "search root: keyDeliveryState read failed"; return 1; }
        if jq -e '.searchQuery == "searchroot451" and (.searchRunning | not)' <<< "$state" >/dev/null; then
            got_path=$(ipc path) && got_total=$(ipc total) || { state_failure "search root: path or total read failed"; return 1; }
            [ "$got_path" = "$path" ] && [ "$got_total" = "$count" ] && return 0
        fi
        sleep "$search_root_interval"
    done
    state_failure "search root wants $path with $count rows, last settled read $got_path with $got_total rows: $state"
    return 1
}

case_search_root_folder() (
    [ "$VISUAL_FRONTEND" = gpui ] || return 77
    local original_home=$HOME input
    export HOME=$C/search-root
    mkdir -p "$HOME/Here" "$HOME/Elsewhere" || return 1
    : > "$HOME/.flea-test-sandbox"
    for input in .local/state/omarchy/current/theme/{colors.toml,shell.toml,icons.theme} \
        .local/state/omarchy/current/theme.name .config/omarchy/shell.toml \
        .config/fontconfig/{fonts.conf,conf.d/50-omarchy-input-method.conf}; do
        if [ -f "$original_home/$input" ]; then
            mkdir -p "$(dirname -- "$HOME/$input")" || return 1
            cp -L "$original_home/$input" "$HOME/$input" || return 1
        fi
    done
    printf 'searchroot451\n' > "$HOME/Here/searchroot451.txt"
    # A non-matching file keeps the plain Here listing (2 rows) apart from a Folder-scoped result (1 row).
    printf 'decoy\n' > "$HOME/Here/decoy.txt"
    printf 'searchroot451\n' > "$HOME/Elsewhere/searchroot451.txt"
    launch list "$HOME/Here" || return 1
    key f || return 1
    key searchroot451 || return 1
    search_root_expect "$HOME/Here" 1 || return 1
    wait_ipc visibleRowName searchroot451.txt || return 1
    key -k Escape || return 1
    wait_ipc searchMode '' || return 1
    key -M ctrl g -m ctrl || return 1
    key searchroot451 || return 1
    search_root_expect "$HOME/Here" 1 || return 1
    key -k Escape || return 1
    wait_ipc searchMode '' || return 1
    key f || return 1
    key searchroot451 || return 1
    search_root_expect "$HOME/Here" 1 || return 1
    key -k Tab || return 1
    search_root_expect "$HOME" 2 || return 1
    key -k Escape || return 1
    wait_ipc searchMode '' || return 1
    key f || return 1
    key searchroot451 || return 1
    search_root_expect "$HOME/Here" 1 || return 1
    wait_ipc visibleRowName searchroot451.txt || return 1
    snap
)
