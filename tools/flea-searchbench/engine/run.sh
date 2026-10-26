#!/bin/bash
set -euo pipefail
export LC_ALL=C.UTF-8
unset LANGUAGE
H=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
UNIT=$(dirname -- "$H")
W=$(cd -- "$H/../../.." && pwd -P)

if [[ ${1:-} != --inside ]]; then
    BUILD=${FINDBENCH_BUILD_ROOT:-}
    if [[ -z $BUILD ]]; then
        BUILD=$(python3 - "$UNIT" <<'PY'
import pathlib, sys
roots = [p for p in pathlib.Path(sys.argv[1]).glob('build.*')
         if (p / '.findbench-root').is_file() and (p / 'BUILD-COMPLETE').is_file()]
if not roots:
    raise SystemExit('no completed build root; run build.sh first')
print(max(roots, key=lambda p: p.stat().st_mtime_ns))
PY
)
    fi
    [[ $BUILD = "$UNIT"/build.* && -f $BUILD/.findbench-root && -f $BUILD/BUILD-COMPLETE ]] || exit 2
    SOURCES=${FINDBENCH_SOURCES:-}
    if [[ -z $SOURCES && -f $BUILD/source-root.txt ]]; then
        IFS= read -r SOURCES < "$BUILD/source-root.txt"
    fi
    SOURCES=${SOURCES:-$(dirname -- "$W")/findbench/src}
    [[ $SOURCES = /* && -d $SOURCES ]] || { echo 'absolute source root required' >&2; exit 2; }
    SOURCES=$(cd -- "$SOURCES" && pwd -P)
    for source in fff fsearch fsearch-nd; do
        [[ -d $SOURCES/$source ]] || { echo "source clone missing: $source" >&2; exit 2; }
    done
    python3 -B "$H/provenance.py" check "$H" "$BUILD"
    RUN=$(mktemp -d "$UNIT/run.XXXXXXXX")
    printf '%s\n' findbench1 > "$RUN/.findbench-root"
    printf 'RUN_ROOT=%s\n' "$RUN"
    image_id=$(cat "$BUILD/image-id")
    podman run --rm --network=none --userns=keep-id --cap-drop=all --security-opt=no-new-privileges \
        --volume "$UNIT:/work/.superpowers/fb0:rw" \
        --volume "$SOURCES:/sources:ro" \
        --env FINDBENCH_RUNS="${FINDBENCH_RUNS:-30}" \
        --env FINDBENCH_FIXTURES="${FINDBENCH_FIXTURES:-100000,1000000,source}" \
        --env FINDBENCH_STORAGE="${FINDBENCH_STORAGE:-local}" \
        --env FINDBENCH_CACHE="${FINDBENCH_CACHE:-warm}" \
        "$image_id" /bin/bash /work/.superpowers/fb0/engine/run.sh --inside \
        "/work/.superpowers/fb0/$(basename -- "$BUILD")" \
        "/work/.superpowers/fb0/$(basename -- "$RUN")" \
        > "$RUN/run.log" 2>&1
    printf 'RUN_ROOT=%s\n' "$RUN"
    exit 0
fi

BUILD=${2:?build root required}
RUN=${3:?run root required}
[[ $BUILD = "$UNIT"/build.* && -f $BUILD/.findbench-root && -f $BUILD/BUILD-COMPLETE ]] || exit 2
[[ $RUN = "$UNIT"/run.* && -f $RUN/.findbench-root ]] || exit 2
python3 -B "$H/provenance.py" check "$H" "$BUILD"
sha256sum --check "$BUILD/binaries.sha256" > "$RUN/binary-verification.log"
mkdir -p "$RUN/tmp"
export TMPDIR="$RUN/tmp" PYTHONDONTWRITEBYTECODE=1
python3 "$H/runner.py" --build "$BUILD" --output "$RUN" --runs "${FINDBENCH_RUNS:-30}" \
    --fixtures "${FINDBENCH_FIXTURES:-100000,1000000,source}" --storage "${FINDBENCH_STORAGE:-local}"

