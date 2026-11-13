# Filtered Grid arrows use the row width drawn at text size 14, including the key that commits the query.
CASES+=("grid-filter-row-down 800x600 view:grid")
CASES+=("list-filter-ranked-shift 450x1000 view:list key:extendDown key:extendUp"
        "grid-filter-ranked-shift 450x1000 view:grid key:extendDown key:extendUp")

case_grid_filter_row_down() (
    [ "$VISUAL_FRONTEND" = gpui ] || return 77
    local root=$C/grid-filter-row-down index name columns
    mkdir -p "$root" || return 1
    : > "$root/.flea-test-sandbox" || return 1
    for index in {1..9}; do
        printf -v name 'file-%02d.txt' "$index"
        printf 'grid filter %s\n' "$index" > "$root/$name" || return 1
    done
    printf 'nonmatching row\n' > "$root/other.txt" || return 1
    export LAUNCH_STATE=',"keys":"default","display":{"textSize":{"mode":14}},"preview":{"thumbnails":"off"}'
    launch grid "$root" || return 1
    key /file-0 || return 1
    wait_ipc drawnCount 9 || return 1
    # Flea maps at 1100x700; the arrows are driven at the shot size so the drawn cursor matches the asserted index.
    local launch_columns
    launch_columns=$(ipc gridColumns) || return 1
    size_window 800 600 || return 1
    for _i in $(seq 1 60); do
        columns=$(ipc gridColumns) && [ "$columns" != "$launch_columns" ] && break
        [ "$_i" = 60 ] && { echo "VISUAL $case_name gridColumns stayed $launch_columns at 800x600" | tee -a "$case_notes"; return 1; }
        sleep 0.1
    done
    [[ $columns =~ ^[0-9]+$ ]] && (( columns > 1 && columns < 9 )) || return 1
    key -k Down || return 1
    wait_ipc cursor "$columns" || return 1
    key -k Up || return 1
    wait_ipc cursor 0 || return 1
    key -k Home || return 1
    key -k Down || return 1
    wait_ipc cursor "$columns" || return 1
    snap
)

ranked_filter_shift() (
    [ "$VISUAL_FRONTEND" = gpui ] || return 77
    local view=$1 root=$C/$case_name name
    mkdir -p "$root" || return 1
    : > "$root/.flea-test-sandbox" || return 1
    for name in a-n-t-s.txt b-n--t--s.txt c-n---t---s.txt z-nt-s.txt; do
        printf 'ranked filter\n' > "$root/$name" || return 1
    done
    export LAUNCH_STATE=',"keys":"default","display":{"textSize":{"mode":14}},"preview":{"thumbnails":"off"}'
    launch "$view" "$root" || return 1
    key /nts || return 1
    wait_ipc drawnCount 4 || return 1
    size_window 450 1000 || return 1
    if [ "$view" = grid ]; then wait_ipc gridColumns 1 || return 1; fi
    key -k Up || return 1
    key -k Home || return 1
    wait_ipc cursor 3 || return 1
    key -M shift -k Down -m shift || return 1
    wait_ipc cursor 0 || return 1
    wait_ipc selectedIndices '0,3' || return 1
    key -M shift -k Down -m shift || return 1
    wait_ipc cursor 1 || return 1
    wait_ipc selectedIndices '0,1,3' || return 1
    key -M shift -k Up -m shift || return 1
    wait_ipc cursor 0 || return 1
    wait_ipc selectedIndices '0,3' || return 1
    snap
)

case_list_filter_ranked_shift() ( ranked_filter_shift list )
case_grid_filter_ranked_shift() ( ranked_filter_shift grid )
