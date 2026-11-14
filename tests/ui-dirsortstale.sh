# Defines the native stale-dirsize race case; tests/ui.sh supplies the guarded fixture and IPC helpers.

dirsortstale_wait_size() {
    local want="$1" state=""
    for _attempt in $(seq 1 300); do
        state=$(ipc dirSizeState 2>/dev/null || true)
        if jq -e --argjson want "$want" '.file["0"].bytes == $want' <<<"$state" >/dev/null 2>&1; then
            printf '%s' "$state"
            return 0
        fi
        sleep 0.05
    done
    printf 'dirSizeState did not reach bytes=%s; last=%s\n' "$want" "$state" >&2
    return 1
}

dirsortstale_cleanup() {
    [[ -n "${dirsortstale_test_ui:-}" ]] || return 0
    flea_ui="$dirsortstale_test_ui"
    flea_bin="$dirsortstale_candidate_bin"
    kill_flea
    flea_ui="$dirsortstale_saved_ui"
    flea_bin="$dirsortstale_saved_bin"
}

# Hidden files last: the same six-entry folder at four depths, named in the order each view must draw it; name_order puts a dot before a letter, and a reversed sort reverses the hidden files too (.hidden .zeta becomes .zeta .hidden).
hiddenlast_on='Alpha|beta|gamma|.config|.hidden|.zeta'
hiddenlast_on_desc='Alpha|gamma|beta|.config|.zeta|.hidden'
hiddenlast_off='.config|Alpha|.hidden|.zeta|beta|gamma'

# The six rows a view draws, '|' joined; a row not drawn yet reads empty.
hiddenlast_rows() {
    local row names=""
    for row in 0 1 2 3 4 5; do names+="${names:+|}$(ipc visibleRowName "$row" 2>/dev/null || true)"; done
    printf '%s' "$names"
}

# The chooser's six rows, read through its own IPC target.
hiddenlast_picker_rows() {
    local row names=""
    for row in 0 1 2 3 4 5; do names+="${names:+|}$(flea_ipc_call --pid "$1" call fleapicker rowAt "$row" 2>/dev/null || true)"; done
    printf '%s' "$names"
}

# Waits up to 15 s for the chooser to report its sort reversed with rows that differ from the ones drawn before the key, so no read lands on the old order.
hiddenlast_picker_reversed() {
    local pid="$1" before="$2" seen="" deadline=$((SECONDS + 15))
    while (( SECONDS < deadline )); do
        seen=$(flea_ipc_call --pid "$pid" call fleapicker snapshot 2>/dev/null | jq -r 'select(.sortDesc == true and .state == "ready") | [.rows[:6][].n] | join("|")' 2>/dev/null || true)
        [[ -n "$seen" && "$seen" != "$before" ]] && return 0
        sleep 0.05
    done
    fail "hiddenlast: the chooser never reversed its sort and redrew its rows, last read [$seen]"
}

# Waits up to 15 s for a reader's output to equal the wanted order, so a relisting view is read once it settles.
hiddenlast_expect() {
    local label="$1" want="$2" seen="" deadline=$((SECONDS + 15))
    shift 2
    while (( SECONDS < deadline )); do
        seen=$("$@" 2>/dev/null || true)
        if [[ "$seen" == "$want" ]]; then
            printf 'HIDDENLAST %s %s\n' "$label" "$seen"
            return 0
        fi
        sleep 0.05
    done
    fail "hiddenlast: $label drew [$seen], wanted [$want]"
}

hiddenlast_view() {
    local view="$1" seen=""
    click_chrome "$view"
    for _attempt in $(seq 1 100); do
        seen=$(ipc viewMode 2>/dev/null || true)
        [[ "$seen" == "$view" ]] && return 0
        sleep 0.05
    done
    fail "hiddenlast: the chrome did not switch to $view, it shows $seen"
}

