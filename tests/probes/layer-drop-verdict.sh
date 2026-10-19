#!/bin/bash
# Verdict helpers shared by the Bottom-layer probe and headless scan; the probe supplies live readers and the scan stubs them.
. "$(dirname "${BASH_SOURCE[0]}")/../../tools/flea-ipc"

layerdrop_path_attempts=30
layerdrop_path_poll=0.5

# Pids in AFTER not in BEFORE, normalised numeric sort deduped, no edge space.
layerdrop_torn_pids() {
    local before="${1:-}" after="${2:-}" pid
    for pid in $after; do
        case " $before " in *" $pid "*) ;; *) printf '%s\n' "$pid";; esac
    done | sort -n -u | tr '\n' ' ' | sed 's/ *$//'
}

# 0 when the probe's own Bottom panel took the drop, read off its log file.
layerdrop_panel_hit() {
    grep -q PANEL-DROP "${1:-/dev/null}" 2>/dev/null
}

# 0 when HAVE is the lifted folder WANT, both from the same qs ipc path reader.
layerdrop_path_matches() {
    [[ -n "${1:-}" && "${1:-}" == "${2:-}" ]]
}

# 0 when a torn pid resolves to a qs instance on the lifted folder; paths_tsv retains failure diagnostics.
layerdrop_catcher_hit() {
    local torn="$1" lifted_path="$2" pid tid seen attempt
    paths_tsv=""
    [[ -n "$lifted_path" ]] || return 1
    [[ -n "$torn" ]] || return 1
    for pid in $torn; do
        tid=$(layerdrop_qsid "$pid") || continue
        [[ -n "$tid" ]] || continue
        seen=""
        for attempt in $(seq 1 "$layerdrop_path_attempts"); do
            seen=$(flea_ipc_call -i "$tid" call flea path 2>/dev/null || true)
            layerdrop_path_matches "$seen" "$lifted_path" && break
            sleep "$layerdrop_path_poll"
        done
        paths_tsv+="${pid}"$'\t'"${seen}"$'\n'
        if layerdrop_path_matches "$seen" "$lifted_path"; then
            printf 'torn-off window %s took the drop on %s\n' "$pid" "$seen" >&2
            return 0
        fi
    done
    return 1
}
