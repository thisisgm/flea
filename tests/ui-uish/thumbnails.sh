# GPUI overrides for UISH-15; seeded from the unchanged oracle at 2faf8d099.
case_thumbs() {
    [[ -d "$FIXTURE_ROOT/flea-media-btrfs" ]] || fail "the media fixture is missing"
    sandbox_make "$thumb_fixture"
    local i
    for i in $(seq 0 $((thumb_rows - 1))); do
        ln "$FIXTURE_ROOT/flea-media-btrfs/photo_0.jpg" "$thumb_fixture/p$i.jpg"
    done
    # kill_flea waits out the previous backend's drain, so the baseline is stable before this one generates.
    local before_large
    kill_flea
    before_large=$(ls -A "$cache_large" | wc -l)
    seed_ui_state "$fixture_root/thumbs-state" '{"view":"list"}'
    launch "$thumb_fixture"
    wait_listing "$thumb_rows"
    [[ "$(ipc viewMode)" == list ]] || fail "thumbs: fixture did not open its list view"
    # Read before any thumbnail lands, so the slot is compared against its pre-thumbnail row.
    local row_height
    row_height=$(ipc fileRowHeight)

    # One settle after the first window, and one request naming the visible rows.
    local waited
    for waited in $(seq 1 $((thumb_fill_s * 20))); do
        [[ -n "$(ipc thumbFile 0)" ]] && break
        sleep 0.05
    done
    printf 'THUMBS requests=%s file0=%q file1=%q\n' "$(ipc thumbRequests)" "$(ipc thumbFile 0)" "$(ipc thumbFile 1)"
    shot thumbs-first-screen
    [[ "$(ipc thumbRequests)" == "1" ]] || fail "the first screen took $(ipc thumbRequests) requests, not one"
    [[ "$(ipc thumbFile 0)" == "$cache_large/"*.png ]] || fail "row 0 has no cached thumbnail: $(ipc thumbFile 0)"
    local screen_rows icon0 icon_next file_next glyph_last y0 y1 pitch
    screen_rows=$(ipc visibleRows)
    icon0=$(ipc rowIcon 0)
    # GPUI builds no row past the viewport (QML's cacheBuffer row does not exist), so the last row inside it witnesses that the row readers see built rows.
    glyph_last=$(ipc rowGlyph "$((screen_rows - 1))")
    # The first row past the screen was never requested: it has no cached file and draws no thumbnail.
    icon_next=$(ipc rowIcon "$screen_rows")
    file_next=$(ipc thumbFile "$screen_rows")
    read -r _thumb_x0 y0 <<< "$(ipc rowCentre 0)"
    read -r _thumb_x1 y1 <<< "$(ipc rowCentre 1)"
    [[ -n "$y0" && -n "$y1" ]] || fail "row 0 or row 1 reported no on-screen centre, so no pitch was ever rendered to measure"
    pitch=$(( y1 - y0 ))
    printf 'THUMBS icon0=%q screenRows=%s iconNext=%q fileNext=%q glyphLast=%q pitch=%s rowHeight=%s\n' \
        "$icon0" "$screen_rows" "$icon_next" "$file_next" "$glyph_last" "$pitch" "$row_height"
    # The mtime query is part of the shape, so cutting it reddens here as well as in case_stale.
    [[ "$icon0" == "file://$cache_large/"*".png?m="* ]] || fail "row 0 still draws its icon: $icon0"
    # The glyph, not the thumbnail URL, proves the row readers see a built row; an unrequested row carries no thumbnail at all.
    [[ -n "$glyph_last" ]] || fail "row $((screen_rows - 1)) inside the viewport reads as never built, so the row readers prove nothing"
    [[ -z "$file_next" ]] || fail "row $screen_rows past the screen was asked for a thumbnail: $file_next"
    [[ -z "$icon_next" ]] || fail "row $screen_rows was never requested and still draws a thumbnail: $icon_next"
    # The pitch comes off itemRect and not off a Theme token, so a thumbnail that grew its row reddens here.
    [[ "$pitch" == "$row_height" ]] || fail "a thumbnail made the rendered pitch $pitch, not the $row_height the theme asks for"

    # The fling has to move the viewport, or the request bounds below would pass on a list that never scrolled.
    local wx wy ww wh before_requests moved cursor_a cursor_b
    read -r wx wy ww wh < <(window_box) || fail "native window coordinates unavailable"
    omarchy-drive move "$((wx + ww / 2))" "$((wy + wh / 2))" >/dev/null
    before_requests=$(ipc thumbRequests)
    # Read before the fling starts: the list saturates within its first notches, 21 of 1500 here, so a sample taken during it already reads the end.
    cursor_a=$(ipc cursor)
    omarchy-drive scroll down "$fling_clicks" >/dev/null
    sleep 0.5
    cursor_b=$(ipc cursor)
    moved=0
    [[ "$cursor_b" != "$cursor_a" ]] && moved=1
    printf 'THUMBS fling before=%s cursor=%s..%s moved=%s after=%s\n' \
        "$before_requests" "$cursor_a" "$cursor_b" "$moved" "$(ipc thumbRequests)"
    sleep 1

    # Every number is read while the window lives and asserted after it dies, so the cache count can go first.
    local screen_rows requests added
    screen_rows=$(ipc visibleRows)
    requests=$(ipc thumbRequests)
    kill_flea
    sandbox_make "$thumb_fixture"
    added=$(( $(ls -A "$cache_large" | wc -l) - before_large ))
    # A window that is not row aligned straddles one more row than it holds, so the bound is the viewport rule plus that row.
    printf 'THUMBS settled requests=%s cache before=%s now=%s added=%s bound=%s\n' \
        "$requests" "$before_large" "$(ls -A "$cache_large" | wc -l)" "$added" "$(( requests * (screen_rows + 1) ))"

    # A warm cache would satisfy the ceiling below with nothing generated, so the witness checks itself first.
    (( added >= screen_rows )) \
        || fail "only $added rows were generated, so the cache was already warm and the bound below proves nothing"
    # The rule that forfeits the project is a row count and thumbRequests counts lines; see AGENTS.md "Thumbnail requests in the GUI".
    (( added <= requests * (screen_rows + 1) )) \
        || fail "$added of $thumb_rows rows were generated by $requests requests of at most $screen_rows visible rows and one straddled row each"
    (( requests - before_requests <= 2 )) \
        || fail "one fling issued $(( requests - before_requests )) requests"
    # And a lower bound, because a settle timer no scroll ever restarts would also issue none at all.
    (( requests - before_requests >= 1 )) \
        || fail "the fling stopped on rows nothing had asked for and settled without asking"
    # A fling that moves nothing fails here instead of passing quietly; "nothing is requested while moving" is carried by the request-count bounds above.
    (( moved >= 1 )) \
        || fail "the fling did not move the viewport at all, so the bounds above prove nothing"
    [[ "$(ls -A "$cache_large" | grep -c '^\.flea-')" == "0" ]] || fail "a temp file was left in the shared cache"
}

