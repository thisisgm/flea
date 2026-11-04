# GPUI overrides for UISH-16; seeded from the unchanged oracle at 2faf8d099.
case_open() {
    local dir="$fixture_root/open"
    sandbox_scratch "$dir"
    mkdir -p "$dir/subdir" "$dir/bin"
    printf 'abc' > "$dir/target.txt"
    ln -s "$dir/target.txt" "$dir/linkfile"
    ln -s "$dir/subdir" "$dir/linkdir"
    ln -s "$dir/nowhere" "$dir/broken"
    # A real archive, because the archive arm below reads the backend's own index of it and not a name.
    bsdtar -a -c -f "$dir/sample.zip" -C "$dir" target.txt
    local opened="$dir/opened.log"
    : > "$opened"
    # Only the open subcommand is intercepted, so stubbing the opener leaves the gio mount calls
    # ui/NetworkMounts.qml makes on every launch answering from the real gio. That name is the mount
    # tool's own and is spelled by hand here; the stub's name is derived from src/open.rs instead.
    {
      printf '#!/bin/sh\n'
      printf '[ "$1" = open ] || exec /usr/bin/gio "$@"\n'
      printf 'printf "OPENED %%s\\n" "$2" >> %q\n' "$opened"
    } > "$dir/bin/$open_handoff"
    chmod +x "$dir/bin/$open_handoff"

    local saved_path="$PATH"
    export PATH="$dir/bin:$PATH"
    launch "$dir"
    export PATH="$saved_path"
    # Measured row order: bin, subdir, broken, linkdir, linkfile, opened.log, sample.zip, target.txt.
    wait_listing 8

    # Bare l enters a real directory and stays silent when its empty listing has no row.
    seek_row_named subdir
    key l >/dev/null
    wait_path "$dir/subdir"
    flea_ipc_wait -p "$flea_ui/boot" flea state empty --timeout 10 >/dev/null \
        || fail "open: subdir never reached its empty listing, state is $(ipc state)"
    [[ ! -s "$opened" ]] || fail "l on a directory handed $(cat "$opened") to $open_handoff open"
    [[ -z "$(ipc lastMessage)" ]] || fail "open: entering the empty subdir said $(ipc lastMessage)"
    key l >/dev/null
    settle
    [[ -z "$(ipc lastMessage)" ]] || fail "open: l on an empty directory said $(ipc lastMessage)"
    key q >/dev/null
    settle
    [[ "$(ipc lastMessage)" == "Press / to filter this folder by name" ]] \
        || fail "open: unbound q said $(ipc lastMessage), so the empty-row l check has no negative control"

    # Bare l enters a symlink to a directory without handing it to the opener.
    key -k Backspace >/dev/null
    wait_path "$dir"
    seek_row_named linkdir
    key l >/dev/null
    wait_path "$dir/linkdir"
    [[ ! -s "$opened" ]] || fail "l on a symlink directory handed $(cat "$opened") to $open_handoff open"

    # Return on a symlink to a directory keeps its existing navigation and opener coverage.
    key -k Backspace >/dev/null
    wait_path "$dir"
    seek_row_named linkdir
    key -k Return >/dev/null
    wait_path "$dir/linkdir"
    [[ ! -s "$opened" ]] || fail "Enter on a symlink directory handed $(cat "$opened") to $open_handoff open"

    # Return on a symlink to a file still resolves and opens its target.
    key -k Backspace >/dev/null
    wait_path "$dir"
    seek_row_named linkfile
    settle
    [[ "$(ipc rowAt "$(ipc cursor)")" == "linkfile|"* ]] || fail "the cursor is on $(ipc rowAt "$(ipc cursor)"), not linkfile"
    key -k Return >/dev/null
    local waited
    for waited in $(seq 1 100); do
        grep -q "^OPENED $dir/target.txt$" "$opened" && break
        sleep 0.05
    done
    printf 'OPEN log=%q path=%q message=%q\n' "$(cat "$opened")" "$(ipc path)" "$(ipc lastMessage)"
    grep -q "^OPENED $dir/target.txt$" "$opened" || fail "Enter on a symlink did not open its target"
    [[ "$(ipc path)" == "$dir" ]] || fail "Enter on a file left the directory for $(ipc path)"

    # A broken symlink is one sentence and nothing else.
    key g >/dev/null
    key -k Down >/dev/null
    key -k Down >/dev/null
    settle
    [[ "$(ipc rowAt "$(ipc cursor)")" == "broken|"* ]] || fail "the cursor is on $(ipc rowAt "$(ipc cursor)"), not broken"
    key -k Return >/dev/null
    settle
    printf 'OPEN broken message=%q path=%q log=%q\n' "$(ipc lastMessage)" "$(ipc path)" "$(cat "$opened")"
    shot open-broken
    [[ -n "$(ipc lastMessage)" ]] || fail "Enter on a broken symlink said nothing"
    [[ "$(ipc path)" == "$dir" ]] || fail "Enter on a broken symlink moved to $(ipc path)"
    [[ "$(grep -c OPENED "$opened")" == "1" ]] || fail "a broken symlink was handed to $open_handoff open"
    [[ "$(ipc total)" == "8" ]] || fail "the listing did not survive Enter on a broken symlink"

    # The operator's 0.1.4 ruling: Enter on an archive opens Flea's own view and hands nothing on,
    # because every archive type this box can name defaults to org.gnome.Nautilus.desktop.
    seek_row_named sample.zip
    key -k Return >/dev/null
    flea_ipc_wait -p "$flea_ui/boot" flea previewState archive --timeout 10 >/dev/null \
        || fail "Enter on an archive left the preview at $(ipc previewState), kind $(ipc previewKind), and the log holds $(cat "$opened")"
    printf 'OPEN archive kind=%q state=%q log=%q\n' "$(ipc previewKind)" "$(ipc previewState)" "$(cat "$opened")"
    shot open-archive
    [[ "$(grep -c OPENED "$opened")" == "1" ]] || fail "Enter on an archive handed $(cat "$opened") to $open_handoff open"
    [[ "$(ipc path)" == "$dir" ]] || fail "Enter on an archive left the directory for $(ipc path)"
    key -k Escape >/dev/null
    settle
    [[ "$(ipc previewOpen)" == "false" ]] || fail "Escape left the archive preview open"
}

