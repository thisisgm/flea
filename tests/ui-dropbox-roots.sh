#!/usr/bin/env bash
# Sourced by ui.sh after ui-providers.sh; private provider fixtures never reach a real cloud account.
# shellcheck disable=SC2154 # ui.sh and the provider fixture supply the candidate and sandbox variables.
dropbox_roots_account() {
    providers_write home/.dropbox/next.json "$1"
    menus_guard "$menu_box/home/.dropbox/next.json"
    menus_guard "$menu_box/home/.dropbox/info.json"
    mv -- "$menu_box/home/.dropbox/next.json" "$menu_box/home/.dropbox/info.json" \
        || fail 'dropboxroots: atomic account update failed'
}

dropbox_roots_rail() {
    local path="$1" label="$2" quoted state
    quoted=$(jq -cn --arg path "$path" '$path') || fail 'dropboxroots: account path encoding failed'
    menus_expect railEntries "[.[] | select(.kind == \"dropbox\")] | length == 1 and .[0].path == $quoted and .[0].label == \"Dropbox\" and .[0].mounted" "$label"
    providers_expect ".dropbox.path == $quoted" 'rail and menu service retain the same account root'
    providers_expect '.dropbox.metadataBusy == false and (.formatsRequests | type == "number")' 'account metadata refresh drains before measuring work'
    state=$(ipc providerState) || fail 'dropboxroots: metadata work observation failed'
    printf 'DROPBOX_ROOTS_WORK label=%q formats_requests=%s listing_requests=%s helper_calls=%s\n' \
        "$label" "$(jq -r .formatsRequests <<< "$state")" "$(ipc listRequests)" "$(jq -s length "$menu_box/calls.jsonl")"
}

dropbox_roots_open_rail() {
    local path="$1" input="$2" target index
    target=$(ipc railEntries | jq -er 'to_entries[] | select(.value.kind == "dropbox") | .key') \
        || fail 'dropboxroots: Dropbox rail row is absent'
    if [[ "$input" == pointer ]]; then
        click_rail_row "$target" left
    else
        [[ "$(ipc focusView)" == rail ]] || key -k Tab >/dev/null || fail 'dropboxroots: rail Tab failed'
        menus_equal 'Tab enters the actual rail' rail "$(ipc focusView)"
        key g >/dev/null || fail 'dropboxroots: rail first-row key failed'
        for ((index = 0; index < target; index++)); do
            key -k Down >/dev/null || fail 'dropboxroots: rail Down failed'
        done
        menus_equal 'keyboard selects the current account row' "$target" "$(ipc railCursor)"
        key -k Return >/dev/null || fail 'dropboxroots: rail Enter failed'
    fi
    wait_path "$path"
    wait_listing 2
    menus_expect listInFlight '. == false' "$input rail activation opens the actual account listing"
}

dropbox_roots_menu() {
    local name="$1" input="$2" expected="$3" absent="$4"
    providers_open "$name" "$input"
    menus_expect menuState "any(.entries[]; .action == \"$expected\" and (.disabled | not)) and all(.entries[]; .action != \"$absent\")" \
        "cursor item offers $expected and excludes $absent through $input"
    providers_geometry
}

