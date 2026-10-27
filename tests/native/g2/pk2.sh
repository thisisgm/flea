#!/bin/bash
# Native picker acceptance in both arms, then scenes; native host only.
set -euo pipefail
: "${W:?absolute staged candidate tree}" "${Q_BIN:?verified v0.3.12 launcher}" "${G_BIN:?verified GPUI flea binary}"
: "${FLEA_VISUAL_DESIGN:?FLEA_VISUAL_DESIGN is unset: set it to the design-hig root}"
[[ $W == /* && -d $W ]] || { printf 'W must be an absolute tree\n' >&2; exit 2; }
cd "$W"
mkdir -p "$W/.superpowers/g2-pk2"
E=$(mktemp -d "$W/.superpowers/g2-pk2/native.XXXXXXXX")
: > "$E/.flea-test-sandbox"
# Board fixture at a fixed root; the step prints it first.
fixture="$(bash "$W/tests/native/g2/pk2-fixture.sh" pk2)" || exit 2
export FLEA_PICKER_FIXTURE="$fixture"
export FLEA_FIXTURE_ROOT="$fixture"
# No adapter yet: the gpui arm stays BLOCKED.
test -x "$W/tests/native/g2/pk2-gpui.sh" || { printf 'BLOCKED: GPUI picker adapter tests/native/g2/pk2-gpui.sh is not registered (tests/picker*.sh hardcode qs)\n' >&2; exit 2; }
suites=(picker-040 picker-grid picker-header picker-hunt picker-recent picker-selection picker-save-wrap picker-issue313-native)
runtime=${XDG_RUNTIME_DIR:?native compositor runtime}
failures="$E/failures"
: > "$failures"
for arm in qml gpui; do
    mkdir -p -m 700 "$E/$arm"/{config,state,cache,runtime,home}
    if [ -d "$runtime/hypr" ]; then ln -s "$runtime/hypr" "$E/$arm/runtime/hypr"; fi
    if [[ ${WAYLAND_DISPLAY:-} != /* && -S "$runtime/${WAYLAND_DISPLAY:-}" ]]; then
        ln -s "$runtime/$WAYLAND_DISPLAY" "$E/$arm/runtime/$WAYLAND_DISPLAY"
    fi
    if [ -S "$runtime/.ydotool_socket" ]; then ln -s "$runtime/.ydotool_socket" "$E/$arm/runtime/.ydotool_socket"; fi
    bin=$Q_BIN
    [ "$arm" != gpui ] || bin=$G_BIN
    for suite in "${suites[@]}"; do
        if [ "$arm" = qml ]; then script="tests/$suite.sh"; else script="tests/native/g2/pk2-gpui.sh $suite"; fi
        # shellcheck disable=SC2086
        env WAYLAND_DISPLAY="${WAYLAND_DISPLAY:-}" FLEA_FRONTEND="$arm" FLEA_BIN="$bin" HOME="$E/$arm/home" \
            XDG_CONFIG_HOME="$E/$arm/config" XDG_STATE_HOME="$E/$arm/state" XDG_CACHE_HOME="$E/$arm/cache" \
            XDG_DATA_HOME="$fixture/data" XDG_RUNTIME_DIR="$E/$arm/runtime" bash $script > "$E/$arm/$suite.log" 2>&1 || printf '%s %s exit %s\n' "$arm" "$suite" "$?" >> "$failures"
    done
done
test -x tools/flea-ci-visual || { printf 'BLOCKED: the GPUI scene CLI tools/flea-ci-visual is absent\n' >&2; exit 2; }
# The tracked manifest keeps the ${FLEA_VISUAL_DESIGN} token; this run reads an expanded copy.
manifest="$E/pk2.json"
json=$(<tests/visual/g2/pk2.json)
case $FLEA_VISUAL_DESIGN in *'"'* | *'\'*) printf 'FLEA_VISUAL_DESIGN holds a quote or backslash\n' >&2; exit 2 ;; esac
printf '%s\n' "${json//'${FLEA_VISUAL_DESIGN}'/"$FLEA_VISUAL_DESIGN"}" > "$manifest"
for arm in qml gpui; do
    tools/flea-ci-visual --frontend "$arm" --manifest "$manifest" --output "$E/visual/$arm"
done
printf 'evidence %s\n' "$E"
[ ! -s "$failures" ]