case_hashcache() {
    [[ -d "$FIXTURE_ROOT/flea-media-btrfs" ]] || fail "the media fixture is missing"
    local pics="$hash_fixture/pics"
    # The two bytes encodeURI leaves alone, in the half of the path that can legally hold them.
    local cache="$hash_fixture/c#a?che"
    sandbox_make "$hash_fixture"
    mkdir -p "$pics" "$cache/thumbnails/large" "$cache/thumbnails/fail"
    ln "$FIXTURE_ROOT/flea-media-btrfs/photo_0.jpg" "$pics/one.jpg"
    # Exported inside this case's own subshell, so no other case reads or writes the redirected root.
    export XDG_CACHE_HOME="$cache"
    launch "$pics"
    wait_listing 1

    local waited file icon status
    for waited in $(seq 1 $((thumb_fill_s * 20))); do
        [[ -n "$(ipc thumbFile 0)" ]] && break
        sleep 0.05
    done
    file=$(ipc thumbFile 0)
    # The decode is asynchronous, so a status read taken at once would catch Loading and prove nothing.
    for waited in $(seq 1 $((thumb_fill_s * 20))); do
        status=$(ipc rowIconStatus 0)
        [[ "$status" != "$image_loading" ]] && break
        sleep 0.05
    done
    icon=$(ipc rowIcon 0)
    printf 'HASHCACHE file=%q icon=%q status=%s generated=%s\n' \
        "$file" "$icon" "$status" "$(ls -A "$cache/thumbnails/large" | wc -l)"
    shot hashcache
    [[ "$file" == "$cache/thumbnails/large/"*.png ]] \
        || fail "the backend did not use the redirected cache root, so no # ever reached the row: $file"
    [[ "$icon" == *%23* && "$icon" == *%3F* ]] || fail "the row URL left # or ? unescaped: $icon"
    [[ "$status" == "$image_ready" ]] || fail "the row URL never opened, Image.status is $status: $icon"
    kill_flea
    sandbox_make "$hash_fixture"
}

