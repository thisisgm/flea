# Sourced by ui.sh; native ui:capsweep and ui:capsweeplow keep separate evidence directories.

# Seconds one IPC poll waits for its state, used by the polling helpers below.
sweep_wait_seconds=30
# Seconds allowed for the model preview to decode and reach the column.
sweep_model_wait_seconds=25
# Eight wheel notches reach the two-page PDF stack's end at the capture window size.
sweep_pdf_end_notches=8

# Every capture asserts the effective text size, parsed theme and fresh PNG dimensions.
sweep_shot() {
    local name="sweep-$1" size dimensions
    size=$(token_of baseSize)
    [[ "$size" == 14 && "$(ipc bodyPx)" == 14 ]] || fail "capsweep: text size is $size, body $(ipc bodyPx), expected 14"
    assert_theme
    settle
    shot "$name"
    dimensions=$(magick identify -ping -format '%wx%h' "$evidence_dir/$name.png")
    # Sample input: 1320x820, from magick identify -ping -format %wx%h.
    [[ "$dimensions" =~ ^[1-9][0-9]*x[1-9][0-9]*$ ]] || fail "capsweep: invalid PNG dimensions for $name: $dimensions"
    printf 'SWEEP %s %s\n' "$name" "$dimensions"
    printf '%s\t%s\n' "$name" "$dimensions" >> "$evidence_dir/manifest.tsv"
}

# Poll scalar IPC with a wall-clock bound, including delayed preview decodes.
sweep_wait() {
    local reader="$1" want="$2" seen='' end=$((SECONDS + sweep_wait_seconds))
    shift 2
    while (( SECONDS < end )); do
        seen=$(ipc "$reader" "$@") || fail "capsweep: $reader unavailable"
        [[ "$seen" == "$want" ]] && return 0
        sleep 0.1
    done
    fail "capsweep: $reader expected '$want', saw '$seen'"
}

# Text IPC is unquoted text, so compare a fragment directly instead of feeding it to jq.
sweep_text() {
    local reader="$1" fragment="$2" seen='' end=$((SECONDS + sweep_wait_seconds))
    shift 2
    while (( SECONDS < end )); do
        seen=$(ipc "$reader" "$@") || fail "capsweep: $reader unavailable"
        [[ "$seen" == *"$fragment"* ]] && return 0
        sleep 0.1
    done
    fail "capsweep: $reader never contained '$fragment'"
}

sweep_json() {
    local reader="$1" expression="$2" label="$3" seen='' end=$((SECONDS + sweep_wait_seconds))
    shift 3
    while (( SECONDS < end )); do
        seen=$(ipc "$reader" "$@") || fail "capsweep: $reader unavailable"
        if jq -e "$expression" <<< "$seen" >/dev/null; then
            printf 'SWEEP_CHECK %s observed=%s\n' "$label" "$seen"
            return
        fi
        sleep 0.1
    done
    fail "capsweep: $label: $seen"
}

# Each surface starts from the same owned state, with size 14 pinned through the shipped schema.
sweep_launch() {
    local path="$1" patch="${2:-}" count
    [[ -n "$patch" ]] || patch='{}'
    kill_flea
    # Sample input: {"view":"grid","preview":{"thumbSize":"large"}}, merged into the default state.
    seed_ui_state "$sweep_root/state" "$(jq -cn --arg path "$sweep_root/views" --argjson patch "$patch" '
        {keys:"default",view:"list",hidden:false,columnsLimit:3,display:{textSize:{mode:14}},
         preview:{column:true,loadOn:"automatic",thumbSize:"medium"},
         menu:{hidden:[]},places:{favourites:[{label:"Sweep",path:$path}]}} * $patch')"
    launch "$path"
    count=$(find "$path" -mindepth 1 -maxdepth 1 ! -name '.*' | wc -l)
    wait_listing "$count"
    cap_resize 1320 820
    sweep_wait bodyPx 14
    sweep_wait path "$path"
}

# Two rows stay selected for the selection specimen and carry an actual cut clipboard afterward.
sweep_select() {
    local first second
    first=$(row_index_of a.txt)
    second=$(row_index_of b.txt)
    click_row "$first" left
    settle
    click_row "$second" left --mods ctrl
    sweep_wait selectionCount 2
}

