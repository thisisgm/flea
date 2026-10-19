# Sourced by ui.sh beside the other case files, but kept out of the default wanted
# list: it runs only by name. Opens a markdown fixture in Quick Look rendered, flips to
# Source and back with r, closes, then shoots the preview column with the file under the
# cursor. All fixtures and writes stay in its marked sandbox.
# case_cap_markdown_kinds, below, shoots the other Markdown kinds the same way, also by name only.
# case_cap_markdown_fences shoots every fence shape in Quick Look from each view, in the preview column and in Dual, also by name only.
# case_cap_markdown_fences14 repeats it at text size 14 with more fence shapes and logs what Quick Look built for every shot, also by name only.
# case_cap_markdown_pele shoots one figure of every Mermaid diagram type at text size 14, in Quick Look and in the preview column, also by name only.
# Figures settle after the preview opens, so the shot waits for them.
capmarkdown_wait_figures() {
    local figs=""
    for _attempt in $(seq 1 60); do
        figs="$(ipc previewFigures)"
        if [[ -n "$figs" && "$figs" != *working* && "$figs" != *idle* && "$figs" != *deferred* ]]; then
            printf '%s\n' "$figs"
            return 0
        fi
        sleep 0.2
    done
    fail "capmarkdown: figures never settled, last saw [$figs]"
}
capmarkdown_wait_column_rendered() {
    local view="" rendered_poll_attempts=40 rendered_poll_seconds=0.1
    for _attempt in $(seq 1 "$rendered_poll_attempts"); do
        view="$(ipc columnMarkdownView)"
        [[ "$view" == "rendered" ]] && return 0
        sleep "$rendered_poll_seconds"
    done
    fail "capmarkdown: column Markdown never rendered after the Quick Look flip, last saw [$view]"
}
# Sample input: previewSurfaceRect "0 54 1268 1240" (the body under the bar), previewCloseState {"hovered":false,"pressed":false,"centre":"1244 40"}; window pixels to the document's centre or the close button's.
capmarkdown_pointer() {
    local where="$1" sx sy sw sh wx wy _ww _wh px py
    read -r wx wy _ww _wh < <(window_box) || fail "capmarkdown: native window coordinates unavailable"
    if [[ "$where" == close ]]; then
        read -r px py <<< "$(ipc previewCloseState | jq -r '.centre // empty' 2>/dev/null)"
        [[ -n "${py:-}" ]] || fail "capmarkdown: the close button never reported its centre"
    elif [[ "$where" == table ]]; then
        # The table's own rects over IPC: the middle of its drawn width, on the middle of its first body row.
        local view row _a _b rw rh rx ry
        view="$(ipc previewTable | jq -r '.view // empty')"; row="$(ipc previewTable | jq -r '.row // empty')"
        [[ -n "$view" && -n "$row" ]] || fail "capmarkdown: no table reported its rects over IPC"
        read -r sx _a sw _b <<< "$view"
        read -r rx ry rw rh <<< "$row"
        px=$((sx + sw / 2)); py=$((ry + rh / 2))
    else
        read -r sx sy sw sh <<< "$(ipc previewSurfaceRect)"
        [[ -n "${sh:-}" ]] || fail "capmarkdown: the Quick Look surface never reported its rect"
        px=$((sx + sw / 2)); py=$((sy + sh / 2))
    fi
    px=$((wx + px)); py=$((wy + py))
    # Two moves so the first lands as the resting point, then a seat nudge there and back, since Hyprland's cursor move sends Qt no pointer frame.
    omarchy-drive move "$((px - capmarkdown_nudge_px * capmarkdown_approach_nudges))" "$py" >/dev/null || fail "capmarkdown: pointer approach to the $where failed"
    omarchy-drive move "$px" "$py" >/dev/null || fail "capmarkdown: pointer move to the $where failed"
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool mousemove -x "$capmarkdown_nudge_px" -y 0 >/dev/null 2>&1 || fail "capmarkdown: pointer nudge failed"
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool mousemove -x "-$capmarkdown_nudge_px" -y 0 >/dev/null 2>&1 || fail "capmarkdown: pointer nudge back failed"
    settle
    [[ "$where" != close ]] || capmarkdown_wait_close hovered true
}
# The close button's own pointer state, polled until it reads as wanted: a shot of a hover or a press is taken only once it holds.
capmarkdown_wait_close() {
    local field="$1" want="$2" state="" end
    end=$(( $(date +%s%3N) + capmarkdown_close_wait_s * 1000 ))
    while (( $(date +%s%3N) < end )); do
        state="$(ipc previewCloseState)"
        [[ "$(jq -r --arg field "$field" '.[$field]' <<< "$state" 2>/dev/null)" == "$want" ]] && return 0
        sleep "$capmarkdown_close_poll_s"
    done
    fail "capmarkdown: the close button never reported $field=$want, last [$state]"
}
# One wheel step run, its exit status kept, and the view proved to have moved: a shot taken after it is a scrolled shot.
capmarkdown_scroll() {
    local direction="$1" notches="$2" before after
    before="$(ipc previewScrollY)"
    omarchy-drive scroll "$direction" "$notches" >/dev/null || fail "capmarkdown: scroll $direction $notches failed"
    settle
    after="$(ipc previewScrollY)"
    [[ -n "$after" && "$after" != "$before" ]] || fail "capmarkdown: the wheel did not move the view, scrollY stayed [$before] after scroll $direction $notches"
}
# At the end of the document the last block and the inset under it lie inside the viewport, at most this many px past it (rounding).
capmarkdown_end_slack_px=1
# Sample input: previewEndGap answers 0 when the last block and its inset are whole in the view, 80 when a picture grew 80 px below it, -1 when the last block is not built.
capmarkdown_end_fit() {
    local before after
    before="$(ipc previewEndGap)"
    settle
    after="$(ipc previewEndGap)"
    [[ "$before" =~ ^[0-9]+$ && "$after" =~ ^[0-9]+$ && "$before" -le "$capmarkdown_end_slack_px" && "$after" -le "$capmarkdown_end_slack_px" ]] \
        || fail "capmarkdown: the last block is cut at the end of the document, previewEndGap read [$before] then [$after]"
}
# One px each way is enough for Hyprland to send Qt a pointer frame.
capmarkdown_nudge_px=1
# The approach starts this many nudges short of the target, so the second move is a real motion onto it.
capmarkdown_approach_nudges=6
# A hover or a press must read as wanted within this many seconds, read again after each pause.
capmarkdown_close_wait_s=5
capmarkdown_close_poll_s=0.05
# A notch is 288 px: three reach the figures region below the quote and tables, twelve more clamp at the tail with the picture and the placeholder.
capmarkdown_notches_mid=3
# The wheel notches that scroll a table sideways to its far columns; a notch is 288 px and overflow.md runs several thousand past the card.
capmarkdown_notches_side=20
# What the last sideways step found wrong, kept so the other shots are still taken and the case fails once they are.
capmarkdown_sideways_error=""
capmarkdown_notches_end=12
# The column is at its end when viewContentY is within this many pixels of viewEndY.
capmarkdown_end_slack=1
# Paragraphs of about 40 px rendered and two source lines each, so both views overflow a 2560 x 1440 card by more than the mid notches.
capmarkdown_notes=60
case_cap_markdown() {
    local dir="$fixture_root/capmarkdown" figs=""
    sandbox_scratch "$dir"
    mkdir -p "$dir/listing"
    cat > "$dir/listing/notes.md" <<'EOF'
# Rendered notes

## Second level

A paragraph with `loadFile()` inline code and [a guide](https://example.com/guide).

> A quoted line for the bar.

1. First item
2. Second item

- Alpha item
- Beta item

| Kind | Asks for | Cached |
| :--- | :--- | :--- |
| rows | the cursor | yes |
| facts | the table | yes |

```js
var fenced = true;
```

```mermaid
flowchart TD
    A --> B
```

```mermaid
sequenceDiagram
    A->>B: hi
```

$$x^2$$

```math
\frac{a}{b}
```

```mermaid
not a diagram {{{
```

## Notes
EOF
    for _note in $(seq 1 "$capmarkdown_notes"); do
        printf '\nNote %s of the fixture, plain text that only gives the document and its source some height.\n' "$_note" >> "$dir/listing/notes.md"
    done
    cat >> "$dir/listing/notes.md" <<'EOF'

A paragraph with $x^2$ inline maths and $5 and $10 prices.

![bench](./bench.png)

![shot](https://cdn.example.com/shot.png)
EOF
    # The board's 160 by 80 stand-in, written inside the fixture sandbox only.
    python3 - "$dir/listing/bench.png" <<'PY'
import struct, sys, zlib
def chunk(tag, body):
    return struct.pack('>I', len(body)) + tag + body + struct.pack('>I', zlib.crc32(tag + body) & 0xffffffff)
png = b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', 160, 80, 8, 2, 0, 0, 0))
png += chunk(b'IDAT', zlib.compress((b'\0' + b'\x40\x80\xc0' * 160) * 80)) + chunk(b'IEND', b'')
open(sys.argv[1], 'wb').write(png)
PY
    launch "$dir/listing"
    # The fixture holds one file, so the listing is waited for at the count the fixture
    # creates rather than a literal carried from a larger fixture.
    wait_listing "$(find "$dir/listing" -mindepth 1 -maxdepth 1 | wc -l | tr -d ' ')"
    goto_row "$(row_index_of notes.md)"
    key -k Space >/dev/null
    for _attempt in $(seq 1 40); do [[ "$(ipc previewOpen)" == "true" ]] && break; sleep 0.1; done
    [[ "$(ipc previewOpen)" == "true" ]] || fail "capmarkdown: Space did not open Quick Look on notes.md"
    settle
    figs="$(capmarkdown_wait_figures)"
    [[ "$(printf '%s' "$figs" | grep -o 'ready' | wc -l | tr -d ' ')" == "4" ]] || fail "capmarkdown: want 4 ready figures, saw [$figs]"
    [[ "$(printf '%s' "$figs" | grep -o 'failed' | wc -l | tr -d ' ')" == "1" ]] || fail "capmarkdown: want 1 failed figure, saw [$figs]"
    shot "cap-markdown-rendered"
    capmarkdown_wait_close focused false
    # Rendered scrolled: the wheel shows the scroll bar on use, so each shot holds it.
    capmarkdown_pointer document
    capmarkdown_scroll down "$capmarkdown_notches_mid"
    shot "cap-markdown-rendered-scrolled"
    capmarkdown_scroll down "$capmarkdown_notches_end"
    capmarkdown_end_fit
    shot "cap-markdown-rendered-end"
    capmarkdown_scroll up "$((capmarkdown_notches_mid + capmarkdown_notches_end))"
    # The close button in each state the bar can show: hover, keyboard focus after Tab, then pressed and released off the button.
    capmarkdown_pointer close
    shot "cap-markdown-close-hover"
    capmarkdown_pointer document
    key -k Tab >/dev/null
    settle
    capmarkdown_wait_close focused true
    shot "cap-markdown-close-focus"
    key -k Tab >/dev/null
    capmarkdown_wait_close focused false
    capmarkdown_pointer close
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool click 0x40 >/dev/null 2>&1 || fail "capmarkdown: pointer press on the close button failed"
    settle
    capmarkdown_wait_close pressed true
    shot "cap-markdown-close-press"
    capmarkdown_pointer document
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool click 0x80 >/dev/null 2>&1 || fail "capmarkdown: pointer release failed"
    settle
    [[ "$(ipc previewOpen)" == "true" ]] || fail "capmarkdown: a press released off the close button closed Quick Look"
    # The control: a press and release both on the close button closes, then Space opens the document again for the Source steps.
    capmarkdown_pointer close
    YDOTOOL_SOCKET="$XDG_RUNTIME_DIR/.ydotool_socket" ydotool click 0xC0 >/dev/null 2>&1 || fail "capmarkdown: pointer click on the close button failed"
    settle
    [[ "$(ipc previewOpen)" == "false" ]] || fail "capmarkdown: a press and release on the close button did not close Quick Look"
    key -k Space >/dev/null
    for _attempt in $(seq 1 40); do [[ "$(ipc previewOpen)" == "true" ]] && break; sleep 0.1; done
    [[ "$(ipc previewOpen)" == "true" ]] || fail "capmarkdown: Space did not reopen Quick Look after the close button closed it"
    settle
    capmarkdown_wait_figures >/dev/null
    key r >/dev/null
    settle
    shot "cap-markdown-source"
    # The close click left the pointer on the bar, so the wheel goes back over the document first.
    capmarkdown_pointer document
    capmarkdown_scroll down "$capmarkdown_notches_mid"
    shot "cap-markdown-source-scrolled"
    key r >/dev/null
    settle
    capmarkdown_wait_figures >/dev/null
    shot "cap-markdown-rendered-again"
    key -k Escape >/dev/null
    settle
    [[ "$(ipc previewOpen)" == "false" ]] || fail "capmarkdown: Escape did not close Quick Look"
    switch_view columns
    goto_row "$(row_index_of notes.md)"
    settle
    shot "cap-markdown-column"
    key -k Space >/dev/null
    for _attempt in $(seq 1 40); do [[ "$(ipc previewOpen)" == "true" ]] && break; sleep 0.1; done
    [[ "$(ipc previewOpen)" == "true" ]] || fail "capmarkdown: Space did not reopen Quick Look from columns"
    key r >/dev/null
    settle
    key -k Escape >/dev/null
    settle
    [[ "$(ipc previewOpen)" == "false" ]] || fail "capmarkdown: Escape did not close Source Quick Look"
    capmarkdown_wait_column_rendered
    shot "cap-markdown-column-after-flip"
    capmarkdown_column_reach_end "notes.md"
    shot "cap-markdown-column-end"
    printf 'CAPMARKDOWN quicklook=ok source=ok column=ok column-after-flip=ok column-end=ok\n'
    kill_flea
}
# The 96 by 48 PNG of four colour bands both the README logo and the figures document point at.
capmarkdownkinds_png_b64='iVBORw0KGgoAAAANSUhEUgAAAGAAAAAwCAIAAABhdOiYAAAAaElEQVR42u3QMQ0AIAwAMIShZEoQwc3NPSe4wsG+fU2qoOOtSWEoECRIkCBBggQJQpCghqDYSUGQIEGCBAkSJEgQggR1BN0MCoIECRIkSJAgQYIQJKgjaMWhIEiQIEGCBAkSJAhBghp8kLRyFsG/bnwAAAAASUVORK5CYII='
# Quick Look must report rendered within this many polls of this many seconds, the bound the notes case uses.
capmarkdownkinds_poll_attempts=40
capmarkdownkinds_poll_seconds=0.1
# Writes the seven fixture files into the one folder, each text copied from its headless case.
capmarkdownkinds_fixture() {
    local dir="$1" width index
    cat > "$dir/tables.md" <<'MD'
# Tables

| Left | Centre | Right |
|:-----|:------:|------:|
| alpha | **bold** | 10 |
| beta | `a \| b` | 200 |
| gamma | [link](https://example.test) | 3000 |

| Only | Two |
|------|-----|
| one | two | three dropped |
| short |

Text after the tables.
MD
    cat > "$dir/readme.md" <<'MD'
<p align="center">
  <img src="logo.png" width="64" alt="logo">
</p>
<h1 align="center">Flea</h1>
<p align="center"><b>A file manager</b> for <i>Omarchy</i></p>

Press <kbd>Ctrl</kbd>+<kbd>C</kbd> to copy. Water is H<sub>2</sub>O and area is r<sup>2</sup>.<br>
A line after a break.

<details open>
<summary>More</summary>

Hidden body text.

</details>

<script>alert(1)</script>
<!-- a comment -->
<div align="right">Right aligned</div>

<div align="center">
<h2>Wrapped heading</h2>
</div>

<h2 align="center"><img src="logo.png" width="120" alt="banner"><br>Picture heading</h2>
MD
    base64 -d > "$dir/logo.png" <<< "$capmarkdownkinds_png_b64"
    base64 -d > "$dir/bands.png" <<< "$capmarkdownkinds_png_b64"
    {
        printf '<p align="center">\n'
        for width in 70 96 54 120; do printf '  <a href="https://example.com/%s"><img src="logo.png" width="%s" height="20" alt="b%s"></a>\n' "$width" "$width" "$width"; done
        printf '</p>\n\n<p>\n'
        for index in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20; do printf '  <img src="logo.png" width="110" height="20" alt="w%s">\n' "$index"; done
        printf '</p>\n'
    } > "$dir/badges.md"
    cat > "$dir/nesting.md" <<'MD'
# Nesting

- one
  - two
    - three
- [ ] task
  - [x] nested done

1. first

2. loose second

> outer
>> inner
>>> innermost

Inline maths $x^2 + y^2$ in a line.
MD
    # The entity keeps the first line a paragraph reading "1. ol"; the two pictures show the gap around a picture nested in a list item and in a quote.
    cat >> "$dir/nesting.md" <<'MD'

&#49;. ol

- an item with a picture

  ![bands](logo.png)

> a quote with a picture
>
> ![bands](logo.png)

After the pictures.
MD
    printf '# Figures\n\n```mermaid\nflowchart TD\n    A --> B\n```\n\n```mermaid\nsequenceDiagram\n    A->>B: hi\n```\n\n$$x^2$$\n\n```mermaid\nnot a diagram {{{\n```\n\n![bands](bands.png)\n' > "$dir/figures.md"
}
# Opens one document in Quick Look and waits until the view reports it rendered.
capmarkdownkinds_open() {
    local name="$1" view=""
    goto_row "$(row_index_of "$name")"
    key -k Space >/dev/null
    for _attempt in $(seq 1 "$capmarkdownkinds_poll_attempts"); do [[ "$(ipc previewOpen)" == "true" ]] && break; sleep "$capmarkdownkinds_poll_seconds"; done
    [[ "$(ipc previewOpen)" == "true" ]] || fail "capmarkdownkinds: Space did not open Quick Look on $name"
    for _attempt in $(seq 1 "$capmarkdownkinds_poll_attempts"); do
        view="$(ipc previewMarkdownView)"
        [[ "$view" == "rendered" ]] && return 0
        sleep "$capmarkdownkinds_poll_seconds"
    done
    fail "capmarkdownkinds: $name never rendered, previewMarkdownView read [$view]"
}
# Wheel runs a document gets to reach its end: a 500-row table takes several, so forty only stop a view that never ends.
capmarkdownkinds_end_runs=40
# Fails by name when previewScrollY answered something other than a number; called in the main shell so the failure stops the case.
capmarkdownkinds_need_number() {
    [[ "$1" =~ ^-?[0-9]+(\.[0-9]+)?$ ]] || fail "capmarkdownkinds: $2 answered [$1] for $3 while wheeling to its end"
}
# Wheels down a run at a time until the end gap is within the slack or the view stops, where capmarkdown_end_fit names a cut last block.
capmarkdownkinds_reach_end() {
    local name="$1" run gap before after
    capmarkdown_scroll down "$((capmarkdown_notches_mid + capmarkdown_notches_end))"
    for ((run = 1; ; run++)); do
        gap="$(ipc previewEndGap)"
        [[ "$gap" =~ ^-?[0-9]+$ ]] || fail "capmarkdownkinds: previewEndGap answered [$gap] for $name while wheeling to its end"
        (( gap >= 0 && gap <= capmarkdown_end_slack_px )) && return 0
        (( run < capmarkdownkinds_end_runs )) || break
        before="$(ipc previewScrollY)"
        capmarkdownkinds_need_number "$before" previewScrollY "$name"
        omarchy-drive scroll down "$((capmarkdown_notches_mid + capmarkdown_notches_end))" >/dev/null || fail "capmarkdown: scroll down failed on $name"
        settle
        after="$(ipc previewScrollY)"
        capmarkdownkinds_need_number "$after" previewScrollY "$name"
        [[ "$after" == "$before" ]] || continue
        # A wheel run still in flight reads as stopped once; a second settle tells it from the view's real end.
        settle
        after="$(ipc previewScrollY)"
        capmarkdownkinds_need_number "$after" previewScrollY "$name"
        [[ "$after" != "$before" ]] || return 0
    done
    fail "capmarkdownkinds: $name never reached its end after $capmarkdownkinds_end_runs wheel runs"
}
# The pointer over the right side of the window, so the wheel scrolls the preview column.
capmarkdown_column_pointer() {
    local name="$1" wx wy ww wh
    read -r wx wy ww wh < <(window_box) || fail "capmarkdown: native window coordinates unavailable for $name"
    omarchy-drive move "$((wx + ww * 9 / 10))" "$((wy + wh / 2))" >/dev/null || fail "capmarkdown: pointer move to the preview column failed on $name"
    settle
}
# Wheels the preview column to its end: the pointer goes over the right side so the wheel scrolls the preview, and viewContentY stops at viewEndY.
capmarkdown_column_reach_end() {
    local name="$1" run before after end
    capmarkdown_column_pointer "$name"
    for ((run = 1; ; run++)); do
        before="$(ipc viewContentY)"
        end="$(ipc viewEndY)"
        [[ "$before" =~ ^-?[0-9]+$ && "$end" =~ ^-?[0-9]+$ ]] || fail "capmarkdown: viewContentY answered [$before] viewEndY [$end] for $name"
        (( before >= end - capmarkdown_end_slack )) && return 0
        (( run < capmarkdownkinds_end_runs )) || break
        omarchy-drive scroll down "$((capmarkdown_notches_mid + capmarkdown_notches_end))" >/dev/null || fail "capmarkdown: scroll down failed on $name"
        settle
        after="$(ipc viewContentY)"
        [[ "$after" =~ ^-?[0-9]+$ ]] || fail "capmarkdown: viewContentY answered [$after] for $name"
        [[ "$after" == "$before" ]] || continue
        # A wheel run still in flight reads as stopped once; a view still short of its end after a second settle never reached it.
        settle
        after="$(ipc viewContentY)"
        [[ "$after" != "$before" ]] || fail "capmarkdown: $name column stopped at viewContentY $after, short of viewEndY $(ipc viewEndY)"
    done
    fail "capmarkdown: $name never reached its column end after $capmarkdownkinds_end_runs wheel runs: viewContentY $before, viewEndY $end"
}
# overflow.md's 24 long words run past the card: a wheel right over its first body row must grow the table's own scroll position, read over IPC.
capmarkdown_table_sideways() {
    local name="$1" before after
    capmarkdown_pointer table
    before="$(ipc previewTable | jq -r '.scrollX // -1')"
    omarchy-drive scroll right "$capmarkdown_notches_side" >/dev/null || { capmarkdown_sideways_error="scroll right $capmarkdown_notches_side failed on $name"; return 1; }
    settle
    shot "cap-markdown-kind-${name%.md}-sideways"
    after="$(ipc previewTable | jq -r '.scrollX // -1')"
    (( after > before )) || { capmarkdown_sideways_error="the sideways wheel left $name's table at scrollX $before, now $after"; return 1; }
}
# Shoots the open document, then its tail when it is taller than the card, then closes Quick Look.
# Sample input: previewEndGap answers 0 when the last block and its inset are whole at the top, so nothing scrolls; any other number, -1 included, means the document runs past the card.
capmarkdownkinds_shoot() {
    local name="$1" gap
    settle
    shot "cap-markdown-kind-${name%.md}"
    [[ "$name" != overflow.md ]] || capmarkdown_table_sideways "$name" || true
    gap="$(ipc previewEndGap)"
    [[ "$gap" =~ ^-?[0-9]+$ ]] || fail "capmarkdownkinds: previewEndGap answered [$gap] for $name"
    if (( gap != 0 )); then
        capmarkdown_pointer document
        capmarkdownkinds_reach_end "$name"
        capmarkdown_end_fit
        shot "cap-markdown-kind-${name%.md}-end"
    fi
    key -k Escape >/dev/null
    settle
    [[ "$(ipc previewOpen)" == "false" ]] || fail "capmarkdownkinds: Escape did not close Quick Look on $name"
}
# The table torture documents (tests/fixtures/markdown-tables) shot on the display box in Quick Look, then each in the narrow preview column.
case_cap_markdown_tables() { capmarkdown_fixture_set markdown-tables CAPMARKDOWNTABLES rows500.md; }
# The inline picture lines (tests/fixtures/markdown-picline): a picture taller or wider than its line, in Quick Look and the column.
case_cap_markdown_picline() { capmarkdown_fixture_set markdown-picline CAPMARKDOWNPICLINE; }
# Shoots one set's own documents with the shared table assets beside it: capmarkdown_fixture_set SET TAG [EXTRA...].
capmarkdown_fixture_set() {
    local set=$1 tag=$2 dir="$fixture_root/capmarkdown-$1" doc name results=""
    shift 2
    sandbox_scratch "$dir"
    mkdir -p "$dir/listing"
    cp "$repo"/tests/fixtures/"$set"/*.md "$dir/listing/"
    # The same assets the headless table suite generates, so the inline shot shows the picture.
    . "$repo/tests/markdown-tables-assets.sh" || fail "capmarkdown $set: the shared table assets helper did not load"
    markdown_tables_assets_write "$dir/listing" || fail "capmarkdown $set: the shared table assets did not generate"
    launch "$dir/listing"
    wait_listing "$(find "$dir/listing" -mindepth 1 -maxdepth 1 | wc -l | tr -d ' ')"
    # Each set shoots only its own fixture names plus explicit extras, so the picline set never shoots the tables assets rows500.
    for doc in "$repo"/tests/fixtures/"$set"/*.md "$@"; do
        name="${doc##*/}"
        capmarkdownkinds_open "$name"
        capmarkdownkinds_shoot "$name"
        # A case counts as ok only with its shot on disk; the shot helper already fails an empty capture.
        [[ -s "$evidence_dir/cap-markdown-kind-${name%.md}.png" ]] || fail "capmarkdown $set: shot for $name is missing, not recording ok"
        results+="${name%.md}=ok "
        [[ "$name" != overflow.md || -n "$capmarkdown_sideways_error" ]] || results+="overflow-sideways=ok "
    done
    switch_view columns
    for doc in "$repo"/tests/fixtures/"$set"/*.md "$@"; do
        name="${doc##*/}"
        goto_row "$(row_index_of "$name")"
        settle
        capmarkdown_wait_column_rendered
        shot "cap-markdown-kind-column-${name%.md}"
        [[ -s "$evidence_dir/cap-markdown-kind-column-${name%.md}.png" ]] || fail "capmarkdown $set: column shot for $name is missing, not recording ok"
    done
    printf '%s %scolumns=ok\n' "$tag" "$results"
    kill_flea
    [[ -z "$capmarkdown_sideways_error" ]] || fail "capmarkdown $set: $capmarkdown_sideways_error"
}
# Every Markdown kind the stage draws beyond notes.md, shot on the display box: GFM tables, a README in raw HTML, a badge row, nesting with pictures inside blocks, and figures. Cases ql-markdown-tables, -html, -badges, -nesting and -figures in ci/visual/lane/cases.sh draw the same text headless.
case_cap_markdown_kinds() {
    local dir="$fixture_root/capmarkdownkinds" figs="" name results=""
    sandbox_scratch "$dir"
    mkdir -p "$dir/listing"
    capmarkdownkinds_fixture "$dir/listing"
    launch "$dir/listing"
    wait_listing "$(find "$dir/listing" -mindepth 1 -maxdepth 1 | wc -l | tr -d ' ')"
    for name in tables.md readme.md badges.md nesting.md figures.md; do
        capmarkdownkinds_open "$name"
        if [[ "$name" == figures.md ]]; then
            figs="$(capmarkdown_wait_figures)"
            [[ "$(printf '%s' "$figs" | grep -o 'ready' | wc -l | tr -d ' ')" == "3" ]] || fail "capmarkdownkinds: want 3 ready figures in figures.md, saw [$figs]"
            [[ "$(printf '%s' "$figs" | grep -o 'failed' | wc -l | tr -d ' ')" == "1" ]] || fail "capmarkdownkinds: want 1 failed figure in figures.md, saw [$figs]"
        fi
        capmarkdownkinds_shoot "$name"
        results+="${name%.md}=ok "
    done
    switch_view columns
    for name in readme.md badges.md figures.md nesting.md; do
        goto_row "$(row_index_of "$name")"
        settle
        capmarkdown_wait_column_rendered
        shot "cap-markdown-kind-column-${name%.md}"
    done
    printf 'CAPMARKDOWNKINDS %scolumn-nesting=ok\n' "$results"
    kill_flea
}
# The fixture documents of case_cap_markdown_fences: each fence shape in LF and CRLF, a README, and a document past the 64 KiB line with fences on both sides of the head's cut.
capmarkdownfences_fixture() {
    # Past Prepared.MAX_BYTES (64 KiB), so the big documents take Quick Look's headed, sliced parse.
    local dir="$1" index filler filler_bytes=70000
    # A leading folder, so the seek covers the grid-stride shape where row 1 sits on the first tile row.
    mkdir -p "$dir/docs"
    printf '# Fences\n\nBefore.\n\n```toml\n[preview]\nwidth = 240\n```\n\nBetween.\n\n~~~\nplain tilde fence\n~~~\n\nAfter.\n' > "$dir/fence-lf.md"
    sed 's/$/\r/' "$dir/fence-lf.md" > "$dir/fence-crlf.md"
    printf '# Four\n\n````md\n```\ninner\n```\n````\n\nAfter.\n' > "$dir/fence-four.md"
    printf '# Item\n\n- item\n\n  ```sh\n  ls\n  ```\n- two\n\n> quoted\n>\n> ```\n> code\n> ```\n' > "$dir/fence-item.md"
    # The README's fences without its lines that name local pictures, which this listing does not hold, so no image load warns.
    sed '/docs\/images\//d' "$repo/README.md" > "$dir/fence-readme.md"
    filler=$'A paragraph of filler text that runs long enough to wrap in a narrow card.\n\n'
    {
        printf '# Big\n\n```toml\nk = 1\n```\n\n'
        for index in $(seq 1 94); do printf 'para %s\n\n' "$index"; done
        printf '```toml\nk = 2\n```\n\n'
        yes "$filler" | head -c "$filler_bytes"
        printf '\n```toml\nk = 3\n```\n'
    } > "$dir/fence-big.md" 2>/dev/null
    sed 's/$/\r/' "$dir/fence-big.md" > "$dir/fence-bigcrlf.md"
}
# Reported 2026-10-05: a code block's ``` fence lines drew as text in Quick Look from List view. Every fence document shot in Quick Look from List, Grid and Columns, in the preview column of Columns and in Quick Look from Dual, as cap-mdfence-<view>-<doc>.png; by name only.
case_cap_markdown_fences() {
    local dir="$fixture_root/capmdfence" view name results=""
    sandbox_scratch "$dir"
    mkdir -p "$dir/listing"
    capmarkdownfences_fixture "$dir/listing"
    launch "$dir/listing"
    wait_listing "$(find "$dir/listing" -mindepth 1 -maxdepth 1 | wc -l | tr -d ' ')"
    for view in list grid columns; do
        switch_view "$view"
        for name in "$dir"/listing/fence-*.md; do
            name="${name##*/}"
            capmarkdownkinds_open "$name"
            settle
            shot "cap-mdfence-$view-${name%.md}"
            key -k Escape >/dev/null
            settle
            [[ "$(ipc previewOpen)" == "false" ]] || fail "capmdfence: Escape did not close Quick Look on $name in $view"
            results+="$view/${name%.md}=ok "
        done
    done
    # The preview column itself, with no Quick Look open: the file under the cursor in Columns.
    for name in "$dir"/listing/fence-*.md; do
        name="${name##*/}"
        goto_row "$(row_index_of "$name")"
        settle
        capmarkdown_wait_column_rendered
        shot "cap-mdfence-columnpane-${name%.md}"
    done
    click_chrome dual
    settle
    for name in "$dir"/listing/fence-*.md; do
        name="${name##*/}"
        capmarkdownkinds_open "$name"
        settle
        shot "cap-mdfence-dual-${name%.md}"
        key -k Escape >/dev/null
        settle
        [[ "$(ipc previewOpen)" == "false" ]] || fail "capmdfence: Escape did not close Quick Look on $name in dual"
    done
    printf 'CAPMDFENCE %scolumnpane=ok dual=ok\n' "$results"
    kill_flea
}
# Beside the base fixtures: wide lines, language tags, tabs and blanks, an unclosed fence, tight neighbours, indented and listed code, and a document of 250 lines with a fence on each of its screens.
capmarkdownfences14_fixture() {
    local dir="$1" index tag wide json
    wide="ssh -o BatchMode=yes host $(printf 'argument%03d ' $(seq 1 40))"
    json='{"items":['
    for index in $(seq 1 40); do json+="{\"id\":$index,\"name\":\"item-$index\"},"; done
    printf '# Wide\n\n```sh\n%s\n```\n\nBetween.\n\n```json\n%s{"id":0}]}\n```\n' "${wide:0:300}" "$json" > "$dir/fence14-wide.md"
    printf '# Tags\n\n```bash\necho hi | tr a-z A-Z\n```\n\n```rust\nfn main() { println!("hi"); }\n```\n\n```json\n{"a": [1, 2, 3]}\n```\n\n```toml\n[preview]\nwidth = 240\n```\n\n```diff\n-old\n+new\n```\n\n```zzz-unknown\nplain words\n```\n' > "$dir/fence14-tags.md"
    printf '# Tabs\n\n```sh\n\tif true; then\n\t\techo one\n\n\t\techo two\n\tfi\n```\n\nAfter.\n' > "$dir/fence14-tabs.md"
    printf '# Open\n\nBefore.\n\n```sh\nls -la\necho never closed\n' > "$dir/fence14-open.md"
    printf '# Heading\n```sh\nafter heading\n```\n\n- list item\n```sh\nafter item\n```\n' > "$dir/fence14-tight.md"
    printf '# Code\n\nBefore.\n\n    indented line one\n    indented line two\n\n1. numbered item\n\n   ```sh\n   echo inside\n   ```\n2. second item\n' > "$dir/fence14-indent.md"
    {
        printf '# Long\n\n'
        for tag in sh rust toml; do
            for index in $(seq 1 40); do printf 'Paragraph %s of the %s stretch of the long fixture.\n\n' "$index" "$tag"; done
            printf '```%s\nfence after the %s stretch\n```\n\n' "$tag" "$tag"
        done
        printf 'Tail.\n'
    } > "$dir/fence14-long.md"
}
# Each fence fixture's block kinds in capmdfence14_runs form, read off the fixture text above, so a fence rule that misfires fails every view.
capmdfence14_want() {
    case "$1" in
    fence-lf.md|fence-crlf.md) printf '%s' "heading,run,fence:toml,run,fence,run";;
    fence-four.md) printf '%s' "heading,fence:md,run";;
    fence-item.md) printf '%s' "heading,list[run,fence:sh],quote[run,fence]";;
    fence-big.md|fence-bigcrlf.md) printf '%s' "heading,fence:toml,run*94,fence:toml,run*910,fence:toml";;
    fence14-wide.md) printf '%s' "heading,fence:sh,run,fence:json";;
    fence14-tags.md) printf '%s' "heading,fence:bash,fence:rust,fence:json,fence:toml,fence:diff,fence:zzz-unknown";;
    fence14-tabs.md) printf '%s' "heading,fence:sh,run";;
    fence14-open.md) printf '%s' "heading,run,fence:sh";;
    fence14-tight.md) printf '%s' "heading,fence:sh,list,fence:sh";;
    fence14-indent.md) printf '%s' "heading,run,fence,list[run,fence:sh]";;
    fence14-long.md) printf '%s' "heading,run*40,fence:sh,run*40,fence:rust,run*40,fence:toml,run";;
    *) return 1;;
    esac
}
# Sample input: ["heading","list[run,fence:sh x]","fence"] answers fence:sh, the tagged code fences at any depth, each tag to its first space, comma or bracket.
capmdfence14_tagged='[scan("fence:[^\\],\\[\\s]+")] | join(",")'
# Four columns past a container's content make indented code, and a tab advances to the next multiple of them, as the parser counts.
capmdfence14_code_indent=4
# Quote markers stand before a line's content, each with its optional space, one per pass.
capmdfence14_quote_re='^ {0,3}> ?(.*)$'
# A list item's marker, then one or more spaces or the line's end, then the item's first line.
capmdfence14_item_re='^([-*+]|[0-9]{1,9}[.)])( +|$)(.*)$'
# An ordered marker that starts at any number but 1, which cannot interrupt a paragraph.
capmdfence14_ordered_not_one_re='^0*([2-9]|[1-9][0-9]+)?[.)]$'
# A thematic break such as "* * *" is one leaf, never three nested list items.
capmdfence14_break_re='^((\* *){3,}|(- *){3,}|(_ *){3,})$'
capmdfence14_setext_re='^(=+|-+) *$'
capmdfence14_heading_re='^#{1,6}( |$)'
capmdfence14_open_re='^(`{3,}|~{3,})[[:space:]]*(.*)$'
# A closing marker is the opening one's character, at least as long, with nothing after it.
capmdfence14_close_re='^(`{3,}|~{3,}) *$'
# Sample input: "```sh x" opens a fence, leaving ``` and "sh x" in BASH_REMATCH; "```a`b" is inline code and opens none.
capmdfence14_opens() {
    [[ "$1" =~ $capmdfence14_open_re ]] && [[ "${BASH_REMATCH[1]}" != '`'* || "${BASH_REMATCH[2]}" != *'`'* ]]
}
# Sample input: "```sh", "* * *", "# h" and "- b" each start a block, which ends an open paragraph; "more words" continues it.
capmdfence14_starts_block() {
    capmdfence14_opens "$1" || [[ "$1" =~ $capmdfence14_break_re || "$1" =~ $capmdfence14_heading_re || "$1" =~ $capmdfence14_item_re ]]
}
# The README changes every release, so its row is the tagged code fences its own text opens, in order, read with the parser's container and indent rules.
# Sample input: "- a\n\n  ```sh x\n  ls\n  ```\n\n      ```py\n" answers fence:sh, as six spaces in that item are indented code.
capmdfence14_fences() {
    local line rest lead n top kept quotes last_quotes=0 para=0 bare=0 marker gap after width
    local fence="" fence_col=0 fence_quotes=0 info tag out=""
    local -a cols=()
    while IFS= read -r line || [[ -n "$line" ]]; do
        rest="${line%$'\r'}"
        quotes=0
        if [[ -n "$fence" ]]; then
            while (( quotes < fence_quotes )) && [[ "$rest" =~ $capmdfence14_quote_re ]]; do rest="${BASH_REMATCH[1]}"; quotes=$((quotes + 1)); done
            lead="${rest%%[! ]*}"
            n=${#lead}
            # A line still inside the fence's quote and list item is the fence's own: its closer or its content.
            if (( quotes == fence_quotes )) && [[ -z "${rest// /}" || $n -ge $fence_col ]]; then
                (( n - fence_col < capmdfence14_code_indent )) && [[ "${rest:n}" =~ $capmdfence14_close_re && "${BASH_REMATCH[1]}" == "$fence"* ]] && fence=""
                continue
            fi
            # A line that leaves the fence's quote or list item ends the fence with them.
            fence=""
        fi
        while [[ "$rest" =~ $capmdfence14_quote_re ]]; do rest="${BASH_REMATCH[1]}"; quotes=$((quotes + 1)); done
        # A quote that starts or ends closes every list item the lines before it opened.
        (( quotes == last_quotes )) || cols=()
        last_quotes=$quotes
        if [[ -z "${rest// /}" ]]; then
            # An item whose first line was empty ends at its first blank line.
            (( bare == 0 || ${#cols[@]} == 0 )) || unset 'cols[-1]'
            bare=0
            para=0
            continue
        fi
        bare=0
        lead="${rest%%[! ]*}"
        n=${#lead}
        # The list items this line is indented into stay open; the line's own blocks start in the innermost of them.
        kept=${#cols[@]}
        while (( kept > 0 && n < cols[kept - 1] )); do kept=$((kept - 1)); done
        top=0
        (( kept == 0 )) || top=${cols[kept - 1]}
        if (( kept < ${#cols[@]} )); then
            # Text that starts no block there continues the open paragraph lazily, and every item stays open.
            (( para && n - top >= capmdfence14_code_indent )) && continue
            (( para )) && ! capmdfence14_starts_block "${rest:n}" && continue
            cols=("${cols[@]:0:kept}")
            para=0
        fi
        # A line of = or - under an open paragraph underlines it as a heading.
        if (( para && n - top < capmdfence14_code_indent )) && [[ "${rest:n}" =~ $capmdfence14_setext_re ]]; then
            para=0
            continue
        fi
        # Each list marker short of indented code opens an item, nested ones on the same line included.
        while (( n - top < capmdfence14_code_indent )) && [[ ! "${rest:n}" =~ $capmdfence14_break_re && "${rest:n}" =~ $capmdfence14_item_re ]]; do
            marker="${BASH_REMATCH[1]}"
            gap="${BASH_REMATCH[2]}"
            after="${BASH_REMATCH[3]}"
            # An item that would interrupt a paragraph must hold text and, when ordered, start at 1.
            (( para )) && [[ -z "$after" || "$marker" =~ $capmdfence14_ordered_not_one_re ]] && break
            # One to four spaces after the marker set the content column; more make the content indented code, and an empty item takes one.
            width=${#gap}
            (( width >= 1 && width <= capmdfence14_code_indent )) && [[ -n "$after" ]] || width=1
            top=$((n + ${#marker} + width))
            cols+=("$top")
            printf -v rest '%*s%s' "$top" '' "${gap:width}$after"
            lead="${rest%%[! ]*}"
            n=${#lead}
            para=0
            [[ -n "$after" ]] || bare=1
        done
        # Indented code, or an open paragraph's next line, is never a fence.
        (( n - top < capmdfence14_code_indent )) || continue
        if ! capmdfence14_opens "${rest:n}"; then
            [[ -n "${rest// /}" && ! "${rest:n}" =~ $capmdfence14_break_re && ! "${rest:n}" =~ $capmdfence14_heading_re ]] && para=1 || para=0
            continue
        fi
        fence="${BASH_REMATCH[1]}"
        info="${BASH_REMATCH[2]}"
        fence_col=$top
        fence_quotes=$quotes
        para=0
        tag="${info%%[][[:space:],]*}"
        # The pane's row writes an indented block as an untagged fence and a mermaid, math or latex fence as a figure, so only tagged code fences count.
        case "${tag,,}" in
        ''|mermaid|math|latex) ;;
        *) out+=",fence:$tag";;
        esac
    done < <(expand -t "$capmdfence14_code_indent" -- "$1")
    printf '%s' "${out#,}"
}
# The text size every shot of the fence14 case is judged at, GM's own.
capmdfence14_body_px=14
# A wheel step covers this percent of the card, so a fence is never skipped between two shots.
capmdfence14_step_percent=60
# Block kinds are polled this many times for two equal reads, enough for a document past 64 KiB to finish its sliced parse.
capmdfence14_kinds_polls=100
# Sample input: ["heading","run","run","fence:sh"] answers heading,run*2,fence:sh, so a document of a thousand paragraphs logs on one short line.
capmdfence14_runs='reduce .[] as $k ([]; if length > 0 and .[-1][0] == $k then .[-1][1] += 1 else . + [[$k, 1]] end) | map(if .[1] > 1 then "\(.[0])*\(.[1])" else .[0] end) | join(",")'
# The block kinds the pane built as a JSON array, kept in capmdfence14_seen once two reads agree; set in the main shell so a failure stops the case.
capmdfence14_seen=""
capmdfence14_read_kinds() {
    local column="$1" last=""
    for _attempt in $(seq 1 "$capmdfence14_kinds_polls"); do
        capmdfence14_seen="$(ipc previewBlockKinds "$column")"
        [[ -n "$capmdfence14_seen" && "$capmdfence14_seen" == "$last" ]] && return 0
        last="$capmdfence14_seen"
        sleep "$capmarkdownkinds_poll_seconds"
    done
    fail "capmdfence14: the block kinds never settled, last [$capmdfence14_seen]"
}
# Fails unless the pane built the kinds the fixture's own text names; every view answers the same document, so the table keys the file alone.
capmdfence14_check_kinds() {
    local view="$1" name="$2" got want
    got="$(jq -r "$capmdfence14_runs" <<< "$capmdfence14_seen")"
    if [[ "$name" == fence-readme.md ]]; then
        got="$(jq -r "join(\",\") | $capmdfence14_tagged" <<< "$capmdfence14_seen")"
        want="$(capmdfence14_fences "$fixture_root/capmdfence14/listing/$name")"
    else
        want="$(capmdfence14_want "$name")" || fail "capmdfence14: no expected kinds for $name"
    fi
    [[ "$got" == "$want" ]] || fail "capmdfence14: $view $name built $got, want $want"
}
# One shot of the open pane at the asserted text size, then the line that says what the pane built: CAPMDFENCE14 <view> <file> blocks=<n> kinds=<kind,...>.
capmdfence14_shot() {
    local view="$1" name="$2" shot_name="$3" column=false
    [[ "$view" != columnpane ]] || column=true
    [[ "$(ipc bodyPx)" == "$capmdfence14_body_px" ]] || fail "capmdfence14: body text is $(ipc bodyPx) px, not $capmdfence14_body_px, before $shot_name"
    settle
    capmdfence14_read_kinds "$column"
    shot "$shot_name"
    printf 'CAPMDFENCE14 %s %s blocks=%s kinds=%s\n' "$view" "$name" "$(jq 'length' <<< "$capmdfence14_seen")" "$(jq -r "$capmdfence14_runs" <<< "$capmdfence14_seen")"
    capmdfence14_check_kinds "$view" "$name"
}
# The wheel's position in the pane: the column's content, or Quick Look's scroll.
capmdfence14_position() {
    if [[ "$1" == true ]]; then ipc viewContentY; else ipc previewScrollY; fi
}
# Whether the pane's last block is whole in view, from numbers only: a pane that answers anything else stops the case.
capmdfence14_at_end() {
    local column="$1" name="$2" gap y end
    if [[ "$column" == true ]]; then
        y="$(ipc viewContentY)"; end="$(ipc viewEndY)"
        [[ "$y" =~ ^-?[0-9]+$ && "$end" =~ ^-?[0-9]+$ ]] || fail "capmdfence14: viewContentY [$y] and viewEndY [$end] are not numbers for $name"
        (( y >= end - capmarkdown_end_slack ))
    else
        gap="$(ipc previewEndGap)"
        [[ "$gap" =~ ^-?[0-9]+$ ]] || fail "capmdfence14: previewEndGap [$gap] is not a number for $name"
        (( gap >= 0 && gap <= capmarkdown_end_slack_px ))
    fi
}
# Shoots a document at its top and after each wheel step to its end, so every fence below the first screen is in a shot; the first step measures a notch in px.
capmdfence14_walk() {
    local view="$1" name="$2" column=false step=0 notches=1 notch_px viewport before after
    [[ "$view" != columnpane ]] || column=true
    if [[ "$column" == true ]]; then
        capmarkdown_column_pointer "$name"
        read -r _ _ _ viewport < <(window_box) || fail "capmdfence14: native window coordinates unavailable for $name"
    else
        capmarkdown_pointer document
        read -r _ _ _ viewport <<< "$(ipc previewSurfaceRect)"
    fi
    capmdfence14_shot "$view" "$name" "cap-mdfence14-$view-${name%.md}-s0"
    until capmdfence14_at_end "$column" "$name"; do
        (( ++step <= capmarkdownkinds_end_runs )) || fail "capmdfence14: $name in $view never reached its end after $capmarkdownkinds_end_runs wheel steps"
        before="$(capmdfence14_position "$column")"
        if [[ "$column" == true ]]; then
            omarchy-drive scroll down "$notches" >/dev/null || fail "capmdfence14: scroll down $notches failed on $name in $view"
            settle
        else
            capmarkdown_scroll down "$notches"
        fi
        after="$(capmdfence14_position "$column")"
        [[ "$after" =~ ^-?[0-9]+$ && "$after" != "$before" ]] || fail "capmdfence14: the wheel left $name in $view at [$before], now [$after]"
        if (( step == 1 )); then
            notch_px=$((after - before))
            notches=$((viewport * capmdfence14_step_percent / 100 / notch_px))
            (( notches >= 1 )) || notches=1
        fi
        capmdfence14_shot "$view" "$name" "cap-mdfence14-$view-${name%.md}-s$step"
    done
}
# A document is shot once, or walked when it is the long one.
capmdfence14_page() {
    local view="$1" name="$2"
    if [[ "$name" == fence14-long.md ]]; then
        capmdfence14_walk "$view" "$name"
    else
        capmdfence14_shot "$view" "$name" "cap-mdfence14-$view-${name%.md}"
    fi
}
# GM runs text size 14 and the Ruling 8 report did not reproduce at the default: every fence document, the base set and the fence14 set, in Quick Look from List, Grid and Columns, in the preview column and in Dual, at size 14 through the shipped schema, each shot with the block kinds the pane built logged beside it; by name only.
case_cap_markdown_fences14() {
    local dir="$fixture_root/capmdfence14" view name
    sandbox_scratch "$dir"
    mkdir -p "$dir/listing"
    capmarkdownfences_fixture "$dir/listing"
    capmarkdownfences14_fixture "$dir/listing"
    seed_ui_state "$fixture_root/capmdfence14-state" "$(jq -cn --argjson px "$capmdfence14_body_px" '{keys:"default",view:"list",display:{textSize:{mode:$px}}}')"
    launch "$dir/listing"
    wait_listing "$(find "$dir/listing" -mindepth 1 -maxdepth 1 | wc -l | tr -d ' ')"
    for _attempt in $(seq 1 "$capmarkdownkinds_poll_attempts"); do [[ "$(ipc bodyPx)" == "$capmdfence14_body_px" ]] && break; sleep "$capmarkdownkinds_poll_seconds"; done
    [[ "$(ipc bodyPx)" == "$capmdfence14_body_px" ]] || fail "capmdfence14: body text is $(ipc bodyPx) px after launch, not $capmdfence14_body_px"
    for view in list grid columns; do
        switch_view "$view"
        for name in "$dir"/listing/fence*.md; do
            name="${name##*/}"
            capmarkdownkinds_open "$name"
            capmdfence14_page "$view" "$name"
            key -k Escape >/dev/null
            settle
            [[ "$(ipc previewOpen)" == "false" ]] || fail "capmdfence14: Escape did not close Quick Look on $name in $view"
        done
    done
    # The preview column itself, with no Quick Look open: the file under the cursor in Columns.
    for name in "$dir"/listing/fence*.md; do
        name="${name##*/}"
        goto_row "$(row_index_of "$name")"
        settle
        capmarkdown_wait_column_rendered
        capmdfence14_page columnpane "$name"
    done
    click_chrome dual
    settle
    for name in "$dir"/listing/fence*.md; do
        name="${name##*/}"
        capmarkdownkinds_open "$name"
        capmdfence14_page dual "$name"
        key -k Escape >/dev/null
        settle
        [[ "$(ipc previewOpen)" == "false" ]] || fail "capmdfence14: Escape did not close Quick Look on $name in dual"
    done
    printf 'CAPMDFENCE14 done list=ok grid=ok columns=ok columnpane=ok dual=ok\n'
    kill_flea
}
# The text size and the fixture set of the Mermaid type shots, one document per diagram type.
capmdpele_body_px=14
capmdpele_set=markdown-mermaid
# Every Mermaid diagram type (tests/fixtures/markdown-mermaid, one figure each) in Quick Look and then in the preview column at text size 14, shot as pele.<type>.<engine>.quicklook and .column; the engine is read off the bundle the tree ships, so a scratch tree holding the previous figure files shoots the other arm, and FLEA_CAPMDPELE_THEME names a stock theme to draw under in a fixture HOME (default the live theme). By name only.
case_cap_markdown_pele() {
    local dir="$fixture_root/capmdpele" engine=beautiful-mermaid doc name type figs theme="${FLEA_CAPMDPELE_THEME:-live}" real_home="$HOME" home
    grep -q 'pele-edge' "$flea_ui/vendor/mermaid.mjs" && engine=pele
    sandbox_scratch "$dir"
    mkdir -p "$dir/listing"
    cp "$repo"/tests/fixtures/"$capmdpele_set"/*.md "$dir/listing/"
    seed_ui_state "$fixture_root/capmdpele-state" "$(jq -cn --argjson px "$capmdpele_body_px" '{keys:"default",view:"list",display:{textSize:{mode:$px}}}')"
    if [[ "$theme" != live ]]; then
        home=$(coh_home "$theme") || fail "capmdpele: no fixture HOME for the theme $theme"
        export HOME="$home"
    fi
    launch "$dir/listing"
    export HOME="$real_home"
    wait_listing "$(find "$dir/listing" -mindepth 1 -maxdepth 1 | wc -l | tr -d ' ')"
    for _attempt in $(seq 1 "$capmarkdownkinds_poll_attempts"); do [[ "$(ipc bodyPx)" == "$capmdpele_body_px" ]] && break; sleep "$capmarkdownkinds_poll_seconds"; done
    [[ "$(ipc bodyPx)" == "$capmdpele_body_px" ]] || fail "capmdpele: body text is $(ipc bodyPx) px after launch, not $capmdpele_body_px"
    for doc in "$dir"/listing/*.md; do
        name="${doc##*/}"; type="${name%.md}"
        capmarkdownkinds_open "$name"
        # A type the engine cannot draw shows its fenced source, which is the shot the native review compares against.
        figs="$(capmarkdown_wait_figures)"
        settle
        shot "pele.$type.$engine.quicklook"
        printf 'CAPMDPELE %s %s quicklook figures=%s\n' "$engine" "$type" "$figs"
        key -k Escape >/dev/null
        settle
        [[ "$(ipc previewOpen)" == "false" ]] || fail "capmdpele: Escape did not close Quick Look on $name"
    done
    # The preview column draws the same figures from the cache Quick Look filled, so one settle after its render is the figure.
    switch_view columns
    for doc in "$dir"/listing/*.md; do
        name="${doc##*/}"; type="${name%.md}"
        goto_row "$(row_index_of "$name")"
        settle
        capmarkdown_wait_column_rendered
        settle
        shot "pele.$type.$engine.column"
    done
    [[ "$theme" != live ]] || theme="live:$(cat "$real_state_dir/theme.name" 2>/dev/null)"
    printf 'CAPMDPELE done engine=%s theme=%s types=%s\n' "$engine" "$theme" "$(find "$dir/listing" -name '*.md' | wc -l | tr -d ' ')"
    kill_flea
}