# Four columns need a columns area of COUNT_4_AT px (read from ui/js/Columns.js), so the window grows by the shortfall the app measures.
hiddenlast_four_columns() {
    local ax ay aw ah wx wy ww wh count="" count4_at
    # Sample input, the Columns.js line: "var COUNT_4_AT = 1700".
    count4_at=$(sed -n 's/^var COUNT_4_AT = \([0-9][0-9]*\)$/\1/p' "$repo/ui/js/Columns.js")
    [[ "$count4_at" =~ ^[0-9]+$ ]] || fail "hiddenlast: COUNT_4_AT not found in ui/js/Columns.js"
    # ipc listAreaRect: "12 42 880 620", the columns area when the columns view is up.
    read -r ax ay aw ah <<< "$(ipc listAreaRect)"
    [[ "$aw" =~ ^[0-9]+$ ]] || fail "hiddenlast: listAreaRect answered '$ax $ay $aw $ah'"
    if (( aw < count4_at )); then
        read -r wx wy ww wh < <(window_box) || fail "hiddenlast: native window coordinates unavailable"
        cap_resize $((ww + count4_at - aw)) "$wh"
    fi
    for _attempt in $(seq 1 100); do
        count=$(ipc columnCount 2>/dev/null || true)
        [[ "$count" =~ ^[0-9]+$ ]] && (( count >= 4 )) && return 0
        sleep 0.05
    done
    fail "hiddenlast: the window shows $count columns, two ancestors need 4"
}

# Four nested copies of one folder (.config and Alpha dirs, .hidden, .zeta, beta and gamma files), so every ancestor and the peeked child hold the same six names.
hiddenlast_fixture() {
    local level="$1" depth
    for depth in 0 1 2 3; do
        mkdir -p "$level/.config" "$level/Alpha"
        : > "$level/.hidden"
        : > "$level/.zeta"
        : > "$level/beta"
        : > "$level/gamma"
        level="$level/Alpha"
    done
}

