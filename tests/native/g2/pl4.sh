#!/bin/bash
# Native acceptance for the places, favorites, tags and plugin rail sections: ui cases and visual scenes in both arms, inside a marked sandbox.
set -euo pipefail
: "${W:?absolute staged candidate tree}" "${Q_BIN:?verified v0.3.12 launcher}" "${G_BIN:?verified GPUI flea binary}"
[[ $W == /* && -d $W ]] || { printf 'W must be an absolute tree\n' >&2; exit 2; }
cd "$W"
mkdir -p "$W/.superpowers/g2-pl4"
E=$(mktemp -d "$W/.superpowers/g2-pl4/native.XXXXXXXX")
: > "$E/.flea-test-sandbox"
runtime=${XDG_RUNTIME_DIR:?native compositor runtime}
for arm in qml gpui; do
    mkdir -p -m 700 "$E/$arm"/{config,state,cache,runtime,home}
    if [ -d "$runtime/hypr" ]; then ln -s "$runtime/hypr" "$E/$arm/runtime/hypr"; fi
    if [[ ${WAYLAND_DISPLAY:-} != /* && -S "$runtime/${WAYLAND_DISPLAY:-}" ]]; then
        ln -s "$runtime/$WAYLAND_DISPLAY" "$E/$arm/runtime/$WAYLAND_DISPLAY"
    fi
    if [ -S "$runtime/.ydotool_socket" ]; then ln -s "$runtime/.ydotool_socket" "$E/$arm/runtime/.ydotool_socket"; fi
    bin=$Q_BIN
    [ "$arm" != gpui ] || bin=$G_BIN
    run() {
        env WAYLAND_DISPLAY="${WAYLAND_DISPLAY:-}" FLEA_FRONTEND="$arm" FLEA_BIN="$bin" HOME="$E/$arm/home" \
            XDG_CONFIG_HOME="$E/$arm/config" XDG_STATE_HOME="$E/$arm/state" XDG_CACHE_HOME="$E/$arm/cache" \
            XDG_RUNTIME_DIR="$E/$arm/runtime" "$@"
    }
    run bash tests/ui.sh places editplace recent > "$E/$arm/ui.log" 2>&1 || printf '%s ui.sh exit %s\n' "$arm" "$?" >> "$E/failures"
    run bash tests/ui-trash.sh railorder raildetails > "$E/$arm/ui-trash.log" 2>&1 || printf '%s ui-trash.sh exit %s\n' "$arm" "$?" >> "$E/failures"
done
test -x tools/flea-ci-visual || { printf 'BLOCKED: the GPUI scene CLI tools/flea-ci-visual is absent\n' >&2; exit 2; }
for arm in qml gpui; do
    tools/flea-ci-visual --frontend "$arm" --manifest tests/visual/g2/pl4.json --output "$E/visual/$arm"
done
printf 'evidence %s\n' "$E"
[ ! -s "$E/failures" ]