# Grid steps tiles with l, since j moves a whole tile row and a one-row grid never reaches the dotfile.
sweep_seek() {
    local want="$1" i n
    [[ "$(ipc viewMode)" == grid ]] || { seek_row_named "$want"; return; }
    local trail='' cur
    n=$(ipc total)
    key g >/dev/null
    for i in $(seq 0 "$n"); do
        # Sample input: {"cursor":2,"cursorName":".hidden.txt"}; renameState reads the grid's cursor name.
        cur=$(ipc renameState | jq -r '"\(.cursor):\(.cursorName)"')
        trail+="$cur "
        [[ "${cur#*:}" == "$want" ]] && return 0
        key l >/dev/null
        settle
    done
    fail "capsweep: could not put the grid cursor on $want (total $n, trail $trail)"
}

# List, both Grid stops and Columns show selections, cut marks and visible dotfiles.
sweep_view_states() {
    local surface="$1" count="$2"
    sweep_select
    sweep_shot "$surface-selected"
    key x >/dev/null
    menus_expect keyDeliveryState '.clipboard.cut and (.clipboard.paths | length) == 2' 'cut clipboard names two selected rows'
    sweep_shot "$surface-cut"
    key -k Escape >/dev/null
    sweep_wait selectionCount 0
    key . >/dev/null
    wait_listing "$((count + 1))"
    sweep_wait showHidden true
    sweep_seek .hidden.txt
    sweep_shot "$surface-hidden"
}

# The views tree: two documents and a dotfile at the top and at a deep path.
sweep_views_fixture() {
    local path deep="$sweep_root/views/projects/release/candidate/deep"
    mkdir -p "$deep"
    for path in "$sweep_root/views" "$deep"; do
        printf 'Release notes\n' > "$path/a.txt"
        printf 'Second document\n' > "$path/b.txt"
        printf 'Hidden configuration\n' > "$path/.hidden.txt"
    done
}

# Columns at rest and a deep path, with selections, cut marks and visible dotfiles.
sweep_columns() {
    local deep="$sweep_root/views/projects/release/candidate/deep"
    sweep_views_fixture
    sweep_launch "$sweep_root/views" '{"view":"columns"}'
    sweep_wait viewMode columns
    sweep_wait columnCount 3
    sweep_view_states columns-rest 3
    sweep_launch "$deep" '{"view":"columns"}'
    sweep_wait viewMode columns
    sweep_wait columnCount 3
    sweep_view_states columns-deep 2
    kill_flea
}

# Views: List, default and larger Grid tiles, three Columns at rest and a deep path.
sweep_views() {
    local size
    sweep_views_fixture
    sweep_launch "$sweep_root/views"
    sweep_wait viewMode list
    sweep_view_states list 3
    for size in medium large; do
        sweep_launch "$sweep_root/views" "$(jq -cn --arg size "$size" '{view:"grid",preview:{thumbSize:$size}}')"
        sweep_wait viewMode grid
        menus_expect uiSettings ".preview.thumbSize == \"$size\"" "Grid $size thumbnail size loaded"
        sweep_view_states "grid-$size" 3
    done
    sweep_columns
}

# Preview fixtures reuse the suites' media, PDF, text and audio generators, plus a real alpha PNG.
sweep_preview_fixture() {
    formats_fixture "$sweep_root/previews"
    magick -size 480x320 xc:none -fill '#A9C6B7' -draw 'circle 240,160 240,40' "$sweep_root/previews/alpha.png"
    printf '# Release notes\n\nReadable **Markdown**, `inline code`, and a short list.\n\n- List view\n- Grid view\n' > "$sweep_root/previews/notes.md"
    printf '# Remote illustration\n\n![Remote image](https://cdn.example.com/preview.png)\n' > "$sweep_root/previews/remote.md"
    printf 'Folder child\n' > "$sweep_root/previews/subdir/child.txt"
    python3 - "$sweep_root/previews/large-readable.txt" <<'PY'
import pathlib, sys
pathlib.Path(sys.argv[1]).write_text("".join(f"Log row {index:05}: selected text loads only when requested.\n" for index in range(8000)))
PY
    # Quick Look autoplays; a long fixture keeps playback alive through native IPC and captures.
    ffmpeg -nostdin -v error -y -stream_loop -1 -i "$sweep_root/previews/v.mp4" -t 60 -c copy "$sweep_root/previews/long.mp4"
    python3 - "$sweep_root/previews/tone.wav" <<'PY'
import math, struct, sys, wave
with wave.open(sys.argv[1], "w") as output:
    output.setparams((1, 2, 16000, 0, "NONE", "not compressed"))
    output.writeframes(b"".join(struct.pack("<h", int(5000 * math.sin(i * math.tau * 440 / 16000))) for i in range(60 * 16000)))
PY
}