case_preview() {
    command -v ffmpeg >/dev/null || fail "ffmpeg is missing, so the mp4 fixture cannot be built"
    local dir="$fixture_root/preview"
    sandbox_scratch "$dir"
    mkdir -p "$dir/bin"
    # The double click below opens the row, so notes.md would hit the real gio open without this stub, the same hazard case_open already fixed.
    local opened="$dir/opened.log"
    : > "$opened"
    # Only the open subcommand is intercepted, so stubbing the opener leaves the gio mount calls
    # ui/NetworkMounts.qml makes on every launch answering from the real gio. That name is the mount
    # tool's own and is spelled by hand here; the stub's name is derived from src/open.rs instead.
    {
      printf '#!/bin/sh\n'
      printf '[ "$1" = open ] || exec /usr/bin/gio "$@"\n'
      printf 'printf "OPENED %%s\\n" "$2" >> %q\n' "$opened"
    } > "$dir/bin/$open_handoff"
    chmod +x "$dir/bin/$open_handoff"
    printf 'hello from flea\n' > "$dir/sample.txt"
    printf '# Notes\n\nSome *text*.\n' > "$dir/notes.md"
    truncate -s 2M "$dir/big.txt"
    # A 440 Hz tone and not silence, so playback is provable by ear and not just by state. Fifteen
    # seconds, not one: an omarchy-drive ipc round trip costs 190 to 565 ms measured on this box
    # (and more under load), so a one-second clip leaves the poll below one or two
    # samples to land the Playing state in, and this case caught that exact miss before the fix.
    # Task 22's own Right/Left checks need headroom on both sides of a 5 s seek (the SEEK_MS in
    # ui/js/PreviewKeys.js) starting from whatever position the round trips above already spent:
    # at three seconds the clip had finished before the checks ran at all, and at eight, Right alone
    # landed within 5 s of the end, clamped to it, and stopped the player before Left ran.
    clip_seconds=15
    python3 - "$dir/tone.wav" "$clip_seconds" <<'PYEOF'
import sys, wave, struct, math
sample_rate = 44100
seconds = int(sys.argv[2])
with wave.open(sys.argv[1], "w") as f:
    f.setnchannels(1)
    f.setsampwidth(2)
    f.setframerate(sample_rate)
    for i in range(sample_rate * seconds):
        sample = int(16000 * math.sin(2 * math.pi * 440 * i / sample_rate))
        f.writeframesraw(struct.pack("<h", sample))
PYEOF
    ffmpeg -y -f lavfi -i "testsrc=duration=$clip_seconds:size=64x64:rate=10" "$dir/clip.mp4" >/dev/null 2>&1
    [[ -s "$dir/clip.mp4" ]] || fail "ffmpeg produced no clip.mp4"
    # Space on a pdf answered "This file cannot be previewed" until kindOf gained its branch, on a
    # type the preview column had rendered all along, so this row is what guards that branch.
    magick \( -size 400x560 xc:white -fill black "${flea_magick_font_args[@]}" -pointsize 40 -annotate +40+80 'PAGEONE' \) \
           \( -size 400x560 xc:white -fill black "${flea_magick_font_args[@]}" -pointsize 40 -annotate +40+80 'PAGETWO' \) \
           "$dir/manual.pdf"
    [[ -s "$dir/manual.pdf" ]] || fail "magick produced no manual.pdf"

    local saved_path="$PATH"
    export PATH="$dir/bin:$PATH"
    launch "$dir"
    export PATH="$saved_path"
    # bin/ and opened.log are the gio stub's own fixture entries, alongside the six under test.
    wait_listing 8

    # A single click must move the cursor and nothing else: no preview, and since 2026-09-02 no open
    # either. The double click below is the negative control that the stub can see one at all.
    [[ "$(ipc previewOpen)" == "false" ]] || fail "preview: open at launch, before any interaction"
    click_row "$(row_index_of notes.md)" left
    settle
    [[ "$(ipc cursor)" == "$(row_index_of notes.md)" ]] || fail "preview: click did not move the cursor to notes.md"
    [[ "$(ipc previewOpen)" == "false" ]] || fail "preview: a left click opened the preview"
    [[ ! -s "$opened" ]] || fail "preview: a single left click opened $(cat "$opened")"
    click_row "$(row_index_of notes.md)" left --double
    for _attempt in $(seq 1 100); do
        grep -q "^OPENED $dir/notes.md$" "$opened" && break
        sleep 0.05
    done
    grep -q "^OPENED $dir/notes.md$" "$opened" || fail "preview: a double click never reached the real-open path the stub is guarding"
    [[ "$(ipc previewOpen)" == "false" ]] || fail "preview: the double click opened the preview rather than the file"
    # Hover is a plain pointer move, no button, over a different row than the click landed on.
    read -r hx hy <<< "$(ipc rowCentre "$(row_index_of big.txt)")"
    read -r wx wy ww wh < <(window_box) || fail "native window coordinates unavailable"
    omarchy-drive move "$((hx + wx))" "$((hy + wy))" >/dev/null
    settle
    [[ "$(ipc previewOpen)" == "false" ]] || fail "preview: hovering a row opened the preview"
    # Negative control: Space still opens it, so the two checks above proved a real absence, not a broken previewOpen read.
    key -k space >/dev/null
    settle
    [[ "$(ipc previewOpen)" == "true" ]] || fail "preview: space stopped opening the preview after the click/hover checks"
    key -k Escape >/dev/null
    settle
    [[ "$(ipc previewOpen)" == "false" ]] || fail "preview: escape did not close the preview after the negative control"

    goto_row "$(row_index_of sample.txt)"
    key l >/dev/null
    settle
    [[ "$(ipc previewOpen)" == "true" ]] || fail "preview: l on sample.txt did not open a preview"
    [[ "$(ipc previewKind)" == "text" ]] || fail "preview: sample.txt classified as $(ipc previewKind), not text"
    shot preview-text
    # Task 22 repurposes Space for play/pause only on a MEDIA preview; text keeps the old binding.
    key -k space >/dev/null
    settle
    [[ "$(ipc previewOpen)" == "false" ]] || fail "preview: space did not close the text preview, the media-only rebinding leaked into text"

    open_row notes.md
    # Markdown renders verbatim like any other text, so the kind is text and there is no second path.
    [[ "$(ipc previewKind)" == "text" ]] || fail "preview: notes.md classified as $(ipc previewKind), not text"
    shot preview-markdown
    key -k Escape >/dev/null
    settle

    open_row_fast tone.wav
    [[ "$(ipc previewKind)" == "audio" ]] || fail "preview: tone.wav classified as $(ipc previewKind), not audio"
    wait_preview_state playing
    shot preview-audio

    # Left/Right seek 5 s (PreviewKeys.js's SEEK_MS), read before the pause/resume dance below spends
    # its own several IPC round trips (190 to 565 ms each, see the clip_seconds comment above): an
    # eight-second clip still has room left once this runs, so both directions are unclamped.
    local pos_before pos_after
    pos_before=$(ipc previewPosition)
    key -k Right >/dev/null
    settle
    pos_after=$(ipc previewPosition)
    (( pos_after > pos_before )) \
        || fail "preview: Right did not move tone.wav forward, before=$pos_before after=$pos_after"
    pos_before=$pos_after
    key -k Left >/dev/null
    settle
    pos_after=$(ipc previewPosition)
    (( pos_after < pos_before )) \
        || fail "preview: Left did not move tone.wav back, before=$pos_before after=$pos_after"

    # Decided 2026-09-11: space closes every kind, media included, and playback moved to its own p, which
    # ui/js/PreviewKeys.js reaches only in the media context. This block is that ruling, not Task 22's.
    key p >/dev/null
    wait_preview_state paused
    [[ "$(ipc previewOpen)" == "true" ]] || fail "preview: p paused tone.wav but also closed the preview"
    pos_before=$(ipc previewPosition)
    key l >/dev/null
    settle
    pos_after=$(ipc previewPosition)
    [[ "$(ipc previewOpen)" == "true" ]] || fail "preview: l closed the paused audio preview"
    [[ "$(ipc previewState)" == "paused" ]] || fail "preview: l changed paused audio to $(ipc previewState)"
    [[ "$pos_after" == "$pos_before" ]] \
        || fail "preview: l moved paused audio, before=$pos_before after=$pos_after"
    key p >/dev/null
    wait_preview_state playing

    # And the second space closes a playing media preview, the way it closes every other kind.
    key -k space >/dev/null
    settle
    [[ "$(ipc previewOpen)" == "false" ]] || fail "preview: space did not close the playing audio preview"

    open_row_fast clip.mp4
    [[ "$(ipc previewKind)" == "video" ]] || fail "preview: clip.mp4 classified as $(ipc previewKind), not video"
    wait_preview_state playing
    shot preview-video
    [[ "$(ipc previewStrip | jq -r .visible)" == "true" ]] \
        || fail "preview: the strip is not visible right after opening the video preview"

    # Fix round 1: the reviewer's finding was that PanelSlider's own MouseArea swallows pointer
    # activity during a scrub or a wheel-seek, so it never reached the two outer hover MouseAreas
    # that call revealStrip(); the fix relays the slider's own moved signal to revealStrip() too
    # (PanelSlider emits it for both a drag in progress and a wheel step). Live isolation of that
    # one line proved impractical on this harness: a temporary signal-level trace (console.log on
    # revealStrip, A/B'd against a git-diff-isolated build with only the onMoved line removed)
    # showed the two outer hover MouseAreas already reveal the strip for both a wheel scroll and
    # an in-place drag on the slider, fix present or not, because Qt Quick delivers hover position
    # updates to every hoverEnabled MouseArea under the pointer independent of which item currently
    # holds the press grab, and the slider sits entirely inside mediaStrip's own hover area by
    # construction; see the fix round report for the full trace. What this still protects, real
    # end-to-end contract regardless of which code path currently satisfies it: interacting with
    # the slider keeps the strip visible past the point it would otherwise auto-hide, and it still
    # genuinely hides once both the interaction and the pointer itself have stopped. SECONDS is
    # bash's own elapsed-seconds counter, reset here and read nowhere else in this file.
    read -r slx sly <<< "$(ipc previewSliderCentre)"
    [[ -n "$slx" ]] || fail "preview: the seek slider reported no on-screen centre"
    read -r wx wy ww wh < <(window_box) || fail "native window coordinates unavailable"
    omarchy-drive move "$((slx + wx))" "$((sly + wy))" >/dev/null

    SECONDS=0
    while (( SECONDS < 3 )); do sleep 0.2; done
    omarchy-drive scroll up 1 >/dev/null
    local interact_at=$SECONDS
    # A pointer left resting anywhere over the overlay keeps re-triggering the hover MouseAreas
    # on its own with no further input (measured live: positionChanged fires repeatedly for a
    # motionless pointer too, likely the compositor's own periodic re-affirm), which would make
    # the "still eventually hides" check below meaningless; moving off the window stops that.
    omarchy-drive move 5 5 >/dev/null
    while (( SECONDS < 6 )); do sleep 0.2; done
    [[ "$(ipc previewStrip | jq -r .visible)" == "true" ]] \
        || fail "preview: the strip hid before the slider interaction's own stripHideMs window expired"
    while (( SECONDS < interact_at + 5 )); do sleep 0.2; done
    [[ "$(ipc previewStrip | jq -r .visible)" == "false" ]] \
        || fail "preview: the strip never auto-hid once the slider interaction's own window expired"

    key -k Escape >/dev/null
    settle

    open_row manual.pdf
    [[ "$(ipc previewKind)" == "pdf" ]] \
        || fail "preview: manual.pdf classified as $(ipc previewKind), not pdf"
    [[ "$(ipc previewState)" == "pdf" ]] \
        || fail "preview: the pdf overlay reports $(ipc previewState), so it fell through to the refusal"
    [[ "$(ipc previewPdfPage)" == "0" ]] || fail "preview: manual.pdf opened on page $(ipc previewPdfPage), not page 0"
    key l >/dev/null
    flea_ipc_wait -p "$flea_ui/boot" flea previewPdfPage 1 --timeout 10 >/dev/null \
        || fail "preview: l left manual.pdf on page $(ipc previewPdfPage), not page 1"
    omarchy-drive wait ocr flea PAGETWO --timeout 10 >/dev/null \
        || fail "preview: l advanced manual.pdf state but left page 1 painted"
    shot preview-pdf
    key -k Escape >/dev/null
    settle
    [[ "$(ipc previewOpen)" == "false" ]] || fail "preview: escape did not close the pdf overlay"

    open_row big.txt
    [[ "$(ipc previewKind)" == "text" ]] || fail "preview: big.txt classified as $(ipc previewKind), not text"
    [[ "$(ipc previewState)" == "This file is too large to preview." ]] \
        || fail "preview: big.txt did not report the too-large sentence, state=$(ipc previewState)"
    shot preview-toolarge
    key -k Escape >/dev/null
    settle
    [[ "$(ipc previewOpen)" == "false" ]] || fail "preview: escape did not close the final preview"

    # The operator's ruling of 2026-09-11: escape was the only way out of a preview. A click on the
    # overlay's ground closes it, and a click on the surface still belongs to whatever pane is drawn
    # there. Both halves are asserted, because a shield that closes on every click is the same defect
    # in the other direction.
    local pwx pwy pww pwh
    open_row big.txt
    [[ "$(ipc previewOpen)" == "true" ]] || fail "preview: the click-away fixture did not open"
    read -r pwx pwy pww pwh < <(window_box) || fail "preview: native window coordinates unavailable"
    # The surface is a centred fraction of the window, so a point near the top-left corner is ground.
    omarchy-drive click "$((pwx + 40))" "$((pwy + 140))" >/dev/null
    settle
    [[ "$(ipc previewOpen)" == "false" ]] || fail "preview: a click on the ground did not close it"
    open_row big.txt
    [[ "$(ipc previewOpen)" == "true" ]] || fail "preview: it did not reopen for the surface check"
    omarchy-drive click "$((pwx + pww / 2))" "$((pwy + pwh / 2))" >/dev/null
    settle
    [[ "$(ipc previewOpen)" == "true" ]] || fail "preview: a click on the surface closed it"
    key -k Escape >/dev/null
    settle
    [[ "$(ipc previewOpen)" == "false" ]] || fail "preview: escape did not close the click-away preview"

    printf 'PREVIEW text=ok markdown=ok audio=ok video=ok toolarge=ok mediacontrols=ok striphide=ok clickaway=ok\n'
    kill_flea
}

