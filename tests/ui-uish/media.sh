# GPUI overrides for UISH-18; seeded from the unchanged oracle at 2faf8d099.
case_mute() {
    command -v ffmpeg >/dev/null || fail "ffmpeg is missing, so the audio fixture cannot be built"
    local dir="$fixture_root/mute"
    sandbox_scratch "$dir"
    # Audio, not video: the strip is permanent on an audio preview, so nothing has to be revealed.
    ffmpeg -y -f lavfi -i "sine=frequency=440:duration=20" "$dir/tone.wav" >/dev/null 2>&1
    [[ -s "$dir/tone.wav" ]] || fail "mute: ffmpeg produced no tone.wav"

    launch "$dir"
    wait_listing 1
    key -k space >/dev/null
    for _attempt in $(seq 1 60); do
        [[ "$(ipc previewOpen)" == "true" ]] && break
        sleep 0.25
    done
    [[ "$(ipc previewOpen)" == "true" && "$(ipc previewKind)" == "audio" ]] \
        || fail "mute: the preview is $(ipc previewKind), open=$(ipc previewOpen)"
    local strip
    strip=$(ipc previewStrip) || fail "mute: the strip has no state"
    [[ "$(jq -r .visible <<< "$strip")" == "true" ]] || fail "mute: an audio preview drew no strip"
    [[ "$(jq -r .muted <<< "$strip")" == "false" ]] || fail "mute: the session opened muted"

    echo "-- m mutes, and the player keeps going --"
    local before after
    before=$(ipc previewPosition)
    key m >/dev/null
    for _attempt in $(seq 1 40); do
        [[ "$(ipc previewStrip | jq -r .muted)" == "true" ]] && break
        sleep 0.25
    done
    [[ "$(ipc previewStrip | jq -r .muted)" == "true" ]] || fail "mute: m did not mute"
    [[ "$(ipc previewState)" == "playing" ]] || fail "mute: m stopped the player, state is $(ipc previewState)"
    sleep 2
    after=$(ipc previewPosition)
    (( after > before )) || fail "mute: the clock stopped at $after, so mute paused the player"
    printf 'MUTE position %s then %s while muted\n' "$before" "$after"

    echo "-- and the mark's own click flips it back --"
    local wx wy cx cy
    read -r wx wy _ _ < <(window_box) || fail "mute: native window coordinates unavailable"
    read -r cx cy <<< "$(ipc previewStrip | jq -r .mute)"
    [[ -n "$cy" ]] || fail "mute: the mark has no centre"
    omarchy-drive click "$((wx + cx))" "$((wy + cy))" left >/dev/null || fail "mute: could not click the mark"
    for _attempt in $(seq 1 40); do
        [[ "$(ipc previewStrip | jq -r .muted)" == "false" ]] && break
        sleep 0.25
    done
    [[ "$(ipc previewStrip | jq -r .muted)" == "false" ]] || fail "mute: the mark's click did not unmute"
    [[ "$(ipc previewState)" == "playing" ]] || fail "mute: the click stopped the player"

    echo "-- the flag is the session's, so it survives the preview that set it --"
    key m >/dev/null
    for _attempt in $(seq 1 40); do
        [[ "$(ipc previewStrip | jq -r .muted)" == "true" ]] && break
        sleep 0.25
    done
    key -k Escape >/dev/null
    settle
    [[ "$(ipc previewOpen)" == "false" ]] || fail "mute: escape left the preview open"
    key -k space >/dev/null
    for _attempt in $(seq 1 60); do
        [[ "$(ipc previewOpen)" == "true" ]] && break
        sleep 0.25
    done
    [[ "$(ipc previewStrip | jq -r .muted)" == "true" ]] \
        || fail "mute: the next preview forgot the session's own flag"
    key -k Escape >/dev/null
    settle
    kill_flea
}