# Preview column and Quick Look: JPEG, alpha PNG, PDF pages, Markdown modes, audio and text.
sweep_previews() {
    local file kind tag expected state before after text_lines text_tail
    sweep_preview_fixture
    sweep_launch "$sweep_root/previews" '{"view":"columns"}'
    for tag in jpeg alpha pdf markdown remote audio text large overlimit; do
        case "$tag" in
            jpeg) file=p.jpg; kind=image; expected=image; state=decoded ;;
            alpha) file=alpha.png; kind=image; expected=image; state=decoded ;;
            pdf) file=manual.pdf; kind=pdf; expected=pdf; state=first-page ;;
            markdown) file=notes.md; kind=text; expected=ready; state=rendered ;;
            remote) file=remote.md; kind=text; expected=ready; state=placeholder ;;
            audio) file=tone.wav; kind=audio; expected=playing; state=ready ;;
            text) file=sample.txt; kind=text; expected=ready; state=loaded ;;
            large) file=large-readable.txt; kind=text; expected=ready; state=loaded ;;
            overlimit) file=big.txt; kind=text; expected='This file is too large to preview.'; state=refused ;;
        esac
        column_expect "$file" "$kind"
        case "$tag" in
            jpeg|alpha) matrix_wait_column image "$file" ;;
            pdf) sweep_wait columnPdfLoaded true; sweep_wait columnPdfPage 0 ;;
            markdown) sweep_text columnMarkdownText 'Release notes' ;;
            remote) sweep_text columnMarkdownText 'cdn.example.com' ;;
            audio) sweep_wait columnPlayerLoaded false; sweep_wait columnMediaPlaying false; sweep_text columnPlayCentre ' ' ;;
            text) sweep_wait columnTextLines 'hello from flea|second line' ;;
            large) sweep_text columnTextLines 'Log row 00000' ;;
            overlimit) sweep_wait columnTextLines 'too large' ;;
        esac
        if [[ "$tag" == markdown ]]; then
            [[ "$(ipc columnMarkdownText)" == 'Release notes'$'\n'* ]] || fail 'capsweep: column Markdown is not rendered'
        fi
        sweep_shot "column-$tag-$state"
        if [[ "$tag" == pdf ]]; then
            capmarkdown_column_pointer sweep-pdf-page-turn
            omarchy-drive scroll down "$sweep_pdf_end_notches" >/dev/null || fail 'capsweep: the PDF stack wheel failed'
            pdf_expect false '.pages == 2 and .scrollY > 0 and (.landed | index(1) != null)' 'sweep second stacked page decoded after scrolling'
            sweep_shot column-pdf-page-turn
        fi
        key -k Space >/dev/null
        sweep_wait previewOpen true
        sweep_wait previewKind "$kind"
        sweep_wait previewState "$expected"
        if [[ "$tag" == markdown ]]; then sweep_wait previewMarkdownView rendered; fi
        if [[ "$tag" == pdf ]]; then pdf_expect true '.page == 0 and .pages == 2' 'sweep first page decoded'; fi
        if [[ "$tag" == text ]]; then [[ "$(ipc previewText)" == *'hello from flea'* ]] || fail 'capsweep: Quick Look text missing'; fi
        if [[ "$tag" == large ]]; then
            # textShown returns retained windows; the paged owner supplies the whole file's line count.
            text_lines=$(wc -l < "$sweep_root/previews/$file")
            text_tail=$(tail -n 1 "$sweep_root/previews/$file")
            sweep_json previewTextState ".visible and .ready and .total == $text_lines" 'large preview reports the total line count from its paged owner' false
            menus_equal 'large preview owns the requested file' "$sweep_root/previews/$file" "$(ipc previewTextState false | jq -er .path)"
            sweep_shot quicklook-large-first-window
            key -k End >/dev/null
            sweep_json previewTextState ".visible and .ready and .total == $text_lines and .y > .minY" 'native End moves the large preview through its paged owner' false
            sweep_wait previewTextTail "$text_tail" "$(( ${#text_tail} + 1 ))"
            state=last-line
        fi
        if [[ "$tag" == remote ]]; then [[ "$(ipc previewText)" == *'cdn.example.com'* ]] || fail 'capsweep: Quick Look remote image missing'; fi
        [[ "$tag" != audio ]] || state=playing
        sweep_shot "quicklook-$tag-$state"
        if [[ "$tag" == pdf ]]; then
            key -k Right >/dev/null
            pdf_expect true '.page == 1' 'sweep page turn'
            sweep_shot quicklook-pdf-page-turn
        elif [[ "$tag" == markdown ]]; then
            key r >/dev/null
            sweep_wait previewState ready
            sweep_wait previewMarkdownView source
            sweep_wait previewText "$(cat "$sweep_root/previews/notes.md")"
            sweep_shot quicklook-markdown-source
        fi
        key -k Escape >/dev/null
        sweep_wait previewOpen false
        if [[ "$tag" == markdown ]]; then
            # The flip lived in the closed Quick Look: the column renders and the next Quick Look opens rendered.
            sweep_text columnMarkdownText 'Release notes'
            sweep_wait columnMarkdownView rendered
            sweep_shot column-markdown-rendered-after-flip
            key -k Space >/dev/null
            sweep_wait previewOpen true
            sweep_wait previewMarkdownView rendered
            key -k Escape >/dev/null
            sweep_wait previewOpen false
        fi
    done
    # Folder contents occupy the third column; Space intentionally opens no folder Quick Look.
    seek_row_named subdir
    sweep_text columnChildRowCentre ' ' 0
    [[ -n "$(ipc columnChildRowCentre 0)" ]] || fail 'capsweep: folder child column is empty'
    sweep_shot column-folder-contents
    key -k Space >/dev/null
    sweep_wait previewOpen false
    sweep_shot quicklook-folder-noop
    printf 'SWEEP_NOTE folder Quick Look is a silent no-op; large text refuses a whole-file read over 1 MiB.\n'
    # Video poster has no column player; Quick Look's poster is a loaded player paused with p.
    column_expect long.mp4 video
    matrix_wait_column video long.mp4
    sweep_wait columnPlayerLoaded false
    sweep_shot column-video-poster
    matrix_click_play
    sweep_wait columnMediaPlaying true
    before=$(ipc columnMediaPosition)
    settle
    after=$(ipc columnMediaPosition)
    (( after > before )) || fail "capsweep: column video did not advance: $before, $after"
    sweep_shot column-video-playing
    key -k Space >/dev/null
    sweep_wait previewState playing
    sweep_shot quicklook-video-playing
    key p >/dev/null
    sweep_wait previewState paused
    sweep_shot quicklook-video-poster
    printf 'SWEEP_NOTE Quick Look autoplays video; p pauses it to show its poster.\n'
    key -k Escape >/dev/null
    sweep_wait previewOpen false
    kill_flea
}