case_lookopen() {
    local dir="$fixture_root/lookopen" view name want wait
    sandbox_scratch "$dir"
    magick \( -size 440x354 xc:white -fill black "${flea_magick_font_args[@]}" -pointsize 24 -annotate +30+45 PAGEONE \) \
           \( -size 440x354 xc:white -fill black "${flea_magick_font_args[@]}" -pointsize 24 -annotate +30+45 PAGETWO \) "$dir/a-doc.pdf" \
        || fail "lookopen: magick could not make a-doc.pdf"
    ffmpeg -y -f lavfi -i "testsrc=size=320x240" -frames:v 1 "$dir/b-image.jpg" >/dev/null 2>&1 || fail "lookopen: ffmpeg could not make b-image.jpg"
    printf 'hello from flea\n' > "$dir/c-notes.txt"
    ffmpeg -y -f lavfi -i "testsrc=size=8000x8000" -frames:v 1 "$dir/d-large.png" >/dev/null 2>&1 || fail "lookopen: ffmpeg could not make d-large.png"
    for view in list columns; do
        lookopen_launch "$dir" "$view" default "{\"keys\":\"default\",\"view\":\"$view\"}"
        for name in a-doc.pdf b-image.jpg c-notes.txt; do lookopen_step "$view" "$name" 0; done
        lookopen_launch "$dir" "$view" thumbsoff "{\"keys\":\"default\",\"view\":\"$view\",\"preview\":{\"thumbnails\":\"off\"}}"
        if [[ "$view" == columns ]]; then want=0; wait=1; else want=1; wait=0; fi
        lookopen_step "$view" d-large.png "$want" "$wait"
        lookopen_reopen "$view" d-large.png "$want"
        kill_flea
    done
}

