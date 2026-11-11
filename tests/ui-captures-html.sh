# The rendered HTML board's native specimens, run only by name.
. "$repo/tests/lib/pointer-glide.sh"

cap_html_expect() {
    local column="$1" expression="$2" label="$3" state="" deadline=$((SECONDS + 20))
    while (( SECONDS < deadline )); do
        state=$(ipc htmlState "$column") || fail "HTML: observer failed"
        if jq -e "$expression" <<< "$state" >/dev/null; then
            menus_checks=$((menus_checks + 1))
            printf 'HTML_CHECK %s %s %s\n' "$menus_checks" "$label" "$(jq -c . <<< "$state")"
            return
        fi
        sleep 0.1
    done
    fail "HTML: $label: $state"
}
cap_html_dir=""
cap_html_open() {
    local view="$1" height="${2:-normal}" dir
    dir=$(mktemp -d "$fixture_root/html.XXXXXXXX") || fail "HTML: cannot create specimen sandbox"
    sandbox_require "$dir"
    cap_html_dir=$dir
    : > "$dir/$SANDBOX_MARKER"
    trap cap_html_close EXIT
    python3 "$repo/tests/html-specimen.py" "$dir/page" || fail "HTML: specimen creation failed"
    if [[ "$height" == tall ]]; then
        python3 - "$dir/page/index.html" <<'PY' || fail "HTML: tall specimen creation failed"
from pathlib import Path
import sys

page = Path(sys.argv[1])
page.write_text(page.read_text().replace('body{margin:0;', 'body{min-height:2400px;margin:0;', 1))
PY
    fi
    seed_ui_state "$dir/state" '{"keys":"default","view":"list","display":{"textSize":{"mode":14}}}'
    launch "$dir/page"
    wait_listing 1
    cap_resize 1100 700
    cap_unlock_still
    seek_row_named index.html
    key -k space >/dev/null
    cap_html_expect false '.visible and .ready and .view == "rendered" and .lines == 61 and .count == "61 lines" and .mark == "html" and (.file | length) > 0' "HTML renders with its bar"
    if [[ "$view" == source ]]; then
        key r >/dev/null
        cap_html_expect false '.visible and .ready and .view == "source" and .lines == 61' "HTML Source draws all lines"
    fi
}
cap_html_point() {
    local where="$1" state rect cx cy sx sy sw sh wx wy _ww _wh
    state=$(ipc htmlState false) || fail "HTML: pointer observer failed"
    if [[ "$where" == close ]]; then
        read -r cx cy <<< "$(jq -er '.close.centre' <<< "$state")" || fail "HTML: no close centre"
    else
        if [[ "$where" == lane ]]; then
            rect=$(jq -er '.lane.rect' <<< "$state") || fail "HTML: no lane rectangle"
        else
            rect=$(jq -er '.frame' <<< "$state") || fail "HTML: no card rectangle"
        fi
        [[ "$rect" =~ ^-?[0-9]+\ -?[0-9]+\ [0-9]+\ [0-9]+$ ]] || fail "HTML: invalid $where rectangle [$rect]"
        read -r sx sy sw sh <<< "$rect"
        cx=$((sx + sw / 2)); cy=$((sy + sh / 2))
    fi
    [[ "$cx $cy" =~ ^[0-9]+\ [0-9]+$ ]] || fail "HTML: invalid $where centre [$cx $cy]"
    read -r wx wy _ww _wh < <(window_box) || fail "HTML: native window coordinates unavailable"
    pointer_glide "$((wx + cx))" "$((wy + cy))" || fail "HTML: pointer did not reach $where"
}
cap_html_shot() {
    local column="$1" name="$2" state state_file="$evidence_dir/$2.json"
    state=$(ipc htmlState "$column") || fail "HTML: capture observer failed"
    jq -e '.visible and .ready and .view == "rendered"' <<< "$state" >/dev/null || fail "HTML: $name is not rendered and ready: $state"
    mkdir -p "$evidence_dir"
    [[ ! -e "$state_file" && ! -L "$state_file" ]] || fail "HTML: refusing existing state evidence $state_file"
    printf '%s\n' "$state" > "$state_file"
    printf 'HTML_STATE %s %s\n' "$name" "$(jq -c . <<< "$state")"
    shot "$name"
    python3 "$repo/tests/html-corners.py" "$evidence_dir/$name.png" "$state" || fail "HTML: $name corner pixels failed"
}
cap_html_close() {
    kill_flea
    if [[ -n "$cap_html_dir" ]]; then sandbox_remove "$cap_html_dir"; fi
    cap_html_dir=""
    trap - EXIT
}
case_cap_html() {
    local menus_checks=0
    cap_html_open rendered
    cap_html_shot false ql-html
    cap_html_close
}
case_cap_html_source() {
    local menus_checks=0
    cap_html_open source
    shot ql-html-source
    key -k Escape >/dev/null
    switch_view columns
    cap_html_expect true '.visible and .ready and .view == "rendered"' "the column renders after Quick Look Source closes"
    switch_view list
    seek_row_named index.html
    key -k space >/dev/null
    cap_html_expect false '.visible and .ready and .view == "rendered"' "Quick Look reopens rendered after Source closes"
    key -k Escape >/dev/null
    cap_html_close
}
case_cap_cols_html() {
    local menus_checks=0
    cap_html_open rendered
    key -k Escape >/dev/null
    switch_view columns
    cap_html_expect true '.visible and .ready and .view == "rendered" and .lines == 61 and .facts == "HTML document · 1.6 kB · 61 lines"' "the tall column shows the board facts"
    cap_html_shot true cols-html
    cap_html_close
}
case_cap_html_scrolled() {
    local menus_checks=0 before
    cap_html_open rendered tall
    cap_html_point document
    cap_html_expect false '.visible and .ready and .contentHeight > .viewportHeight and .y == 0' "the tall HTML starts at its top"
    before=$(ipc htmlState false | jq -er '.y') || fail "HTML: no starting scroll offset"
    key j >/dev/null
    cap_html_expect false ".visible and .ready and .y > $before and .lane.visible and .lane.shown" "j scrolls inside the card and reveals its lane"
    cap_html_point lane
    cap_html_expect false ".visible and .ready and .y > $before and .lane.visible and .lane.shown" "the scrolled lane remains visible for its capture"
    cap_html_shot false ql-html-scrolled
    cap_html_close
}
case_cap_cols_html_after_source() {
    local menus_checks=0
    cap_html_open source
    key -k Escape >/dev/null
    switch_view columns
    cap_html_expect true '.visible and .ready and .view == "rendered" and .lines == 61' "the column stays rendered after Quick Look Source closes"
    cap_html_shot true cols-html-after-source
    cap_html_close
}
case_cap_html_reopened() {
    local menus_checks=0
    cap_html_open source
    key -k Escape >/dev/null
    [[ "$(ipc previewOpen)" == false ]] || fail "HTML: Escape did not close Source Quick Look"
    key -k space >/dev/null
    cap_html_expect false '.visible and .ready and .view == "rendered" and .lines == 61' "Space reopens HTML rendered after Source closes"
    cap_html_shot false ql-html-reopened
    cap_html_close
}
case_cap_html_close_hover() {
    local menus_checks=0
    cap_html_open rendered
    cap_html_point close
    cap_html_expect false '.visible and .ready and .close.hovered and (.close.focused | not) and (.close.ring | not) and (.close.pressed | not)' "the HTML close mark is hovered without a keyboard ring"
    cap_html_shot false ql-html-close-hover
    cap_html_close
}
case_cap_html_close_focus() {
    local menus_checks=0
    cap_html_open rendered
    cap_html_point document
    key -k Tab >/dev/null
    cap_html_expect false '.visible and .ready and .close.focused and .close.ring and (.close.hovered | not) and (.close.pressed | not)' "Tab focuses the HTML close mark and draws its keyboard ring"
    cap_html_shot false ql-html-close-focus
    cap_html_close
}