# Escape closes each flyout and its parent before the next menu specimen.
sweep_close_menu() {
    key -k Escape >/dev/null
    settle
    if [[ "$(ipc contextMenuVisible)" == true ]]; then key -k Escape >/dev/null; fi
    sweep_wait contextMenuVisible false
}

# Menus: one and several files, background, sidebar, Copy as, Paste as and Open with.
sweep_menus() {
    local first
    sweep_launch "$sweep_root/views"
    menus_file_menu a.txt
    menus_expect menuState '.opened and .snapshotReady' 'single-row menu settled'
    sweep_shot menu-row-one
    menus_seek copyAs
    key -k Right >/dev/null
    menus_expect menuState '.submenu' 'Copy as flyout open'
    sweep_shot menu-copy-as-open
    sweep_close_menu
    seek_row_named a.txt
    key y >/dev/null
    menus_expect keyDeliveryState '(.clipboard.paths | length) == 1 and (.clipboard.cut | not)' 'copy on clipboard'
    menus_file_menu a.txt
    menus_seek pasteAs
    key -k Right >/dev/null
    menus_expect menuState '.submenu' 'Paste as flyout open'
    sweep_shot menu-paste-as-open
    sweep_close_menu
    openwith_open_flyout a.txt
    sweep_shot menu-open-with-open
    sweep_close_menu
    sweep_select
    first=$(row_index_of a.txt)
    click_row "$first" right
    menus_expect menuState '.opened and .snapshotReady' 'multi-row menu settled'
    sweep_wait selectionCount 2
    sweep_shot menu-row-several
    sweep_close_menu
    key -k Escape >/dev/null
    sweep_wait selectionCount 0
    click_background
    menus_expect menuState '.opened and (.hasRow | not)' 'background menu opened'
    sweep_shot menu-background-open
    sweep_close_menu
    wait_rail_label Sweep
    click_rail_row "$(rail_row_of Sweep)" right
    sweep_wait contextMenuVisible true
    [[ "$(ipc contextMenuEntries)" == *'Remove from Favorites'* ]] || fail 'capsweep: sidebar menu missing favourite actions'
    sweep_shot menu-sidebar-open
    sweep_close_menu
    kill_flea
}

