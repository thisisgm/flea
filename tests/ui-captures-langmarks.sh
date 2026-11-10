#!/usr/bin/env bash
# Native language-mark specimens and their drawn-state receipts, sourced by ui.sh.

cap_langmarks_ui() {
    [[ "$test_frontend" != gpui ]] || { printf '%s\n' "$repo/ui"; return; }
    local target="$fixture_root/cap-langmarks-ui"
    sandbox_scratch "$target"
    cp -a "$repo/ui/." "$target/" || fail "langmarks: the UI fixture could not be copied"
    cp "$repo/tests/langmark-count.js" "$target/LangmarkCapture.js" || fail "langmarks: the reader could not be copied"
    python3 - "$target/Ipc.qml" <<'PY' || fail "langmarks: the capture reader could not be installed"
from pathlib import Path
import sys
path = Path(sys.argv[1])
source = path.read_text()
anchor = '        function rowRect(i: int): string'
assert source.count(anchor) == 1, "rowRect reader anchor changed"
reader = '''        function langmarksState(): string {
            var rows = []
            for (var i = 0; i < root.pane.total; i++) {
                var row = root.pane.visibleItemFor(i)
                if (row && row.row) rows.push(LangCapture.state(row))
            }
            return JSON.stringify({ view: root.pane.viewMode, cursor: root.pane.cursorIndex, rows: rows })
        }
        function langmarksCode(column: bool): string {
            return JSON.stringify(LangCapture.codeState(root.pane, column, Text.StyledText))
        }
'''
path.write_text('import "LangmarkCapture.js" as LangCapture\n' + source.replace(anchor, reader + anchor))
PY
    printf '%s\n' "$target"
}

# Geometry, ink, cut alpha and fallback are read from the same delegates the shot shows.
cap_langmarks_shot() {
    local name="$1" pixels="$2" state
    state=$(ipc langmarksState) || fail "langmarks: the drawn rows could not be read"
    jq -e --argjson pixels "$pixels" '
        (.rows | length) == 12 and
        ([.rows[] | select(.built)] | length) == 10 and
        ([.rows[] | select(.name == "main.rs")][0] | .cursor and .slug == "rust" and .built) and
        ([.rows[] | select(.name == "notes.ini")][0] | .slug == "" and (.built | not) and .clip == "scissors" and (.alpha - 0.55 | fabs) < 0.01) and
        ([.rows[] | select(.name == "src")][0] | .slug == "" and (.built | not)) and
        all(.rows[] | select(.built); .glyphVisible == false and .ink == .glyphInk and .markSize == $pixels and .page == "M4 7V2h10l6 6v14H17 M14 2v6h6") and
        (if .view == "grid" then true else ([.rows[].labelX] | unique | length) == 1 end)
    ' <<< "$state" >/dev/null || fail "langmarks: $name violates the ink, slot, cut or fallback recipe (mark=$pixels): $state"
    printf '%s\n' "$state" > "$evidence_dir/$name.json"
    shot "$name"
    printf 'CAP_LANGMARKS shot=%s rows=12 cursor=main.rs ink=glyph slot=shared mark=%s cut=0.55 fallback=notes.ini,src\n' "$name" "$pixels"
}

# Both the production readiness reader and the drawn token colours must name the requested file.
cap_langmarks_code() {
    local column="$1" file="$2" slug="$3" state ready end=$((SECONDS + 25))
    while (( SECONDS < end )); do
        ready=$(ipc previewTextState "$column") || fail "langmarks: code readiness could not be read"
        state=$(ipc langmarksCode "$column") || fail "langmarks: drawn code could not be read"
        if jq -e --arg file "$file" '.ready and .visible and (.path | endswith("/" + $file))' <<< "$ready" >/dev/null \
            && jq -e --arg file "$file" --arg slug "$slug" --argjson column "$column" '
                .answer == "styled" and .firstScreen and (.path | endswith("/" + $file)) and
                (.bodies | length) > 0 and (.colors | length) > 0 and
                (. as $state | any(.colors[]; . != $state.inks[0])) and
                (. as $state | all(.colors[]; . as $color | ($state.inks | index($color)) != null)) and
                (if $column then true else .language.slug == $slug and .language.color == .language.slotInk
                    and .language.size == .language.slotSize and (.language.glyphVisible | not) end)
            ' <<< "$state" >/dev/null; then
            jq -cn --argjson state "$state" --argjson ready "$ready" '$state + {ready: $ready.ready}'
            return 0
        fi
        sleep 0.1
    done
    fail "langmarks: $file has no settled coloured code or language bar: $ready $state"
}