# This controlled native card retains the same image and document while only its refusal changes.
cap_html_clip_shot() {
    local phase="$1" name="$2" state state_file="$evidence_dir/$2.json"
    state=$(ipc htmlState true) || fail "HTML clip: observer failed"
    jq -e '.visible and .ready and .accelerated and .drawn and (.file | length) > 0' <<< "$state" >/dev/null \
        || fail "HTML clip: $phase needs a drawn page on RHI: $state"
    mkdir -p "$evidence_dir"
    [[ ! -e "$state_file" && ! -L "$state_file" ]] || fail "HTML clip: refusing existing evidence $state_file"
    printf '%s\n' "$state" > "$state_file"
    printf 'HTML_STATE %s %s\n' "$name" "$(jq -c . <<< "$state")"
    shot "$name"
    python3 "$repo/tests/html-clip-pixels.py" "$phase" "$evidence_dir/$name.png" "$state" \
        || fail "HTML clip: $phase pixel check failed"
}
cap_html_clip() {
    [[ "$test_frontend" != gpui ]] || { cap_html_clip_gpui "$1"; return; }
    local refusal="$1" menus_checks=0 dir before
    kill_flea
    dir=$(mktemp -d "$fixture_root/html-clip.XXXXXXXX") || fail "HTML clip: cannot create sandbox"
    sandbox_require "$dir"
    cap_html_dir=$dir
    : > "$dir/$SANDBOX_MARKER"
    trap cap_html_close EXIT
    local flea_ui="$dir" HTML_ROOT="$dir"
    export HTML_ROOT
    mkdir -p "$dir/boot"
    cp "$repo/tests/html-clip.qml" "$dir/boot/shell.qml"
    ln -s "$repo/ui" "$dir/boot/flea"
    ln -s "$(readlink -m "$repo/ui/boot/Commons")" "$dir/boot/Commons"
    ln -s "$(readlink -m "$repo/ui/boot/Ui")" "$dir/boot/Ui"
    printf '<body style="margin:0;background:#2345bc;min-height:400px"></body>\n' > "$dir/page.html"
    magick -size 300x400 xc:'#2345bc' "$dir/page.png" || fail "HTML clip: page creation failed"
    seed_ui_state "$dir/state" '{"keys":"default","view":"list"}'
    launch_window "$dir"
    cap_html_expect true '.visible and .ready and .accelerated and .drawn and .clip and .state == "ready" and (.pending | not)' "native clip draws its page on RHI"
    cap_html_clip_shot ready "html-clip-$refusal-before"
    before=$(ipc htmlState true | jq -er '.file') || fail "HTML clip: no drawn image path"
    [[ "$(ipc htmlClipRefuse "$refusal")" == true ]] || fail "HTML clip: $refusal transition refused"
    cap_html_expect true ".visible and .ready and .drawn and .$refusal and (.pending | not)" "$refusal settles after the page was drawn"
    [[ "$(ipc htmlState true | jq -er '.file')" == "$before" ]] || fail "HTML clip: $refusal discarded the old image before the check"
    cap_html_clip_shot "$refusal" "html-clip-$refusal"
    [[ "$(ipc htmlState true | jq -er '.clip | not')" == true ]] || fail "HTML clip: $refusal keeps its texture loader active"
    cap_html_close
}