# Dialogs: delete, collision, one and several Permissions, New File, New Folder, inline Rename and Open with.
sweep_dialogs() {
    local section dir="$sweep_root/dialogs"
    mkdir -p "$dir/to"
    printf 'Source\n' > "$dir/a.txt"
    printf 'Other row\n' > "$dir/b.txt"
    printf 'Destination\n' > "$dir/to/a.txt"
    chmod 0644 "$dir/a.txt"
    chmod 0600 "$dir/b.txt"
    sweep_launch "$dir"
    menus_file_menu a.txt
    menus_choose deletePermanently
    menus_expect menuDialogState '.confirmation.opened and (.confirmation.destructiveFocus | not)' 'delete confirmation opens on Cancel'
    sweep_shot dialog-delete-confirm
    key -k Escape >/dev/null
    menus_expect menuDialogState '.opened | not' 'delete cancelled'
    [[ -f "$dir/a.txt" ]] || fail 'capsweep: deletion escaped capture'
    for section in one several; do
        if [[ "$section" == several ]]; then sweep_select; else click_row "$(row_index_of a.txt)" left; fi
        click_row "$(row_index_of a.txt)" right
        menus_choose permissions
        menus_expect permissionsState '.opened and (.busy | not)' 'Permissions settled'
        sweep_wait selectionCount "$([[ "$section" == several ]] && printf 2 || printf 1)"
        sweep_shot "dialog-permissions-$section"
        key -k Escape >/dev/null
        menus_expect permissionsState '.opened | not' 'Permissions closed'
        key -k Escape >/dev/null
    done
    click_background
    menus_choose newFile
    menus_expect menuDialogState '.opened and .action == "newFile"' 'New File opened'
    sweep_shot dialog-new-file-open
    key -k Escape >/dev/null
    menus_expect menuDialogState '.opened | not' 'New File closed'
    [[ ! -e "$dir/New Folder" ]] || fail 'capsweep: New Folder exists before the menu'
    click_background
    [[ ! -e "$dir/New Folder" ]] || fail 'capsweep: New Folder existed before the menu'
    menus_choose newFolder
    sweep_wait renameEditorLive true
    [[ -d "$dir/New Folder" ]] || fail 'capsweep: New Folder missing after menu'
    sweep_shot dialog-new-folder-editor
    key -k Escape >/dev/null
    sweep_wait renameEditorLive false
    [[ -d "$dir/New Folder" ]] || fail 'capsweep: Escape removed New Folder'
    seek_row_named a.txt
    key r >/dev/null
    sweep_wait renameEditorLive true
    sweep_shot dialog-rename-inline
    key -k Escape >/dev/null
    sweep_wait renameEditorLive false
    openwith_open_dialog a.txt
    sweep_shot dialog-open-with-open
    key -k Escape >/dev/null
    menus_expect openWithState '.opened | not' 'Open with closed'
    seek_row_named a.txt
    key y >/dev/null
    menus_expect keyDeliveryState '(.clipboard.paths | length) == 1 and (.clipboard.paths[0] | endswith("/a.txt")) and (.clipboard.cut | not)' 'collision source is on the clipboard'
    seek_row_named to
    key -k Return >/dev/null
    wait_path "$dir/to"
    wait_listing 1
    key p >/dev/null
    menus_expect collideState '.opened' 'paste collision opened'
    sweep_shot dialog-collide-open
    key -k Escape >/dev/null
    menus_expect collideState '.opened | not' 'collision cancelled'
    [[ "$(cat "$dir/to/a.txt")" == Destination ]] || fail 'capsweep: collision changed destination'
    [[ "$(cat "$dir/a.txt")" == Source && "$(cat "$dir/b.txt")" == 'Other row' ]] || fail 'capsweep: a cancelled dialog changed source bytes'
    kill_flea
}

