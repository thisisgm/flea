#!/usr/bin/env bash
# Declarations name overrides, not successful cases or native acceptance.
declare -a flea_uish_modules=()
declare -A flea_uish_assignments=()
flea_uish_module() {
    local module=$1
    shift
    flea_uish_modules+=("$module")
    flea_uish_assignments["$module"]="$*"
}
flea_uish_module tests/ui-uish/common-observers.sh scroll colour lifted settings
flea_uish_module tests/ui-uish/launch.sh select renderer
flea_uish_module tests/ui-uish/state.sh xwsettings xwwatch makedefault
flea_uish_module tests/ui-uish/sizes.sh dirsortstale nosweep hiddenlast
flea_uish_module tests/ui-uish/inline-rename.sh clickedge renamefirst renamelife renamefar
flea_uish_module tests/ui-uish/modal-routing.sh collide menu clickthrough wheelunder
flea_uish_module tests/ui-uish/places.sh editplace placemenu rename recent
flea_uish_module tests/ui-uish/network-form.sh network netmark networkauth
flea_uish_module tests/ui-uish/network-service.sh networktimeout gvfs unmount hangshare hanglisting
flea_uish_module tests/ui-uish/devices.sh unmounted optical phones eject poweroff
flea_uish_module tests/ui-uish/providers.sh sharebrowser taildrop providers
flea_uish_module tests/ui-uish/thumbnails.sh thumbs hashcache stale userthumbnailer
flea_uish_module tests/ui-uish/quick-look.sh open preview lookopen relstart
flea_uish_module tests/ui-uish/documents.sh pdffocus columns views formats
flea_uish_module tests/ui-uish/media.sh mute previewviews
flea_uish_module tests/ui-uish/preview-swap.sh watch previewswap
flea_uish_module tests/ui-uish/tabs.sh tabs tabdrag middleclick opentab folderviews
flea_uish_module tests/ui-uish/operations.sh dd ddclick operations transferlive xwundo
flea_uish_module tests/ui-uish/open-with.sh openterminal makeexec openwithdesign
flea_uish_module tests/ui-uish/drag.sh griddrop railpointer dualautohidereveal dualdivider
flea_uish_module tests/ui-uish/menus.sh querypaste runscript background hidden reload focus columnsbackground
flea_uish_module tests/ui-uish/backend.sh terminal

flea_uish_module tests/ui-uish/autofit.sh columnautofit
flea_uish_module tests/ui-uish/input.sh shiftscroll overlays

flea_uish_load() {
    [[ "$test_frontend" == gpui ]] || return 0
    python3 -B "$repo/tests/lib/flea_uish_inventory.py" --repo "$repo" --check >/dev/null || return 1
    local module name
    for module in "${flea_uish_modules[@]}"; do
        if [[ ! -f "$repo/$module" ]]; then
            [[ "$module" == tests/ui-uish/common-observers.sh ]] && continue
            printf 'missing GPUI override module: %s\n' "$module" >&2
            return 1
        fi
        . "$repo/$module" || return 1
        for name in ${flea_uish_assignments[$module]}; do
            declare -F "case_$name" >/dev/null || return 1
        done
    done
}