case_stale() {
    command -v magick >/dev/null || fail "ImageMagick is missing, so no chroma of the icon slot can be read"
    local pics="$stale_fixture/tree/pics"
    local cache="$stale_fixture/cache"
    local src="$stale_fixture/src"
    local state="$stale_fixture/state"
    sandbox_make "$stale_fixture"
    mkdir -p "$pics" "$src" "$cache/thumbnails/large" "$cache/thumbnails/fail"
    magick -size 512x512 xc:red "$src/before.jpg"
    magick -size 512x512 xc:blue "$src/after.jpg"
    cp "$src/before.jpg" "$pics/one.jpg"
    # Exported inside this case's own subshell, so no other case reads or writes the redirected root.
    export XDG_CACHE_HOME="$cache"
    seed_ui_state "$state" '{"view":"list","keys":"default","preview":{"column":false,"thumbnails":"images"}}'
    launch "$pics"
    wait_listing 1

    local first_file crop red_before blue_before
    wait_thumb_ready
    first_file=$(ipc thumbFile 0)
    crop=$(icon_crop)
    shot stale-before
    red_before=$(count_pixels "$evidence_dir/stale-before.png" "$crop" "$icon_red")
    blue_before=$(count_pixels "$evidence_dir/stale-before.png" "$crop" "$icon_blue")
    printf 'STALE before file=%q crop=%s red=%s blue=%s\n' "$first_file" "$crop" "$red_before" "$blue_before"
    [[ "$first_file" == "$cache/thumbnails/large/"*.png ]] \
        || fail "the backend did not use the redirected cache root, so nothing here is this case's own: $first_file"
    (( red_before > 0 )) || fail "the first thumbnail drew no red pixel at all, so this case could not fail"
    (( blue_before == 0 )) || fail "the first thumbnail drew $blue_before blue pixels before anything was edited"

    cp "$src/after.jpg" "$pics/one.jpg"
    touch -d "@$(( $(date +%s) - stale_mtime_back_s ))" "$pics/one.jpg"
    # Only Pane.open clears the thumbnail map, so leaving the directory and coming back is what re-asks for the row.
    key h >/dev/null
    wait_path "$stale_fixture/tree"
    wait_listing 1
    # The second tap is what opens a row, see keys.toml's [[pointer]] table.
    click_row 0 left --double
    wait_path "$pics"
    wait_listing 1

    local second_file red_after blue_after cache_colour cache_blue cache_red
    wait_thumb_ready
    second_file=$(ipc thumbFile 0)
    crop=$(icon_crop)
    shot stale-after
    red_after=$(count_pixels "$evidence_dir/stale-after.png" "$crop" "$icon_red")
    blue_after=$(count_pixels "$evidence_dir/stale-after.png" "$crop" "$icon_blue")
    cache_colour=$(magick "$second_file" -resize 1x1! -format "%[fx:int(u.b*255)] %[fx:int(u.r*255)]" info:)
    printf 'STALE after file=%q same_path=%s icon=%q red=%s blue=%s cachepng_b_r=%s\n' \
        "$second_file" "$([[ "$second_file" == "$first_file" ]] && printf yes || printf no)" \
        "$(ipc rowIcon 0)" "$red_after" "$blue_after" "$cache_colour"
    [[ "$second_file" == "$first_file" ]] \
        || fail "the regenerated thumbnail landed at a new path, so the stale frame this case guards is unreachable"
    read -r cache_blue cache_red <<< "$cache_colour"
    (( cache_blue > 128 && cache_red < 128 )) \
        || fail "the backend never regenerated the thumbnail, so the screen below has nothing new to show"
    (( blue_after > 0 )) || fail "the regenerated thumbnail drew no blue pixel, so the row is showing the old frame"
    (( red_after == 0 )) || fail "the row still draws $red_after red pixels of the thumbnail it replaced"

    # The columns view draws the slot with its own Image, so the same regeneration runs once more under it, back to red.
    local col_crop red_cols blue_cols
    switch_view columns
    for _attempt in $(seq 1 50); do
        [[ "$(ipc rowThumbReady 0)" == "true" ]] && break
        sleep 0.1
    done
    [[ "$(ipc rowThumbReady 0)" == "true" ]] || fail "columns: the row never decoded its thumbnail"
    read -r cx cy cw ch <<< "$(ipc rowThumbRect 0)"
    col_crop="${cw}x${ch}+${cx}+${cy}"
    shot stale-columns-before
    blue_cols=$(count_pixels "$evidence_dir/stale-columns-before.png" "$col_crop" "$icon_blue")
    (( blue_cols > 0 )) || fail "columns: the row draws no blue pixel of the current thumbnail in $col_crop"
    cp "$src/before.jpg" "$pics/one.jpg"
    touch -d "@$(( $(date +%s) - 2 * stale_mtime_back_s ))" "$pics/one.jpg"
    key h >/dev/null
    wait_path "$stale_fixture/tree"
    wait_listing 1
    click_row 0 left --double
    wait_path "$pics"
    wait_listing 1
    for _attempt in $(seq 1 50); do
        [[ "$(ipc rowThumbReady 0)" == "true" && "$(ipc thumbFile 0)" == "$first_file" ]] && break
        sleep 0.1
    done
    read -r cx cy cw ch <<< "$(ipc rowThumbRect 0)"
    col_crop="${cw}x${ch}+${cx}+${cy}"
    shot stale-columns-after
    red_cols=$(count_pixels "$evidence_dir/stale-columns-after.png" "$col_crop" "$icon_red")
    blue_cols=$(count_pixels "$evidence_dir/stale-columns-after.png" "$col_crop" "$icon_blue")
    printf 'STALE columns file=%q red=%s blue=%s\n' "$(ipc thumbFile 0)" "$red_cols" "$blue_cols"
    [[ "$(ipc thumbFile 0)" == "$first_file" ]] || fail "columns: the regenerated thumbnail landed at a new path, $(ipc thumbFile 0)"
    (( red_cols > 0 )) || fail "columns: the regenerated thumbnail drew no red pixel, so the row is showing the old frame"
    (( blue_cols == 0 )) || fail "columns: the row still draws $blue_cols blue pixels of the thumbnail it replaced"
    switch_view list
    kill_flea
    sandbox_make "$stale_fixture"
}

