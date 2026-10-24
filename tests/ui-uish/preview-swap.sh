# GPUI overrides for UISH-19; seeded from the unchanged oracle at 2faf8d099.
case_watch() {
    local dir="$fixture_root/watch"
    sandbox_scratch "$dir"
    printf 'a\n' > "$dir/alpha.txt"
    printf 'b\n' > "$dir/beta.txt"
    printf 'p\n' > "$dir/preview-me.txt"
    launch "$dir"
    wait_listing 3
    [[ "$(ipc rowAt 0)" == alpha.txt\|* ]] || fail "watch: row 0 is $(ipc rowAt 0), not alpha.txt"

    # Issue 159's move-into-new-directory case: create and move from outside Flea while its one window stays open.
    local move_bytes='watch-move-payload-159' watch_pid
    watch_pid=$(flea_pid)
    printf '%s' "$move_bytes" > "$dir/move-source.txt"
    mkdir "$dir/move-target"
    mv "$dir/move-source.txt" "$dir/move-target/move-source.txt"
    flea_ipc_wait -p "$flea_ui/boot" flea total 4 --timeout 15 >/dev/null \
        || fail "watch: creating a directory and moving a file into it left the listing at $(ipc total) rows"
    [[ "$(ipc rowAt 0)" == move-target\|dir\|* ]] \
        || fail "watch: the moved-into directory is not row 0, got $(ipc rowAt 0)"
    for _move_row in 0 1 2 3; do
        [[ "$(ipc rowAt "$_move_row")" != move-source.txt\|* ]] \
            || fail "watch: the moved file stayed in the parent listing at row $_move_row"
    done
    seek_row_named move-target
    key -k Return >/dev/null
    wait_path "$dir/move-target"
    wait_listing 1
    [[ "$(ipc rowAt 0)" == move-source.txt\|file\|* ]] \
        || fail "watch: entering the new directory lists $(ipc rowAt 0), not the moved file"
    open_row move-source.txt
    local preview_wait
    for preview_wait in $(seq 1 100); do
        [[ "$(ipc previewState)" == ready ]] && break
        sleep 0.05
    done
    [[ "$(ipc previewState)" == ready && "$(ipc previewText)" == "$move_bytes" ]] \
        || fail "watch: the moved file preview is state=$(ipc previewState), bytes=$(ipc previewText | od -An -tx1)"
    key -k Escape >/dev/null
    settle
    key -k Backspace >/dev/null
    wait_path "$dir"
    wait_listing 4
    [[ "$(flea_pid)" == "$watch_pid" ]] || fail "watch: the move case relaunched Flea"
    printf 'WATCH move=ok parent-row=gone child-bytes=exact no-relaunch=ok\n'
    [[ "$(ipc selectionCount)" == 1 ]] || fail "watch: returning to the parent did not select the folder"
    key -k Escape >/dev/null
    [[ "$(ipc selectionCount)" == 0 ]] || fail "watch: Escape did not release the folder selection"
    rm "$dir/move-target/move-source.txt"
    rmdir "$dir/move-target"
    flea_ipc_wait -p "$flea_ui/boot" flea total 3 --timeout 15 >/dev/null \
        || fail "watch: move-case cleanup left the parent listing at $(ipc total) rows"

    # The reporter's four changes, from another process, while the window sits on the folder.
    printf 'new\n' > "$dir/NEWFILE-appeared.txt"
    mv "$dir/alpha.txt" "$dir/alpha-RENAMED.txt"
    rm "$dir/beta.txt"
    mkdir "$dir/brand-new-folder"
    # The 400 ms settle plus the re-read; the reporter waited several seconds and saw nothing move.
    flea_ipc_wait -p "$flea_ui/boot" flea total 4 --timeout 15 >/dev/null \
        || fail "watch: the listing stayed at $(ipc total) rows after four outside changes"
    settle
    printf 'WATCH total=%s row0=%q row1=%q row2=%q\n' \
        "$(ipc total)" "$(ipc rowAt 0)" "$(ipc rowAt 1)" "$(ipc rowAt 2)"
    shot watch-after-outside-changes
    # Directories first, then name ascending and case-insensitive: brand-new-folder,
    # alpha-RENAMED.txt, NEWFILE-appeared.txt, preview-me.txt.
    [[ "$(ipc rowAt 0)" == brand-new-folder\|dir\|* ]] \
        || fail "watch: the new directory is not row 0, got $(ipc rowAt 0)"
    [[ "$(ipc rowAt 1)" == alpha-RENAMED.txt\|* ]] \
        || fail "watch: the renamed file is not row 1, got $(ipc rowAt 1)"
    [[ "$(ipc rowAt 2)" == NEWFILE-appeared.txt\|* ]] \
        || fail "watch: the created file is not row 2, got $(ipc rowAt 2)"
    [[ "$(ipc rowAt 3)" == preview-me.txt\|* ]] \
        || fail "watch: the untouched file is not row 3, got $(ipc rowAt 3)"

    # The cursor is put back on the file it was on, not on the row that index now names: without the
    # anchor the create above it leaves the cursor on brand-new-folder.
    goto_row 3
    [[ "$(ipc rowAt "$(ipc cursor)")" == preview-me.txt\|* ]] \
        || fail "watch: the cursor did not start on preview-me.txt"
    printf 'z\n' > "$dir/AAA-above-the-cursor.txt"
    flea_ipc_wait -p "$flea_ui/boot" flea total 5 --timeout 15 >/dev/null \
        || fail "watch: the second outside create left the listing at $(ipc total) rows"
    settle
    printf 'WATCH cursor=%s row=%q\n' "$(ipc cursor)" "$(ipc rowAt "$(ipc cursor)")"
    [[ "$(ipc rowAt "$(ipc cursor)")" == preview-me.txt\|* ]] \
        || fail "watch: a create above the cursor moved it to $(ipc rowAt "$(ipc cursor)")"

    # xw5: a selection no longer holds the re-read back. The change applies at once and every mark
    # stays on the same file by identity, so a window holding a selection never looks stale.
    key v >/dev/null
    settle
    [[ "$(ipc selectionCount)" == "1" ]] || fail "watch: v did not select the cursor row"
    printf 'held\n' > "$dir/BBB-while-selected.txt"
    flea_ipc_wait -p "$flea_ui/boot" flea total 6 --timeout 15 >/dev/null \
        || fail "watch: the listing did not re-read to 6 rows while a selection stood, total is $(ipc total)"
    settle
    [[ "$(ipc selectionCount)" == "1" ]] \
        || fail "watch: the re-read cleared the held selection"
    [[ "$(ipc rowAt "$(ipc cursor)")" == preview-me.txt\|* ]] \
        || fail "watch: the re-read moved the cursor to $(ipc rowAt "$(ipc cursor)")"
    [[ "$(ipc selectedIndices)" == "$(ipc cursor)" ]] \
        || fail "watch: the held mark is on $(ipc selectedIndices), not on the cursor file"
    # Clearing the selection now owes nothing: the change was already applied under it.
    key -k Escape >/dev/null
    settle
    [[ "$(ipc selectionCount)" == "0" ]] || fail "watch: Escape did not clear the selection"
    [[ "$(ipc total)" == "6" ]] || fail "watch: clearing the selection re-read to $(ipc total) rows"
    printf 'WATCH immediate=ok kept=ok total=%s\n' "$(ipc total)"

    # A directory under continuous writing still has to settle. The timer absorbs notifications rather
    # than being restarted by them, so the sample that matters is taken WHILE the writing is still
    # going: a restart() is pushed forward by every notification and re-reads nothing until the writer
    # stops, which a sample taken afterwards cannot tell apart from a timer that fired all along.
    local before during writer n
    before=$(ipc total)
    ( for n in $(seq 1 40); do
          printf 'x\n' > "$dir/stream-$n.txt"
          sleep 0.1
      done ) &
    writer=$!
    sleep 1.2
    during=$(ipc total)
    wait "$writer"
    printf 'WATCH stream before=%s during=%s after=%s\n' "$before" "$during" "$(ipc total)"
    (( during > before )) \
        || fail "watch: nothing re-read while the directory was still being written, total stayed $before"

    # A debt owed here must not be paid by re-listing the next folder: the open menu holds it and takes every key, so its own Open on the folder row is the navigation.
    key -k Home >/dev/null
    settle
    [[ "$(ipc rowAt 0)" == brand-new-folder\|dir\|* ]] || fail "watch: row 0 is not the folder the menu opens: $(ipc rowAt 0)"
    key m >/dev/null
    settle
    local held_total
    held_total=$(ipc total)
    printf 'owed\n' > "$dir/CCC-owed-on-leaving.txt"
    sleep 1
    [[ "$(ipc total)" == "$held_total" ]] || fail "watch: the open menu did not hold the re-read, total $held_total to $(ipc total)"
    # Counted from before the navigation, not from after it: the menu still holds the debt at the
    # navigation, so without the guard the timer pays it with a re-list of the folder being opened,
    # which wait_path polling would otherwise count as the navigation itself.
    local before_nav after_nav
    before_nav=$(ipc listRequests)
    menus_seek open
    key -k Return >/dev/null
    wait_path "$dir/brand-new-folder"
    sleep 1.5
    after_nav=$(ipc listRequests)
    printf 'WATCH carried lists %s to %s, one navigation and nothing else\n' "$before_nav" "$after_nav"
    (( after_nav == before_nav + 1 )) \
        || fail "watch: leaving cost $(( after_nav - before_nav )) listings, so a debt owed for the directory just left was paid by the one the pane moved to"
    key -k Escape >/dev/null
    settle
    assert_window
    kill_flea
}

