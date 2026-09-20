#!/usr/bin/env bash
# Real QML/Process lifecycle checks, offscreen; needs qs and usable Flea Commons imports.
set -eu
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.."
command -v qs >/dev/null
command -v python3 >/dev/null
fixture="$FIXTURE_ROOT/cloud-ui-$$"
sandbox_make "$fixture"
trap 'sandbox_remove "$fixture"' EXIT
ui=${FLEA_TEST_UI:-$PWD/ui}
ln -s "$ui" "$fixture/ui"
ln -s "$ui/Commons" "$fixture/Commons"
ln -s "$ui/Ui" "$fixture/Ui"
cp tests/cloud-ui.qml "$fixture/shell.qml"
cp tests/cloud-ui-helper.py "$fixture/helper"
chmod +x "$fixture/helper"
for scenario in stale missing hang recover; do
    printf 0 > "$fixture/calls"
    helper="$fixture/helper"
    [[ $scenario != missing ]] || helper="$fixture/absent-helper"
    if ! QT_QPA_PLATFORM=offscreen FLEA_BIN="$helper" FLEA_CLOUD_CASE="$scenario" FLEA_CLOUD_FIXTURE="$fixture" \
        timeout 12 qs -p "$fixture/shell.qml" > "$fixture/output" 2>&1; then
        cat "$fixture/output"; exit 1
    fi
    grep -F "cloud-ui $scenario PASS" "$fixture/output" || { cat "$fixture/output"; exit 1; }
    if [[ $scenario == hang ]] && kill -0 "$(cat "$fixture/helper.pid")" 2>/dev/null; then
        echo 'FAIL: helper survived its deadline'; exit 1
    fi
done
printf 'cloud-ui: 4 lifecycle scenarios passed\n'