case_userthumbnailer() (
    local menu_box="" path cached deadline encoded
    sandbox_require "$fixture_root"
    command -v magick >/dev/null || fail "userthumbnailer: the real fixture tool is unavailable: magick"
    menu_box=$(mktemp -d "$fixture_root/user-thumbnailer.XXXXXXXX") || fail "userthumbnailer: fixture creation failed"
    printf 'native user thumbnailer fixture\n' > "$menu_box/.flea-test-sandbox"
    [[ "$menu_box" == "$(realpath -e "$menu_box")" ]] || fail "userthumbnailer: fixture path is not canonical"
    for path in listing sources state config cache data data/thumbnailers bin; do
        menus_guard "$menu_box/$path"
        mkdir "$menu_box/$path" || fail "userthumbnailer: cannot create $path"
    done
    menus_guard "$menu_box/sources/image.png"
    magick -size 64x48 xc:steelblue "$menu_box/sources/image.png" || fail "userthumbnailer: image creation failed"
    encoded=$(base64 -w0 "$menu_box/sources/image.png") || fail "userthumbnailer: image encoding failed"
    # The program lives outside /usr and holds its own PNG, so only the jail's bind of that one file is under test.
    menus_guard "$menu_box/bin/drive-url-thumbnail"
    printf '#!/bin/sh\nprintf %%s %s | /usr/bin/base64 -d > "$2"\n' "'$encoded'" > "$menu_box/bin/drive-url-thumbnail"
    chmod 755 "$menu_box/bin/drive-url-thumbnail" || fail "userthumbnailer: the program is not executable"
    menus_guard "$menu_box/data/thumbnailers/url.thumbnailer"
    printf '[Thumbnailer Entry]\nTryExec=%s\nExec=%s %%i %%o %%s\nMimeType=application/x-mswinurl;\n' \
        "$menu_box/bin/drive-url-thumbnail" "$menu_box/bin/drive-url-thumbnail" > "$menu_box/data/thumbnailers/url.thumbnailer"
    menus_guard "$menu_box/listing/a.url"
    printf '[InternetShortcut]\nURL=https://example.com/\n' > "$menu_box/listing/a.url"
    export XDG_CONFIG_HOME="$menu_box/config" XDG_CACHE_HOME="$menu_box/cache" XDG_DATA_HOME="$menu_box/data"
    seed_ui_state "$menu_box/state" '{"view":"grid","keys":"default","preview":{"column":true,"loadOn":"manual","thumbnails":"media"}}'
    launch "$menu_box/listing"
    wait_listing 1
    [[ "$(ipc viewMode)" == grid ]] || fail "userthumbnailer: the fixture did not open its grid view"
    thumbnailpolicy_expect '.files["0"] | type == "string" and length > 0' "grid asks the registered thumbnailer outside /usr"
    cached=$(ipc thumbnailPolicyState | jq -er '.files["0"]') || fail "userthumbnailer: the tile has no cached thumbnail path"
    [[ "$cached" == "$menu_box/cache/thumbnails/large/"*.png ]] || fail "userthumbnailer: the thumbnail is outside the case cache: $cached"
    deadline=$((SECONDS + 15))
    while [[ "$(ipc rowThumbReady 0)" != true ]] && (( SECONDS < deadline )); do sleep 0.05; done
    [[ "$(ipc rowThumbReady 0)" == true ]] || fail "userthumbnailer: the tile never drew the cached thumbnail"
    shot user-thumbnailer-grid
    kill_flea
)
