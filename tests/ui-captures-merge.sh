# Sourced by ui.sh, runs only by name: MergeFolders, the folder-on-folder card with Merge fourth of five buttons and Keep both the default, then the inner conflicts card, focus walked to each button.
case_cap_merge() {
    local dir="$fixture_root/cap-merge" menus_checks=0
    sandbox_scratch "$dir"
    mkdir -p "$dir/src/trip" "$dir/Pictures/trip"
    printf 'src photo one\n' > "$dir/src/trip/IMG_4120.HEIC"
    printf 'src clip one\n' > "$dir/src/trip/IMG_4121.MOV"
    printf 'src photo two\n' > "$dir/src/trip/IMG_4122.HEIC"
    printf 'there photo one\n' > "$dir/Pictures/trip/IMG_4120.HEIC"
    printf 'there clip one\n' > "$dir/Pictures/trip/IMG_4121.MOV"
    printf 'there photo two\n' > "$dir/Pictures/trip/IMG_4122.HEIC"
    seed_ui_state "$fixture_root/cap-merge-state" '{"keys":"default","view":"list"}'
    launch "$dir/src"
    wait_listing 1
    cap_resize 1100 700
    seek_row_named "trip" || fail "cap_merge: the source trip never came under the cursor"
    key y >/dev/null
    settle
    menus_expect keyDeliveryState '(.clipboard.paths | length) == 1' "trip is on the clipboard"
    key -k BackSpace >/dev/null
    wait_path "$dir"
    seek_row_named "Pictures" || fail "cap_merge: Pictures never came under the cursor"
    key -k Return >/dev/null
    wait_path "$dir/Pictures"
    wait_listing 1
    key p >/dev/null
    menus_expect collideState '.opened and .title == "trip already exists in Pictures." and .names == ["trip"] and .focus == "keep"' "the folder card opens on Keep both"
    menus_expect collideState '.explain == "Replaced items go to Trash, and Undo restores them."' "the folder card carries the board note"
    menus_expect collideState '[.buttons[] | select(.name == "merge") | .visible] == [true]' "Merge shows on a folder-on-folder card"
    shot merge-folder-keep
    key h >/dev/null
    menus_expect collideState '.focus == "replace"' "h moves the folder card to Replace"
    shot merge-folder-replace
    key h >/dev/null
    menus_expect collideState '.focus == "merge"' "h moves the folder card to Merge"
    shot merge-folder-merge
    key h >/dev/null
    menus_expect collideState '.focus == "skip"' "h moves the folder card to Skip"
    shot merge-folder-skip
    key h >/dev/null
    menus_expect collideState '.focus == "cancel"' "h moves the folder card to Cancel"
    shot merge-folder-cancel
    key l >/dev/null
    key l >/dev/null
    menus_expect collideState '.focus == "merge"' "l from Cancel walks back to Merge"
    key -k Return >/dev/null
    menus_expect collideState '.opened and .title == "3 items already exist in trip." and .focus == "keep"' "Merge walks and the inner card opens on Keep both"
    menus_expect collideState '(.names | length) == 3 and ([.names[] | select(. == "IMG_4120.HEIC" or . == "IMG_4121.MOV" or . == "IMG_4122.HEIC")] | length) == 3' "the inner card names the three colliding photos"
    menus_expect collideState '[.buttons[] | select(.name == "merge") | .visible] == [false]' "the inner card offers no Merge"
    shot merge-inner-keep
    key h >/dev/null
    menus_expect collideState '.focus == "replace"' "h moves the inner card to Replace"
    key h >/dev/null
    menus_expect collideState '.focus == "skip"' "h moves the inner card to Skip"
    shot merge-inner-skip
    key h >/dev/null
    menus_expect collideState '.focus == "cancel"' "h moves the inner card to Cancel"
    shot merge-inner-cancel
    key l >/dev/null
    key l >/dev/null
    menus_expect collideState '.focus == "replace"' "l from Cancel walks back to Replace"
    shot merge-inner-replace
    key -k Escape >/dev/null
    menus_expect collideState '.opened | not' "Escape cancels the inner card and sends nothing"
    [[ "$(cat "$dir/Pictures/trip/IMG_4120.HEIC")" == "there photo one" ]] || fail "cap_merge: Cancel changed the destination photo"
    [[ "$(cat "$dir/src/trip/IMG_4120.HEIC")" == "src photo one" ]] || fail "cap_merge: Cancel changed the source photo"
    printf 'CAP_MERGE folder=keep,skip,cancel,merge,replace inner=keep,skip,cancel,replace\n'
    kill_flea
}
