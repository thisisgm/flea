#!/bin/bash
# Run on the controller's Omarchy machine; all generated files stay in a fresh marked scratch root.
set -euo pipefail
W=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P)
commons=${1:?usage: run.sh OMARCHY_SHELL FROZEN_UI [PALETTE_ROOT...]}
oracle=${2:?usage: run.sh OMARCHY_SHELL FROZEN_UI [PALETTE_ROOT...]}
shift 2
[[ "$commons" = /* && "$oracle" = /* ]] || { echo "absolute source paths required" >&2; exit 2; }
[[ -f "$commons/Commons/Color.qml" && -f "$oracle/ThemeColors.qml" ]] || exit 2
command -v qs >/dev/null || { echo "Quickshell is required" >&2; exit 2; }
mkdir -p -- "$W/.superpowers/g05"
scratch=$(mktemp -d "$W/.superpowers/g05/golden.XXXXXX")
[[ "$scratch" = "$W"/.superpowers/g05/golden.?* ]] || exit 2
printf '%s\n' 'g05 theme golden scratch' > "$scratch/.g05-test-root"
mkdir -p -- "$scratch/home/.local/state/omarchy/current/theme" "$scratch/cache" "$scratch/config"
printf '%s\n' 'background = "#101315"' 'foreground = "#cacccc"' > "$scratch/home/.local/state/omarchy/current/theme/colors.toml"
cp -- "$W/tools/theme-golden/dump.qml" "$scratch/shell.qml"
ln -s -- "$commons/Commons" "$scratch/Commons"
ln -s -- "$oracle" "$scratch/Oracle"
if [ $# -eq 0 ]; then
    set -- "${OMARCHY_PATH:-$HOME/.local/share/omarchy}/themes" "$HOME/.config/omarchy/themes"
fi
python3 "$W/tools/theme-golden/manifest.py" "$scratch" "$commons" "$@"
set +e
HOME="$scratch/home" XDG_CONFIG_HOME="$scratch/config" XDG_CACHE_HOME="$scratch/cache" \
    XDG_STATE_HOME="$scratch/home/.local/state" FLEA_GOLDEN_MANIFEST="$scratch/palettes.json" \
    QSG_RHI_BACKEND=opengl QT_QPA_PLATFORM=offscreen \
    timeout --signal=TERM 120s qs -p "$scratch" > "$scratch/qs.log" 2>&1
status=$?
set -e
[[ "$status" = 0 || "$status" = 143 ]] || { cat "$scratch/qs.log" >&2; exit "$status"; }
python3 - "$scratch" <<'PY'
import json
import pathlib
import sys

scratch = pathlib.Path(sys.argv[1])
lines = (scratch / 'qs.log').read_text().splitlines()
rows = [line.split('THEME_GOLDEN\t', 1)[1] for line in lines if 'THEME_GOLDEN\t' in line]
done = [line for line in lines if 'THEME_GOLDEN_DONE' in line]
if len(done) != 1 or not rows:
    raise SystemExit(f'incomplete QML dump; inspect {scratch}/qs.log')
expected = {p['name'] for p in json.loads((scratch / 'palettes.json').read_text())}
cases = {(r.split('\t')[0], r.split('\t')[1]) for r in rows}
if cases != {(name, str(stop)) for name in expected for stop in (12, 14)}:
    raise SystemExit('palette/stop coverage mismatch')
if len(rows) != len(set('\t'.join(r.split('\t')[:3]) for r in rows)):
    raise SystemExit('duplicate token rows')
print('# QML golden, live Omarchy Commons plus frozen Flea QML, tag rule 2026-10-17')
print('# palette\tstop\ttoken\tvalue; colors are #AARRGGBB; input.* retains reproducible inputs')
print('\n'.join(rows))
print(f'evidence: {scratch}', file=sys.stderr)
PY
