# Filtered Grid arrows use the row width drawn at text size 14, including the key that commits the query.
CASES+=("grid-filter-row-down 800x600 view:grid")

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
    columns=$(ipc gridColumns)
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