# The production GPUI component and backend observe genuine changes to their owned HTML input.
cap_html_clip_gpui() {
    local refusal="$1" menus_checks=0 dir before
    local flea_launch_fixture=html-lifecycle
    kill_flea
    dir=$(mktemp -d "$fixture_root/html-clip.XXXXXXXX") || fail "HTML clip: cannot create sandbox"
    sandbox_require "$dir"
    cap_html_dir="$dir"
    : > "$dir/$SANDBOX_MARKER"
    trap cap_html_close EXIT
    printf '<body style="margin:0;background:#2345bc;min-height:400px"></body>\n' > "$dir/page.html"
    launch_window "$dir/page.html"
    cap_resize 380 280
    cap_html_expect true '.visible and .ready and .accelerated and .drawn and .clip and .state == "ready" and (.pending | not)' "native clip draws its real helper output on GPUI"
    cap_html_clip_shot ready "html-clip-$refusal-before"
    before=$(ipc htmlState true | jq -er '.file') || fail "HTML clip: no drawn image path"
    case "$refusal" in
        failed) chmod 000 "$dir/page.html" || fail "HTML clip: input read refusal could not be caused" ;;
        large) truncate -s 1048577 "$dir/page.html" || fail "HTML clip: oversized input could not be caused" ;;
        *) fail "HTML clip: unknown refusal $refusal" ;;
    esac
    cap_html_expect true ".visible and .ready and .drawn and .$refusal and (.pending | not)" "$refusal settles after the page was drawn"
    [[ "$(ipc htmlState true | jq -er '.file')" == "$before" ]] || fail "HTML clip: $refusal discarded the old image before the check"
    cap_html_clip_shot "$refusal" "html-clip-$refusal"
    [[ "$(ipc htmlState true | jq -er '.clip | not')" == true ]] || fail "HTML clip: $refusal keeps image submission active"
    cap_html_close
}
case_cap_html_failed() { cap_html_clip failed; }
case_cap_html_large() { cap_html_clip large; }