case_relstart() {
    local dir="$fixture_root/relstart" real
    sandbox_scratch "$dir"
    mkdir -p "$dir/sub"
    real=$(cd "$dir" && pwd -P)
    magick -size 320x240 xc:'#3070c0' "$dir/a-photo.png" || fail "relstart: magick could not make a-photo.png"
    magick -size 440x354 xc:white -fill black "${flea_magick_font_args[@]}" -pointsize 24 -annotate +30+45 PAGEONE "$dir/b-doc.pdf" \
        || fail "relstart: magick could not make b-doc.pdf"
    magick -size 240x320 xc:'#c07030' "$dir/sub/c-photo.png" || fail "relstart: magick could not make c-photo.png"
    # The relative start stays relative to the case's cwd, and the launch records its PID here, not in a subshell.
    pushd "$dir" >/dev/null || fail "relstart: cannot enter $dir"
    launch .
    popd >/dev/null || fail "relstart: cannot leave $dir"
    wait_listing 3
    settle
    [[ "$(ipc path)" == "$real" ]] || fail "relstart: a start of . opened $(ipc path), not $real"
    relstart_look a-photo.png image
    relstart_look b-doc.pdf pdf
    seek_row_named sub
    key -k Return >/dev/null
    wait_path "$real/sub"
    wait_listing 1
    settle
    relstart_look c-photo.png image
    printf 'RELSTART path=%s\n' "$real"
    kill_flea
}