case_previewviews() {
    local dir="$fixture_root/previewviews" mode name wx wy cx cy fx fy fw fh p1 p2 changed _attempt
    formats_fixture "$dir"
    launch "$dir"
    wait_listing 26
    read -r wx wy _ww _wh < <(window_box) || fail "native window coordinates unavailable"
    for mode in list grid columns; do
        switch_view list
        goto_row "$(row_index_of p.jpg)"
        switch_view "$mode"
        key -k space >/dev/null
        for _attempt in $(seq 1 40); do [[ "$(ipc previewState)" == "image" ]] && break; sleep 0.1; done
        [[ "$(ipc previewOpen)" == "true" && "$(ipc previewKind)" == "image" && "$(ipc previewState)" == "image" ]] \
            || fail "$mode: Space on p.jpg: open $(ipc previewOpen), kind $(ipc previewKind), state $(ipc previewState)"
        key -k space >/dev/null
        settle
        [[ "$(ipc previewOpen)" == "false" ]] || fail "$mode: Space did not close the p.jpg preview"
        switch_view list
        goto_row "$(row_index_of manual.pdf)"
        switch_view "$mode"
        key -k space >/dev/null
        for _attempt in $(seq 1 40); do [[ "$(ipc previewState)" == "pdf" ]] && break; sleep 0.1; done
        [[ "$(ipc previewKind)" == "pdf" && "$(ipc previewState)" == "pdf" && "$(ipc previewPdfPage)" == "0" ]] \
            || fail "$mode: Space on manual.pdf: kind $(ipc previewKind), state $(ipc previewState), page $(ipc previewPdfPage)"
        # Space closes a PDF Quick Look instead of paging or zooming it; the open page and zoom pin the old behaviour, cleared readers aside.
        local page_before zoom_before page_after zoom_after
        page_before=$(ipc previewPdfPage); zoom_before=$(ipc previewPdfZoom)
        [[ "$page_before" == "0" && -n "$zoom_before" ]] \
            || fail "$mode: manual.pdf opened on page $page_before zoom $zoom_before, not page 0"
        # A reopen from page 0 proves nothing, so page off it first: the close must keep the page and the reopen must reset it.
        key -k Right >/dev/null
        settle
        key -k Right >/dev/null
        settle
        [[ "$(ipc previewPdfPage)" == "1" ]] || fail "$mode: two Rights left manual.pdf on page $(ipc previewPdfPage), not the last page 1"
        page_before=$(ipc previewPdfPage)
        key -k space >/dev/null
        settle
        [[ "$(ipc previewOpen)" == "false" ]] || fail "$mode: Space did not close the manual.pdf preview"
        page_after=$(ipc previewPdfPage); zoom_after=$(ipc previewPdfZoom)
        [[ "$page_after" == "$page_before" || "$page_after" == "-1" ]] \
            || fail "$mode: Space paged manual.pdf to $page_after instead of closing it"
        [[ "$zoom_after" == "$zoom_before" || -z "$zoom_after" ]] \
            || fail "$mode: Space zoomed manual.pdf to $zoom_after instead of closing it"
        key -k space >/dev/null
        for _attempt in $(seq 1 40); do [[ "$(ipc previewState)" == "pdf" ]] && break; sleep 0.1; done
        [[ "$(ipc previewOpen)" == "true" && "$(ipc previewPdfPage)" == "0" ]] \
            || fail "$mode: reopened manual.pdf shows open $(ipc previewOpen) on page $(ipc previewPdfPage), not an open preview on the first"
        key -k Right >/dev/null
        settle
        key -k Right >/dev/null
        settle
        [[ "$(ipc previewPdfPage)" == "1" ]] || fail "$mode: two Rights left manual.pdf on page $(ipc previewPdfPage), not the last page 1"
        key -k Escape >/dev/null
        settle
        switch_view list
        goto_row "$(row_index_of broken.pdf)"
        switch_view "$mode"
        key -k space >/dev/null
        for _attempt in $(seq 1 40); do [[ "$(ipc previewState)" == "This file could not be read." ]] && break; sleep 0.1; done
        [[ "$(ipc previewState)" == "This file could not be read." ]] || fail "$mode: Space on broken.pdf reads '$(ipc previewState)'"
        key -k Escape >/dev/null
        settle
        switch_view list
        goto_row "$(row_index_of a.zip)"
        switch_view "$mode"
        key -k space >/dev/null
        for _attempt in $(seq 1 40); do [[ "$(ipc previewState)" == "archive" ]] && break; sleep 0.1; done
        [[ "$(ipc previewKind)" == "archive" && "$(ipc previewState)" == "archive" && "$(ipc previewArchiveNames)" == *"sample.txt"* ]] \
            || fail "$mode: Space on a.zip: kind $(ipc previewKind), state $(ipc previewState), members '$(ipc previewArchiveNames)'"
        preview_surface_lit "$mode-zip" 50 "a.zip's members"
        key -k Escape >/dev/null
        settle
        # The three other image formats through the original file, not a cached thumbnail: PreviewImage reads the file itself.
        for name in p.png p.webp p.heic; do
            switch_view list
            goto_row "$(row_index_of "$name")"
            switch_view "$mode"
            key -k space >/dev/null
            for _attempt in $(seq 1 60); do [[ "$(ipc previewState)" == "image" ]] && break; sleep 0.1; done
            [[ "$(ipc previewKind)" == "image" && "$(ipc previewState)" == "image" ]] || fail "$mode: Space on $name: kind $(ipc previewKind), state $(ipc previewState)"
            preview_surface_lit "$mode-$name" 200 "$name's picture"
            key -k Escape >/dev/null
            settle
        done
        for name in sample.txt main.rs; do
            switch_view list
            goto_row "$(row_index_of "$name")"
            switch_view "$mode"
            key -k space >/dev/null
            for _attempt in $(seq 1 40); do [[ "$(ipc previewState)" == "text" ]] && break; sleep 0.1; done
            [[ "$(ipc previewKind)" == "text" && "$(ipc previewText)" == *"$([[ $name == sample.txt ]] && echo 'hello from flea' || echo 'fn main')"* ]] \
                || fail "$mode: Space on $name: kind $(ipc previewKind), state $(ipc previewState), text '$(ipc previewText | cut -c1-40)'"
            preview_surface_lit "$mode-$name" 50 "$name's text"
            key -k Escape >/dev/null
            settle
        done
        # Audio and the three video containers: a player exists, its position advances, the picture moves, and Escape empties the loader.
        for name in tone.wav v.mp4 v.mkv v.webm; do
            switch_view list
            goto_row "$(row_index_of "$name")"
            switch_view "$mode"
            key -k space >/dev/null
            for _attempt in $(seq 1 40); do [[ "$(ipc previewState)" == "playing" ]] && break; sleep 0.1; done
            [[ "$(ipc previewMediaLoaded)" == "true" && "$(ipc previewState)" == "playing" ]] || fail "$mode: Space on $name: loaded $(ipc previewMediaLoaded), state $(ipc previewState)"
            sleep 0.8
            p1=$(ipc previewPosition)
            shot "previewviews-$mode-$name-1"
            sleep 1.2
            p2=$(ipc previewPosition)
            shot "previewviews-$mode-$name-2"
            (( p2 > p1 )) || fail "$mode: $name's position did not advance, $p1 then $p2"
            if [[ "$name" != tone.wav ]]; then
                read -r fx fy fw fh <<< "$(ipc previewSurfaceRect)"
                (( fw > 0 && fh > 0 )) || fail "$mode: $name's picture has no box"
                changed=$(magick \( "$evidence_dir/previewviews-$mode-$name-1.png" -crop "${fw}x${fh}+${fx}+${fy}" +repage \) \
                    \( "$evidence_dir/previewviews-$mode-$name-2.png" -crop "${fw}x${fh}+${fx}+${fy}" +repage \) \
                    -compose difference -composite -threshold 10% -format "%[fx:int(mean*w*h+0.5)]" info:)
                (( changed > 1000 )) || fail "$mode: $name's picture changed $changed pixels in 1.2 s of playback"
            fi
            key -k Escape >/dev/null
            settle
            [[ "$(ipc previewOpen)" == "false" && "$(ipc previewMediaLoaded)" == "false" ]] || fail "$mode: after Escape on $name, open $(ipc previewOpen), media loaded $(ipc previewMediaLoaded)"
        done
        # Bad then good in the image reader: an unreadable picture, then two rows up to a readable one.
        switch_view list
        goto_row "$(row_index_of shut.jpg)"
        switch_view "$mode"
        key -k space >/dev/null
        for _attempt in $(seq 1 40); do [[ "$(ipc previewState)" == "This image could not be read." ]] && break; sleep 0.1; done
        [[ "$(ipc previewState)" == "This image could not be read." ]] || fail "$mode: Space on shut.jpg reads '$(ipc previewState)'"
        key k >/dev/null
        key k >/dev/null
        for _attempt in $(seq 1 60); do [[ "$(ipc previewState)" == "image" ]] && break; sleep 0.1; done
        [[ "$(ipc previewState)" == "image" ]] || fail "$mode: the preview did not recover from shut.jpg to p.webp, state $(ipc previewState)"
        preview_surface_lit "$mode-recovered" 200 "the recovered picture"
        key -k Escape >/dev/null
        settle
        printf 'PREVIEWVIEWS %s image=4 pdf=ok error=ok archive=ok text=2 media=4 recovery=ok\n' "$mode"
    done
    # The column player and the two things that must end it: another view, and Space on the same file.
    switch_view list
    goto_row "$(row_index_of v.mp4)"
    switch_view columns
    sleep 1
    [[ "$(ipc previewColumnState)" == "video" ]] || fail "previewviews: the cursor on v.mp4 shows $(ipc previewColumnState)"
    read -r cx cy <<< "$(ipc columnPlayCentre)"
    omarchy-drive click "$((wx + cx))" "$((wy + cy))" left >/dev/null
    sleep 1.2
    [[ "$(ipc columnMediaPlaying)" == "true" ]] || fail "previewviews: play did not start in the column"
    switch_view list
    settle
    [[ "$(ipc columnPlayerLoaded)" == "false" ]] || fail "previewviews: the column player survived a switch to the list"
    switch_view columns
    sleep 1
    [[ "$(ipc columnPlayerLoaded)" == "false" ]] || fail "previewviews: a player came back with the view without a press"
    omarchy-drive click "$((wx + cx))" "$((wy + cy))" left >/dev/null
    sleep 1.2
    [[ "$(ipc columnMediaPlaying)" == "true" ]] || fail "previewviews: play did not restart in the column"
    key -k space >/dev/null
    for _attempt in $(seq 1 40); do [[ "$(ipc previewKind)" == "video" ]] && break; sleep 0.1; done
    sleep 1
    p1=$(ipc previewPosition)
    sleep 1
    p2=$(ipc previewPosition)
    [[ "$(ipc previewOpen)" == "true" && "$(ipc previewKind)" == "video" ]] && (( p2 > p1 )) || fail "previewviews: Space over the playing column: open $(ipc previewOpen), kind $(ipc previewKind), position $p1 then $p2"
    [[ "$(ipc columnPlayerLoaded)" == "false" ]] || fail "previewviews: the column kept its player under the Space preview"
    key -k Escape >/dev/null
    settle
    [[ "$(ipc previewOpen)" == "false" && "$(ipc columnPlayerLoaded)" == "false" ]] || fail "previewviews: after Escape, preview $(ipc previewOpen), column player $(ipc columnPlayerLoaded)"
    printf 'PREVIEWVIEWS lifetimes=ok\n'
    kill_flea
}