case_previewswap() {
    local dir="$fixture_root/previewswap"
    sandbox_scratch "$dir"
    mkdir -p "$dir/0-folder"
    # Real media, made the way case_views makes its clip: a zero-byte image or video fails to decode and Qt logs it.
    ffmpeg -y -f lavfi -i "testsrc=size=64x64" -frames:v 1 "$dir/a-image.jpg" >/dev/null 2>&1 || fail "previewswap: ffmpeg could not make a-image.jpg"
    ffmpeg -y -f lavfi -i "testsrc=duration=1:size=64x64:rate=10" "$dir/b-video.mp4" >/dev/null 2>&1 || fail "previewswap: ffmpeg could not make b-video.mp4"
    : > "$dir/c-doc.pdf"
    printf 'hello from flea\n' > "$dir/d-notes.txt"
    printf 'fn main() {}\n' > "$dir/e-main.rs"
    : > "$dir/f-report.odt"
    : > "$dir/g-font.ttf"
    : > "$dir/h-archive.zip"
    ffmpeg -y -f lavfi -i "testsrc=size=64x64" -frames:v 1 "$dir/i-photo.png" >/dev/null 2>&1 || fail "previewswap: ffmpeg could not make i-photo.png"

    seed_ui_state "$fixture_root/previewswap-state" '{"keys":"default","view":"columns"}'
    launch "$dir"
    wait_listing 10
    settle
    [[ "$(ipc viewMode)" == columns ]] || fail "previewswap: fixture did not open its columns view"

    local before after before_mid after_mid frame_rect picture_rect
    before=$(ipc previewSwapState)
    # Walk the whole preview row: each j holds the old picture until the new one is whole.
    goto_row 0
    settle
    local i
    for i in $(seq 1 9); do
        key j >/dev/null
        settle
    done
    for i in $(seq 1 9); do
        key k >/dev/null
        settle
    done
    shot previewswap-column-landed
    after=$(ipc previewSwapState)
    before_mid=$(jq -r '.column.midFrames // 0' <<< "$before")
    after_mid=$(jq -r '.column.midFrames // 0' <<< "$after")
    [[ "$(jq -r '.column.holds // 0' <<< "$after")" -gt "$(jq -r '.column.holds // 0' <<< "$before")" ]] \
        || fail "previewswap: the column walk took no hold; before $before after $after"
    [[ "$after_mid" == "$before_mid" ]] \
        || fail "previewswap: the column walk drew $((after_mid - before_mid)) half-built frame(s); before $before after $after"
    printf 'PREVIEWSWAP column holds=%s mid=%s fallbacks=%s\n' \
        "$(jq -r '.column.holds' <<< "$after")" "$after_mid" "$(jq -r '.column.fallbacks' <<< "$after")"

    # Quick Look follows the cursor the same way: Space opens, j/k move under the held picture.
    goto_row 1
    settle
    key -k space >/dev/null
    settle
    [[ "$(ipc previewOpen)" == "true" ]] || fail "previewswap: space did not open Quick Look"
    before=$(ipc previewSwapState)
    key j >/dev/null
    settle
    key j >/dev/null
    settle
    key k >/dev/null
    settle
    shot previewswap-look-landed
    key -k Escape >/dev/null
    settle
    after=$(ipc previewSwapState)
    before_mid=$(jq -r '.look.midFrames // 0' <<< "$before")
    after_mid=$(jq -r '.look.midFrames // 0' <<< "$after")
    [[ "$(jq -r '.look.holds // 0' <<< "$after")" -gt "$(jq -r '.look.holds // 0' <<< "$before")" ]] \
        || fail "previewswap: Quick Look took no hold across its moves; before $before after $after"
    [[ "$after_mid" == "$before_mid" ]] \
        || fail "previewswap: Quick Look drew $((after_mid - before_mid)) half-built frame(s); before $before after $after"
    printf 'PREVIEWSWAP look holds=%s mid=%s fallbacks=%s\n' \
        "$(jq -r '.look.holds' <<< "$after")" "$after_mid" "$(jq -r '.look.fallbacks' <<< "$after")"

    # A small clip's poster fills the frame the way its player does; a small image keeps its own size.
    seek_row_named "b-video.mp4"
    settle
    for _attempt in $(seq 1 60); do
        [[ "$(ipc previewColumnState)" == "video" && "$(ipc columnThumbShown)" == "true" ]] && break
        sleep 0.1
    done
    [[ "$(ipc previewColumnState)" == "video" && "$(ipc columnThumbShown)" == "true" ]] \
        || fail "previewswap: b-video.mp4 shows $(ipc previewColumnState), thumb shown $(ipc columnThumbShown)"
    for _attempt in $(seq 1 60); do
        [[ "$(ipc columnFrameReady)" == "true" ]] && break
        sleep 0.1
    done
    [[ "$(ipc columnFrameReady)" == "true" ]] \
        || fail "previewswap: b-video.mp4 frame never read Ready"
    frame_rect=$(ipc columnFrameRect)
    picture_rect=$(ipc columnPictureRect)
    [[ "$frame_rect" =~ ^-?[0-9]+\ -?[0-9]+\ [0-9]+\ [0-9]+$ ]] \
        || fail "previewswap: b-video.mp4 frame has no valid rectangle: $frame_rect"
    [[ "$picture_rect" =~ ^-?[0-9]+\ -?[0-9]+\ [0-9]+\ [0-9]+$ ]] \
        || fail "previewswap: b-video.mp4 picture has no valid rectangle: $picture_rect"
    read -r fx fy fw fh <<< "$frame_rect"
    read -r px py pw ph <<< "$picture_rect"
    printf 'PREVIEWSWAP poster frame=%s,%s,%s,%s picture=%s,%s,%s,%s\n' "$fx" "$fy" "$fw" "$fh" "$px" "$py" "$pw" "$ph"
    (( ph >= fh - 4 && ph <= fh )) \
        || fail "previewswap: the 64x64 clip poster is $pw x $ph in a $fw x $fh frame, not filled on its limiting side"
    (( 2 * px + pw - (2 * fx + fw) >= -4 && 2 * px + pw - (2 * fx + fw) <= 4 )) \
        || fail "previewswap: the clip poster is not centred horizontally in its frame"
    (( 2 * py + ph - (2 * fy + fh) >= -4 && 2 * py + ph - (2 * fy + fh) <= 4 )) \
        || fail "previewswap: the clip poster is not centred vertically in its frame"
    seek_row_named "a-image.jpg"
    settle
    for _attempt in $(seq 1 60); do
        [[ "$(ipc previewColumnState)" == "image" && "$(ipc columnThumbShown)" == "true" ]] && break
        sleep 0.1
    done
    [[ "$(ipc previewColumnState)" == "image" && "$(ipc columnThumbShown)" == "true" ]] \
        || fail "previewswap: a-image.jpg shows $(ipc previewColumnState), thumb shown $(ipc columnThumbShown)"
    for _attempt in $(seq 1 60); do
        [[ "$(ipc columnFrameReady)" == "true" ]] && break
        sleep 0.1
    done
    [[ "$(ipc columnFrameReady)" == "true" ]] \
        || fail "previewswap: a-image.jpg frame never read Ready"
    picture_rect=$(ipc columnPictureRect)
    [[ "$picture_rect" =~ ^-?[0-9]+\ -?[0-9]+\ [0-9]+\ [0-9]+$ ]] \
        || fail "previewswap: a-image.jpg picture has no valid rectangle: $picture_rect"
    read -r px py pw ph <<< "$picture_rect"
    printf 'PREVIEWSWAP still frame=%s,%s,%s,%s picture=%s,%s,%s,%s\n' "$fx" "$fy" "$fw" "$fh" "$px" "$py" "$pw" "$ph"
    (( pw >= 62 && pw <= 66 && ph >= 62 && ph <= 66 )) \
        || fail "previewswap: the 64x64 image draws at $pw x $ph, not its own size"

    printf 'PREVIEWSWAP column=ok look=ok %s\n' "$(ipc previewSwapState)"
    kill_flea
}