# Settings: every live rail page once, plus the keymap sheet filtered by a typed query.
sweep_settings() {
    local section sweep_sections
    sweep_launch "$sweep_root/views"
    settings_open_key
    sweep_wait settingsOpen true
    # Sample input: [{"id":"keys"},{"id":"display"}], from settingsSections.
    mapfile -t sweep_sections < <(ipc settingsSections | jq -er '.[] | .id')
    (( ${#sweep_sections[@]} > 0 )) || fail 'capsweep: Settings inventory is empty'
    for section in "${sweep_sections[@]}"; do
        settings_section "$section"
        sweep_wait settingsSection "$section"
        sweep_shot "settings-$section-open"
    done
    key -k Escape >/dev/null
    sweep_wait settingsOpen false
    key '?' >/dev/null
    sweep_wait keymapSheetOpen true
    key copy >/dev/null
    sweep_wait keymapQuery copy
    sheet_rows=$(ipc keymapSheetRows)
    [[ "$sheet_rows" == *"copy as"* ]] || fail "capsweep: keymap query lists no copy action: ${sheet_rows//$'\n'/ | }"
    sweep_shot keymap-query-copy
    # The first Escape closes the sheet from a standing query (SheetKeys.sheetKey, CommandPalette callout 2).
    key -k Escape >/dev/null
    sweep_wait keymapSheetOpen false
    kill_flea
}

# Two owned windows fill equal monitor halves, with a selected file held in flight over B.
sweep_windows() {
    local apid aid bpid bid ax ay bx by width height sx sy dx dy result client address pid x end
    local drag_wait_seconds=10 drag_poll_seconds=0.1 placement_wait_seconds=20 drag_seen=false mouse_press=0x40 mouse_release=0x80
    sweep_launch "$sweep_root/views"
    apid=$(flea_pid)
    aid=$(xwdrag_qsid "$apid")
    xwdrag_launch_second "$sweep_root/dialogs"
    bpid=$XW_SECOND_PID
    bid=$XW_SECOND_ID
    trap 'xwdrag_cleanup; case_xwdrag_cleanup' EXIT
    trap xwdrag_signal_cleanup HUP INT TERM
    # Sample input: [{"focused":true,"x":0,"y":0,"width":1920,"height":1080,"scale":1}].
    read -r ax ay width height < <(hyprctl monitors -j | jq -er '.[] | select(.focused) | [.x,.y,(.width / .scale | floor),(.height / .scale | floor)] | @tsv')
    # Sample input: height=1080 width=1920, integer logical monitor dimensions.
    [[ "$height" =~ ^[0-9]+$ && "$width" =~ ^[0-9]+$ ]] || fail 'capsweep: focused monitor dimensions unavailable'
    width=$((width / 2))
    for pid in "$apid" "$bpid"; do
        x=$ax
        [[ "$pid" == "$bpid" ]] && x=$((ax + width))
        # Sample input: [{"pid":123,"address":"0xabc","floating":true}], from hyprctl clients -j.
        client=$(hyprctl clients -j | jq -ec --argjson pid "$pid" '.[] | select(.pid == $pid)')
        # Sample input: {"pid":123,"address":"0xabc","floating":true}.
        address=$(jq -er .address <<< "$client")
        # Sample input: 0xabc, an owned Hyprland window address.
        [[ "$address" =~ ^0x[0-9a-fA-F]+$ ]] || fail 'capsweep: invalid window address'
        # Sample input: {"pid":123,"address":"0xabc","floating":true}.
        if ! jq -e .floating <<< "$client" >/dev/null; then omarchy-drive window float "$address" >/dev/null; fi
        result=$(hypr_window_resize "$address" "$width" "$height" 2>&1) || fail "capsweep: half-screen resize refused: $result"
        result=$(hypr_window_move "$address" "$x" "$ay" 2>&1) || fail "capsweep: half-screen placement refused: $result"
        end=$((SECONDS + placement_wait_seconds))
        while (( SECONDS < end )); do
            # Sample input: 0 0 960 1080, the owned window's logical x, y, width and height.
            read -r bx by sx sy < <(xwdrag_geometry "$pid")
            [[ "$bx $by $sx $sy" == "$x $ay $width $height" ]] && break
            sleep 0.1
        done
        [[ "$bx $by $sx $sy" == "$x $ay $width $height" ]] || fail "capsweep: window $pid did not occupy its monitor half"
        [[ "$(xwdrag_qs "$(xwdrag_qsid "$pid")" tokens)" == *$'baseSize=14\n'* ]] || fail "capsweep: window $pid text size is not 14"
        [[ "$(xwdrag_qs "$(xwdrag_qsid "$pid")" bodyPx)" == 14 ]] || fail "capsweep: window $pid body size is not 14"
        [[ "$(xwdrag_qs "$(xwdrag_qsid "$pid")" themeLoaded)" == true ]] || fail "capsweep: window $pid theme is not ready"
        [[ "$(xwdrag_qs "$(xwdrag_qsid "$pid")" themeForeground)" == "${real_foreground,,}" ]] || fail "capsweep: window $pid has the wrong theme"
    done
    xwdrag_focus "$bpid"
    xwdrag_key "$(xwdrag_addr "$bpid")" -k End >/dev/null
    xwdrag_key "$(xwdrag_addr "$bpid")" v >/dev/null
    [[ "$(xwdrag_qs "$bid" selectionCount)" == 1 ]] || fail 'capsweep: B selection missing'
    xwdrag_focus "$apid"
    # Sample input: 120 240, the source row's desktop centre.
    read -r sx sy < <(xwdrag_row_point "$aid" "$apid" a.txt) || fail 'capsweep: A source row unavailable'
    # Sample input: 1100 800, the destination floor's desktop centre.
    read -r dx dy < <(xwdrag_floor_point "$bid" "$bpid") || fail 'capsweep: B drop floor unavailable'
    xwdrag_glide "$sx" "$sy"
    ydotool click "$mouse_press" >/dev/null 2>&1
    sleep 0.3
    xwdrag_glide "$dx" "$dy"
    settle
    end=$((SECONDS + drag_wait_seconds))
    while (( SECONDS < end )); do
        drag_seen=$(xwdrag_qs "$bid" listingDropActive) || drag_seen=unavailable
        [[ "$drag_seen" == true ]] && break
        sleep "$drag_poll_seconds"
    done
    if [[ "$drag_seen" != true ]]; then
        ydotool click "$mouse_release" >/dev/null 2>&1 || fail 'capsweep: pointer release failed'
        fail "capsweep: B has no drag in flight over its floor, saw '$drag_seen'"
    fi
    [[ -f "$sweep_root/views/a.txt" ]] || fail 'capsweep: source moved before button release'
    sweep_desktop_shot windows-drag-held
    ydotool click "$mouse_release" >/dev/null 2>&1 || fail 'capsweep: pointer release failed'
    xwdrag_kill_second "$bpid"
    cat "$run_root/flea-second.log" >> "$run_log"
    kill_flea
    trap - EXIT HUP INT TERM
}

# Desktop capture includes both halves and verifies the image, with no title-based ambiguity.
sweep_desktop_shot() {
    local name="sweep-$1" dimensions png="$evidence_dir/sweep-$1.png"
    [[ ! -e "$png" && ! -L "$png" ]] || fail "capsweep: refusing existing evidence $png"
    omarchy-drive shot "$png" >/dev/null || fail 'capsweep: desktop capture failed'
    [[ -s "$png" ]] || fail 'capsweep: empty desktop capture'
    dimensions=$(magick identify -ping -format '%wx%h' "$png")
    # Sample input: 1920x1080, from magick identify -ping -format %wx%h.
    [[ "$dimensions" =~ ^[1-9][0-9]*x[1-9][0-9]*$ ]] || fail 'capsweep: invalid desktop capture dimensions'
    printf 'SWEEP %s %s\n' "$name" "$dimensions"
    printf '%s\t%s\n' "$name" "$dimensions" >> "$evidence_dir/manifest.tsv"
}

# Current palette or Cool Dawn runs through the same cases and an isolated child HOME.
sweep_run() {
    local sweep_theme="$1" sweep_root="$fixture_root/capsweep-$1" sweep_real_bin="$flea_bin" sweep_home
    # A section run names its evidence for its sections at the pinned size, apart from the theme runs.
    local sweep_evidence="${2:-$1}"
    [[ -n "${2:-}" ]] && sweep_evidence="${sweep_evidence}14"
    local real_foreground="$real_foreground" flea_bin="$flea_bin" evidence_dir="$evidence_dir/$sweep_evidence" menus_checks=0 picker_wait_seconds=180
    local -a sweep_sections=(${2:-views previews menus dialogs settings windows picker})
    local section
    sandbox_make "$sweep_root"
    sweep_home="$sweep_root/home"
    fixture_home_make "$sweep_home"
    local real_state_dir="$sweep_home/.local/state/omarchy/current"
    if [[ "$sweep_theme" == cool-dawn ]]; then
        cp "$repo/tests/fixtures/cool-dawn/colors.toml" "$sweep_home/.local/state/omarchy/current/theme/colors.toml"
        printf 'cool-dawn\n' > "$sweep_home/.local/state/omarchy/current/theme.name"
        # Sample input: foreground = "#DFE8E0" in the sweep's installed colors.toml.
        real_foreground=$(python3 -c 'import pathlib,sys,tomllib; print(tomllib.loads(pathlib.Path(sys.argv[1]).read_text())["foreground"].lower())' "$sweep_home/.local/state/omarchy/current/theme/colors.toml")
    fi
    mkdir -p "$evidence_dir" "$sweep_root/views"
    [[ ! -e "$evidence_dir/manifest.tsv" ]] || fail 'capsweep: evidence directory already used'
    # Only the child receives HOME; the operator environment and every session file stay intact.
    local -a flea_launch_wrap=()
    if [[ "$test_frontend" == gpui ]]; then
        flea_launch_wrap=(env "HOME=$sweep_home" "XDG_CONFIG_HOME=$sweep_home/.config")
    else
        flea_bin="$sweep_root/launcher"
        { printf '#!/usr/bin/env bash\n'; printf 'exec env HOME=%q %q "$@"\n' "$sweep_home" "$sweep_real_bin"; } > "$flea_bin"
        chmod +x "$flea_bin"
    fi
    printf 'SWEEP_THEME %s textSize=14 seam=display.textSize childHome=%s\n' "$sweep_theme" "$sweep_home"
    for section in "${sweep_sections[@]}"; do
        case "$section" in
            views) sweep_views ;;
            columns) sweep_columns ;;
            previews) sweep_previews ;;
            menus) sweep_menus ;;
            dialogs) sweep_dialogs ;;
            settings) sweep_settings ;;
            windows) sweep_windows ;;
            picker) ;;
            *) fail "capsweep: no section named $section" ;;
        esac
    done
    if [[ " ${sweep_sections[*]} " == *" picker "* ]]; then
        FLEA_BIN="$sweep_real_bin" FLEA_UI="$flea_ui" timeout "$picker_wait_seconds" python3 "$repo/tests/ui-captures-sweep-picker.py" "$sweep_root/picker" "$sweep_home" "$evidence_dir" \
            || fail "capsweep: picker sweep failed, status $?"
        cat "$evidence_dir/picker-backend.log" >> "$run_log"
    fi
    printf 'SWEEP_TOTAL %s %s shots\n' "$sweep_theme" "$(wc -l < "$evidence_dir/manifest.tsv")"
}

