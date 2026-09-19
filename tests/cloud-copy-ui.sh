#!/usr/bin/env bash
# Own offscreen QML process with fake transport: no production config, mount or uploads.
set -eu
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.."
fixture="$FIXTURE_ROOT/cloud-copy-ui-$$"
sandbox_make "$fixture"
trap 'sandbox_remove "$fixture"' EXIT
cp -a ui "$fixture/ui"
if [[ -n ${FLEA_TEST_COMMONS:-} ]]; then
    ln -sfn "$FLEA_TEST_COMMONS" "$fixture/ui/Commons"
fi
ln -s "$fixture/ui/Commons" "$fixture/Commons"
cp tests/cloud-copy-ui.qml "$fixture/shell.qml"
cp tests/cloud-copy-ui-helper.py "$fixture/helper"
chmod +x "$fixture/helper"
for scenario in success badexit cancel missing dialog cancelrace terminalcancel; do
    helper="$fixture/helper"
    [[ $scenario != missing ]] || helper="$fixture/missing"
    if ! QT_QPA_PLATFORM=offscreen FLEA_BIN="$helper" FLEA_COPY_CASE="$scenario" timeout 13 qs -p "$fixture/shell.qml" > "$fixture/output" 2>&1; then
        cat "$fixture/output"; exit 1
    fi
    if grep -E "ReferenceError|TypeError|Cannot assign|Unable to assign|is not a type" "$fixture/output"; then cat "$fixture/output"; exit 1; fi
    grep -F "cloud-copy-ui $scenario PASS" "$fixture/output" || { cat "$fixture/output"; exit 1; }
done
printf 'cloud-copy-ui: 7 lifecycle cases passed\n'
