#!/usr/bin/env bash
# Compare the release recipe with a pinned or externally supplied OPR recipe, offline.
set -euo pipefail
cd "$(dirname "$0")/.."
python3 -B tests/opr_recipe.py PKGBUILD "${FLEA_OPR_PKGBUILD:-tests/fixtures/opr/PKGBUILD}"