case_dropboxroots() (
    local menu_box="$fixture_root/dropboxroots" menu_dir="$fixture_root/dropboxroots/list" menus_checks=0
    # shellcheck disable=SC2034 # Sourced provider helpers use these case-local values.
    local taildrop_fd dropbox_fd provider_ready='{"BackendState":"Stopped"}'
    local personal="$menu_box/accounts/Personal" business="$menu_box/accounts/Business"
    local sibling="$menu_box/accounts/Personal-old" path record started first_rows requests before clipboard_before
    providers_fixture
    trap 'providers_cleanup || exit 1' EXIT
    for path in "$personal" "$business" "$sibling" "$menu_box/home/Dropbox"; do
        menus_guard "$path"
        mkdir -p "$path" || fail 'dropboxroots: owned account directory creation failed'
        providers_write "${path#"$menu_box/"}/a-marked.txt" "$path marked original"
        providers_write "${path#"$menu_box/"}/b-cursor.txt" "$path cursor original"
    done
    providers_install dropbox-cli yes
    record=$(jq -cn --arg personal "$personal" --arg business "$business" '{personal:{path:$personal},business:{path:$business}}')
    dropbox_roots_account "$record"
    export HOME="$menu_box/home" XDG_STATE_HOME="$menu_box/state" XDG_CONFIG_HOME="$menu_box/config"
    export XDG_CACHE_HOME="$menu_box/cache" XDG_DATA_HOME="$menu_box/data" PATH="$menu_box/bin" FLEA_PROVIDERS_BOX="$menu_box"
    "$flea_bin" --ui-state '{"view":"list","keys":"default","menu":{"hidden":[]}}' >/dev/null \
        || fail 'dropboxroots: private settings seed failed'
    started=$(date +%s%3N)
    launch "$menu_dir"
    wait_listing 2
    dropbox_roots_rail "$personal" 'startup uses personal metadata instead of stale HOME/Dropbox'
    first_rows=$(ipc firstRowsAt)
    printf 'DROPBOX_ROOTS_STARTUP first_rows_ms=%s observation_ms=%s first_rows_includes_launcher=true\n' \
        "$((first_rows - started))" "$(($(date +%s%3N) - started))"
    menus_equal 'account discovery runs no provider helper' 0 "$(jq -s length "$menu_box/calls.jsonl")"
    menus_expect menuState '(.opened | not) and .snapshotId == 0' 'startup root discovery needs no menu snapshot'
    dropbox_roots_open_rail "$personal" pointer
    menus_shot dropbox-roots-personal
    menus_visit "$menu_dir" 2
    requests=$(ipc listRequests)
    started=$(date +%s%3N)
    dropbox_roots_account "$(jq -cn --arg path "$business" '{business:{path:$path}}')"
    dropbox_roots_rail "$business" 'atomic business-only account change updates the rail before any menu'
    printf 'DROPBOX_ROOTS_CHANGE observation_ms=%s\n' "$(($(date +%s%3N) - started))"
    menus_equal 'account change does not reload the listing' "$requests" "$(ipc listRequests)"
    menus_equal 'account change runs no provider helper' 0 "$(jq -s length "$menu_box/calls.jsonl")"
    dropbox_roots_open_rail "$business" key
    menus_shot dropbox-roots-business

    launch "$menu_dir"
    wait_listing 2
    dropbox_roots_rail "$business" 'business-only root survives application restart'
    menus_guard "$menu_box/home/.dropbox/info.json"
    menus_guard "$menu_box/retired/info.json"
    mv -- "$menu_box/home/.dropbox/info.json" "$menu_box/retired/info.json" || fail 'dropboxroots: metadata retirement failed'
    menus_expect railEntries 'all(.[]; .kind != "dropbox")' 'removed account metadata removes the stale rail row'
    dropbox_roots_account "$record"
    dropbox_roots_rail "$personal" 'creating missing metadata re-arms the account watch'
    dropbox_roots_account '{'
    menus_expect railEntries 'all(.[]; .kind != "dropbox")' 'malformed account metadata cannot retain a stale root'
    dropbox_roots_account "$record"
    dropbox_roots_rail "$personal" 'valid metadata recovers after malformed input'
    menus_guard "$menu_box/home/.dropbox/info.json"
    menus_guard "$menu_box/retired/valid-account.json"
    mv -- "$menu_box/home/.dropbox/info.json" "$menu_box/retired/valid-account.json" || fail 'dropboxroots: valid metadata retirement failed'
    menus_guard "$menu_box/home/.dropbox/info.json"
    mkfifo "$menu_box/home/.dropbox/info.json" || fail 'dropboxroots: owned metadata FIFO creation failed'
    providers_expect '.facts.dropboxError | contains("not a regular file")' 'metadata FIFO is refused without blocking the real backend'
    menus_expect railEntries 'all(.[]; .kind != "dropbox")' 'unreadable metadata cannot retain a stale root'
    menus_guard "$menu_box/home/.dropbox/info.json"
    menus_guard "$menu_box/retired/account.fifo"
    mv -- "$menu_box/home/.dropbox/info.json" "$menu_box/retired/account.fifo" || fail 'dropboxroots: owned metadata FIFO retirement failed'
    dropbox_roots_account "$record"
    dropbox_roots_rail "$personal" 'valid account recovers after non-regular metadata refusal'
    menus_guard "$menu_box/home/.dropbox"
    menus_guard "$menu_box/retired/live-account-state"
    mv -- "$menu_box/home/.dropbox" "$menu_box/retired/live-account-state" || fail 'dropboxroots: live account directory retirement failed'
    menus_expect railEntries 'all(.[]; .kind != "dropbox")' 'moving the live account directory removes its stale root'
    menus_guard "$menu_box/home/.dropbox"
    mkdir "$menu_box/home/.dropbox" || fail 'dropboxroots: live account directory recreation failed'
    dropbox_roots_account "$record"
    dropbox_roots_rail "$personal" 'recreated account directory updates the live rail'

    kill_flea
    menus_guard "$menu_box/home/.dropbox"
    menus_guard "$menu_box/retired/account-state"
    mv -- "$menu_box/home/.dropbox" "$menu_box/retired/account-state" || fail 'dropboxroots: account directory retirement failed'
    launch "$menu_dir"
    wait_listing 2
    providers_expect '.facts.dropboxInfo == "" and .dropbox.path == ""' 'startup observes an absent account directory'
    menus_expect railEntries 'all(.[]; .kind != "dropbox")' 'stale HOME/Dropbox alone never creates a provider row'
    menus_guard "$menu_box/home/.dropbox"
    mkdir "$menu_box/home/.dropbox" || fail 'dropboxroots: account directory recreation failed'
    dropbox_roots_account "$record"
    dropbox_roots_rail "$personal" 'new account directory is discovered without a menu or restart'
    menus_expect menuState '(.opened | not) and .snapshotId == 0' 'all account watch transitions precede menu entry'
    menus_equal 'all root discovery remains free of daemon helper calls' 0 "$(jq -s length "$menu_box/calls.jsonl")"

    menus_visit "$personal" 2
    dropbox_roots_menu b-cursor.txt menu-key sharelink dropbox
    providers_close
    menus_visit "$sibling" 2
    dropbox_roots_menu b-cursor.txt key dropbox sharelink
    providers_close
    menus_visit "$menu_box/accounts" 3
    key -M ctrl -k f -m ctrl b-cursor -k Return >/dev/null || fail 'dropboxroots: native ancestor Search failed'
    menus_expect keyDeliveryState '.searchMode == "results" and .searchQuery == "b-cursor" and (.searchRunning | not)' \
        'ancestor Search returns the actual account and sibling files'
    wait_listing 3
    wait_path "$menu_box/accounts"
    dropbox_roots_menu Personal/b-cursor.txt pointer sharelink dropbox
    providers_expect ".cursorPath == \"$personal/b-cursor.txt\" and .path == \"$menu_box/accounts\"" \
        'Search menu captures an account cursor while its listing base remains the ancestor'
    menus_shot dropbox-roots-search
    before=$(providers_calls dropbox-cli)
    clipboard_before=$(providers_calls wl-copy)
    providers_choose sharelink
    providers_call dropbox-cli "$(jq -cn --arg path "$personal/b-cursor.txt" '["sharelink",$path]')" "$((before + 1))"
    providers_call wl-copy '["https://fixture.invalid/share"]' "$clipboard_before"
    # The status bar keeps the search keys in its primary slot on purpose (tests/js/status.js, "a
    # plain notice still yields to the search"), so the confirmation is read where it is emitted.
    menus_said 'Share link copied to the clipboard.' 'ancestor Search dispatches the captured absolute account path'
    providers_expect '.listFocus and (.pendingActivation | not)' 'Search share-link activation restores its listing focus'
    dropbox_roots_menu Personal-old/b-cursor.txt menu-letter dropbox sharelink
    providers_close
    key -k Escape >/dev/null || fail 'dropboxroots: Search Escape failed'
    menus_expect keyDeliveryState '.searchMode == ""' 'Search dismissal restores ordinary account browsing'
    wait_listing 3
    printf 'DROPBOX_ROOTS_NATIVE checks=%s input=rail-pointer,rail-keys,Menu,Shift-F10,m,right-click,Search metadata=personal,business,atomic,removed,malformed,FIFO,new-parent visual_inspection=pending performance_pair=pending\n' "$menus_checks"
)