# Current-theme sweep covers views, previews, menus, dialogs, Settings, picker and two windows.
case_capsweep() { sweep_run current; }

# Cool Dawn repeats the entire native sweep without editing the user's theme or text setting.
case_capsweeplow() { sweep_run cool-dawn; }

# Columns only at size 14 on the live theme, for one native columns specimen.
case_cap_columns14() { sweep_run current columns; }

# An STL previews as a flat-shaded model, in the column and in Quick Look (HIG ql-stl, GM 2026-10-17 3D previews), never a cache thumbnail or the unreadable sentence.
capsweepstl_wait_column() {
    local file="$1" end=$((SECONDS + sweep_model_wait_seconds)) path="" state=""
    while (( SECONDS < end )); do
        path=$(ipc previewSelectionState | jq -r .path)
        state=$(ipc previewColumnState)
        [[ "$path" == *"$file" && "$state" == model3d ]] && return 0
        sleep 0.1
    done
    fail "capsweepstl: the column shows $path as '$state', want $file as model3d"
}

# The STL renders as a shaded model in both the preview column and Quick Look.
case_capsweepstl() {
    local dir="$fixture_root/capsweepstl" seen end
    sandbox_scratch "$dir"
    # One facet is the smallest model the renderer can shade.
    printf 'solid capsweepstl\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\nendsolid capsweepstl\n' > "$dir/model.stl" \
        || fail 'capsweepstl: the STL fixture failed'
    seed_ui_state "$fixture_root/capsweepstl-state" '{"keys":"default","view":"columns","display":{"textSize":{"mode":14}}}'
    launch "$dir"
    wait_listing 1
    settle
    seek_row_named model.stl
    capsweepstl_wait_column model.stl
    sweep_shot column-stl-model
    key -k Space >/dev/null
    sweep_wait previewOpen true
    sweep_wait previewKind model3d
    end=$((SECONDS + sweep_wait_seconds))
    seen=$(ipc previewState)
    while (( SECONDS < end )) && [[ "$seen" == loading ]]; do
        sleep 0.1
        seen=$(ipc previewState) || fail 'capsweepstl: previewState unavailable'
    done
    [[ "$seen" == model3d ]] || fail "capsweepstl: Quick Look on the STL ended on '$seen', want model3d"
    sweep_shot quicklook-stl-model
    key -k Escape >/dev/null
    sweep_wait previewOpen false
    printf 'CAPSWEEPSTL column=model3d quicklook=model3d state=%s\n' "$seen"
    kill_flea
}