cap_langmarks_code_receipt() {
    local column="$1" file="$2" code="$3"
    printf 'CAP_LANGMARKS_CODE column=%s file=%s ready=%s styled=%s colors=%s slug=%s\n' \
        "$column" "$file" "$(jq -r '.ready' <<< "$code")" "$(jq -r '.answer' <<< "$code")" \
        "$(jq -r '.colors | length' <<< "$code")" "$(jq -r '.language.slug // ""' <<< "$code")"
}

# GPUI opens the background menu from a measured empty point below the rows.
cap_langmarks_background() {
    [[ "$test_frontend" == gpui ]] || { click_background; return; }
    local x y width height cx cy wx wy ww wh landed frame
    frame=$(ipc listAreaRect) || fail "langmarks: list bounds could not be read"
    [[ "$frame" =~ ^-?[0-9]+\ -?[0-9]+\ [0-9]+\ [0-9]+$ ]] || fail "langmarks: invalid list bounds: $frame"
    read -r x y width height <<< "$frame"
    (( width > 0 && height > 0 )) || fail "langmarks: the list has no visible area"
    cx=$((x + width / 2))
    cy=$((y + height - 1))
    landed=$(list_row_at_y "$cy")
    [[ -z "$landed" ]] || fail "langmarks: the background point lands on row $landed"
    read -r wx wy ww wh < <(window_box) || fail "langmarks: native window coordinates unavailable"
    omarchy-drive click "$((cx + wx))" "$((cy + wy))" right >/dev/null || fail "langmarks: background right click failed"
}

# The launch set's quiet listing, menu row at rest and lit, and the same listing after activation.
cap_langmarks_toggle_shots() {
    local pixels="$1" before state menu show_label="Show code icons"
    # The native menu follows HIG APL-MENU-04 title case; QML retains its original label.
    [[ "$test_frontend" != gpui ]] || show_label="Show Code Icons"
    before=$(ipc listRequests) || fail "langmarks: listing counter could not be read"
    cap_langmarks_background
    settle
    menu_seek "Hide code icons"
    key -k Return >/dev/null
    settle
    state=$(ipc langmarksState) || fail "langmarks: off-state rows could not be read"
    jq -e '(.rows | length) == 12 and all(.rows[]; (.built | not) and .glyphVisible and .slug == "")' \
        <<< "$state" >/dev/null || fail "langmarks: disabling code icons left a mark: $state"
    printf '%s\n' "$state" > "$evidence_dir/langmarks-list-off.json"
    cap_gm_move 550 4
    settle
    shot langmarks-list-off
    cap_langmarks_background
    settle
    menu=$(ipc menuState) || fail "langmarks: off-state menu could not be read"
    jq -e --arg label "$show_label" '.entries as $rows | ($rows | map(.action) | index("toggleLangMarks")) as $code |
        ($rows | map(.action) | index("toggleHidden")) as $hidden |
        $code == $hidden + 1 and $rows[$code].label == $label and $rows[$code].glyph == "code"' \
        <<< "$menu" >/dev/null || fail "langmarks: Show code icons is missing or misplaced: $menu"
    printf '%s\n' "$menu" > "$evidence_dir/langmarks-menu-off.json"
    cap_gm_move 550 4
    settle
    shot langmarks-menu-off-rest
    menu_seek "Show code icons"
    shot langmarks-menu-off-lit
    key -k Return >/dev/null
    settle
    cap_gm_move 550 4
    settle
    cap_langmarks_shot langmarks-list-on "$pixels"
    [[ "$(ipc listRequests)" == "$before" ]] || fail "langmarks: the menu toggle re-listed the directory"
    printf 'CAP_LANGMARKS_TOGGLE off=generic on=language menu=Show-code-icons listing-requests=%s\n' "$before"
}