case_hiddenlast() {
    local dir="$fixture_root/hiddenlast" state="$fixture_root/hiddenlast-state" patch
    local l0="$dir" l1="$dir/Alpha" l2="$dir/Alpha/Alpha" l3="$dir/Alpha/Alpha/Alpha"
    local title="Hidden last chooser $$" reply="$fixture_root/hiddenlast-reply.json" picker_pid="" shot_png menus_checks=0
    sandbox_scratch "$dir"
    hiddenlast_fixture "$dir"
    patch='{"hidden":true,"hiddenLast":true,"rememberSort":false,"columnsLimit":4,"sort":{"key":"name","reverse":false},"view":"list"}'
    seed_ui_state "$state" "$patch"
    launch "$l2"
    wait_listing 6

    hiddenlast_expect list "$hiddenlast_on" hiddenlast_rows
    shot hiddenlast-list

    hiddenlast_view columns
    hiddenlast_four_columns
    hiddenlast_expect columns-active "$hiddenlast_on" hiddenlast_rows
    hiddenlast_expect columns-parent "$hiddenlast_on" ipc columnNames parent
    hiddenlast_expect columns-grandparent "$hiddenlast_on" ipc columnPeekNames "$l0"
    hiddenlast_expect columns-child "$hiddenlast_on" ipc columnNames child
    shot hiddenlast-columns

    hiddenlast_view grid
    hiddenlast_expect grid "$hiddenlast_on" hiddenlast_rows
    shot hiddenlast-grid

    # Ctrl+Return on Alpha opens the deepest copy in a second tab.
    hiddenlast_view list
    seek_row_named Alpha
    key -M ctrl -k Return -m ctrl >/dev/null || fail "hiddenlast: Ctrl+Return was rejected"
    wait_tabs 2 "Ctrl+Return on Alpha"
    wait_path "$l3"
    hiddenlast_expect tab "$hiddenlast_on" hiddenlast_rows
    shot hiddenlast-tab

    # The setting goes off in the running window: the plain order returns in the tab, the first tab and a column, with no relaunch.
    env XDG_STATE_HOME="$state" "$flea_bin" --ui-state '{"hiddenLast":false}' >/dev/null || fail "hiddenlast: the state write failed"
    hiddenlast_expect tab-off "$hiddenlast_off" hiddenlast_rows
    click_tab 0
    wait_path "$l2"
    hiddenlast_expect list-off "$hiddenlast_off" hiddenlast_rows
    hiddenlast_view columns
    hiddenlast_expect columns-parent-off "$hiddenlast_off" ipc columnNames parent
    shot hiddenlast-off

    # Both dual panes, each from its own listing.
    kill_flea
    seed_ui_state "$state-dual" "$(jq -cn --arg l "$l1" --arg r "$l2" '{hidden:true,hiddenLast:true,rememberSort:false,sort:{key:"name",reverse:false},view:"dual",dual:{paths:[$l,$r],focus:0}}')"
    launch "$l1"
    menus_expect dualState '.active and .focused == 0 and all(.panes[]; .total == 6 and (.loading | not))' 'both dual listings landed'
    hiddenlast_expect dual-left "$hiddenlast_on" hiddenlast_rows
    key -k Tab >/dev/null
    menus_expect dualState '.focused == 1' 'the second pane has the keys'
    hiddenlast_expect dual-right "$hiddenlast_on" hiddenlast_rows
    shot hiddenlast-dual

    # The chooser: its list, then its header sort, which used to forget the setting.
    kill_flea
    FLEA_UI="$flea_ui" FLEA_BIN="$flea_bin" FLEA_PICKER="$(jq -cn --arg f "$l2" --arg t "$title" '{mode:"open",folder:$f,title:$t}')" FLEA_PICKER_REPLY="$reply" \
        flea_test_exec qs -p "$flea_ui/boot/picker.qml" >"$fixture_root/hiddenlast-picker.log" 2>&1 </dev/null &
    for _attempt in $(seq 1 300); do
        omarchy-drive windows --json | jq -e --arg title "$title" '[.windows[] | select(.title == $title)] | length == 1' >/dev/null && break
        sleep 0.05
    done
    omarchy-drive focus "$title" >/dev/null || fail "hiddenlast: the chooser never opened"
    for _attempt in $(seq 1 100); do
        picker_pid=$(flea_pids | head -1)
        [[ -n "$picker_pid" ]] && break
        sleep 0.05
    done
    [[ -n "$picker_pid" ]] || fail "hiddenlast: no chooser process"
    omarchy-drive key --window "$title" . >/dev/null || fail "hiddenlast: the chooser would not show hidden files"
    hiddenlast_expect chooser "$hiddenlast_on" hiddenlast_picker_rows "$picker_pid"
    shot_png="$evidence_dir/hiddenlast-chooser.png"
    mkdir -p "$evidence_dir"
    [[ ! -e "$shot_png" && ! -L "$shot_png" ]] || fail "hiddenlast: refusing existing evidence $shot_png"
    omarchy-drive shot "$shot_png" "$title" >/dev/null || fail "hiddenlast: the chooser shot failed"
    printf 'SHOT %s\n' "$shot_png"
    omarchy-drive key --window "$title" S >/dev/null || fail "hiddenlast: the chooser would not reverse its sort"
    hiddenlast_picker_reversed "$picker_pid" "$hiddenlast_on"
    hiddenlast_expect chooser-sorted "$hiddenlast_on_desc" hiddenlast_picker_rows "$picker_pid"
    shot_png="$evidence_dir/hiddenlast-chooser-sorted.png"
    [[ ! -e "$shot_png" && ! -L "$shot_png" ]] || fail "hiddenlast: refusing existing evidence $shot_png"
    omarchy-drive shot "$shot_png" "$title" >/dev/null || fail "hiddenlast: the sorted chooser shot failed"
    printf 'SHOT %s\n' "$shot_png"
    kill_flea
    printf 'HIDDENLAST list=ok columns=ok grid=ok tab=ok off=ok dual=ok chooser=ok\n'
}
