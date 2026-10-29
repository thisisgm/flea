#!/bin/bash
# Controller-only native output acceptance through the real frontend transport.
set -euo pipefail
: "${W:?absolute staged tree}" "${Q_BIN:?verified v0.3.12 launcher}" "${G_BIN:?verified GPUI launcher}"
: "${FLEA_VISUAL_DESIGN:?FLEA_VISUAL_DESIGN is unset: set it to the design-hig root}"
[[ $W == /* && -d $W ]] || exit 2
cd "$W"
source tools/flea-ipc
flea_ipc_init "$G_BIN" gpui
mkdir -p "$W/.superpowers/g2-op4"
output_evidence=$(mktemp -d "$W/.superpowers/g2-op4/native.XXXXXXXX")
: > "$output_evidence/.flea-test-sandbox"
for arm in qml gpui; do
    bin=$Q_BIN
    [ "$arm" != gpui ] || bin=$G_BIN
    mkdir -p -m 700 "$output_evidence/$arm"/{home,config,state,cache,data}
    env FLEA_FRONTEND="$arm" FLEA_BIN="$bin" HOME="$output_evidence/$arm/home" \
        XDG_CONFIG_HOME="$output_evidence/$arm/config" XDG_STATE_HOME="$output_evidence/$arm/state" \
        XDG_CACHE_HOME="$output_evidence/$arm/cache" XDG_DATA_HOME="$output_evidence/$arm/data" \
        bash tests/ui.sh convertdesign > "$output_evidence/$arm/convertdesign.log" 2>&1
 done
test -x tools/flea-ci-visual || { printf 'BLOCKED: tools/flea-ci-visual is absent\n' >&2; exit 2; }
# The tracked manifest keeps the ${FLEA_VISUAL_DESIGN} token; this run reads an expanded copy.
manifest="$output_evidence/op4.json"
json=$(<tests/visual/g2/op4.json)
case $FLEA_VISUAL_DESIGN in *'"'* | *'\'*) printf 'FLEA_VISUAL_DESIGN holds a quote or backslash\n' >&2; exit 2 ;; esac
printf '%s\n' "${json//'${FLEA_VISUAL_DESIGN}'/"$FLEA_VISUAL_DESIGN"}" > "$manifest"
for arm in qml gpui; do
    tools/flea-ci-visual --frontend "$arm" --manifest "$manifest" \
        --output "$output_evidence/visual/$arm" --design "$FLEA_VISUAL_DESIGN"
done
printf 'G2_OP4 NATIVE artifacts %s\n' "$output_evidence"