# List specimen files, Medium and Large Grid, and Columns, all at native 1x with main.rs under the cursor.
case_cap_langmarks() {
    local dir="$fixture_root/cap-langmarks" name view size main monitor thumb_size saved_ui="$flea_ui" mark_pixels code slug menus_checks=0
    command -v hyprctl >/dev/null || fail "langmarks: Hyprland is required for the 1x check"
    sandbox_scratch "$dir"
    mkdir -p "$dir/src" || fail "langmarks: src could not be created"
    for name in main.rs gui.rs Cargo.toml build.sh README.md flake.nix index.ts app.tsx style.scss data.json notes.ini; do
        printf '%s specimen\n' "$name" > "$dir/$name" || fail "langmarks: $name could not be created"
    done
    python3 - "$dir" <<'PY' || fail "langmarks: coloured-code specimens could not be seeded"
from pathlib import Path
import sys
root = Path(sys.argv[1])
(root / 'main.rs').write_text('fn main() {\n    let answer: i32 = 42;\n    let message = "Hello, Flea";\n    println!("{} {}", message, answer);\n}\n')
(root / 'index.ts').write_text('interface Note { name: string; count: number; }\nconst note: Note = { name: "Flea", count: 42 };\nconsole.log(note.name, note.count);\n')
(root / 'build.sh').write_text('#!/usr/bin/env bash\nset -euo pipefail\nname="Flea"\nprintf "Building %s\\n" "$name"\n')
PY
    # Modified descending preserves the board's specimen order; directories still come first.
    python3 - "$dir" <<'PY' || fail "langmarks: specimen times could not be seeded"
import os
from pathlib import Path
import sys
root = Path(sys.argv[1])
names = ['src', 'main.rs', 'gui.rs', 'Cargo.toml', 'build.sh', 'README.md', 'flake.nix', 'index.ts', 'app.tsx', 'style.scss', 'data.json', 'notes.ini']
for index, name in enumerate(names):
    stamp = 1790524800 - index * 86400
    os.utime(root / name, (stamp, stamp))
PY
    flea_ui=$(cap_langmarks_ui)
    mkdir -p "$evidence_dir" || fail "langmarks: evidence directory could not be created"
    for size in list medium large columns; do
        view="$size"
        [[ "$size" == medium || "$size" == large ]] && view=grid
        thumb_size=medium
        [[ "$size" == medium || "$size" == large ]] && thumb_size="$size"
        seed_ui_state "$fixture_root/cap-langmarks-state-$size" \
            '{"langMarks":true,"keys":"default","view":"'"$view"'","columnsLimit":3,"sort":{"key":"date","reverse":true},"preview":{"thumbSize":"'"$thumb_size"'","thumbnails":"off","column":'"$([[ "$size" == columns ]] && printf true || printf false)"'}}'
        launch "$dir"
        wait_listing 12
        cap_resize 1100 760
        monitor=$(hyprctl activewindow -j | jq -er '.monitor') || fail "langmarks: the window has no monitor"
        hyprctl monitors -j | jq -e --argjson monitor "$monitor" 'any(.[]; .id == $monitor and .scale == 1)' >/dev/null \
            || fail "langmarks: the window must be captured at 1x"
        [[ "$(ipc viewMode)" == "$view" ]] || fail "langmarks: the requested $view view did not land"
        printf 'CAP_LANGMARKS_TEXT defaults=omarchy baseSize=%s body=%s bodySmall=%s caption=%s\n' \
            "$(token_of baseSize)" "$(ipc bodyPx)" "$(token_of bodySmall)" "$(token_of caption)"
        cap_seek_named notes.ini
        key -M ctrl x -m ctrl >/dev/null
        menus_expect keyDeliveryState '(.clipboard.paths | length) == 1 and .clipboard.cut' "langmarks: notes.ini is cut"
        cap_seek_named main.rs
        main=$(ipc cursor)
        [[ "$(ipc visibleRowName "$main")" == main.rs ]] || fail "langmarks: main.rs does not hold the cursor"
        # Park the pointer in the chrome, so no resting mark borrows hover ink.
        cap_gm_move 550 4
        settle
        # Sample input: "markSize=19" -> "19"; other token lines contribute nothing.
        mark_pixels=$(ipc tokens | sed -n 's/^markSize=//p')
        case "$size" in medium) mark_pixels=64 ;; large) mark_pixels=96 ;; esac
        if [[ "$size" == columns ]]; then
            code=$(cap_langmarks_code true main.rs rust) || fail "langmarks: columns code capture failed"
            printf '%s\n' "$code" > "$evidence_dir/langmarks-columns-code.json"
            cap_langmarks_code_receipt true main.rs "$code"
        fi
        cap_langmarks_shot "langmarks-$size" "$mark_pixels"
        [[ "$size" != list ]] || cap_langmarks_toggle_shots "$mark_pixels"
        if [[ "$size" == columns ]]; then
            for name in main.rs index.ts build.sh; do
                case "$name" in main.rs) slug=rust ;; index.ts) slug=typescript ;; build.sh) slug=gnubash ;; esac
                cap_seek_named "$name"
                key -k space >/dev/null
                code=$(cap_langmarks_code false "$name" "$slug") || fail "langmarks: Quick Look $name code capture failed"
                printf '%s\n' "$code" > "$evidence_dir/langmarks-quicklook-$slug.json"
                shot "langmarks-quicklook-$slug"
                cap_langmarks_code_receipt false "$name" "$code"
                key -k Escape >/dev/null
                settle
            done
        fi
        kill_flea
    done
    flea_ui="$saved_ui"
}
