#!/usr/bin/env bash
# The shared native harness owns fixtures and processes; this entry point owns the display lane.
set -euo pipefail
repo="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="$HOME/.local/bin:$PATH"
[[ "$#" == 0 ]] || { printf 'Usage: tests/cardsizes.sh\n' >&2; exit 2; }
lock="/run/user/$(id -u)/flea-display.lock"
exec 9>"$lock"
flock -n 9 || { printf 'REFUSED another native Flea lane owns %s\n' "$lock" >&2; exit 1; }
source_sha=$(git -C "$repo" rev-parse HEAD) || { printf 'ERROR: cardsizes source identity failed\n' >&2; exit 1; }
printf 'CARDSIZES_SOURCE=%s\n' "$source_sha"
sha256sum -- "${FLEA_BIN:-$repo/target/release/flea}"
exec bash "$repo/tests/ui.sh" settingscompact cardsizes
