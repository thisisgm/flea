#!/bin/bash
# Run the timing child on the same fixture filesystem as the protocol suites.
set -euo pipefail
cd "$(dirname "$0")/.."
. "$PWD/tools/flea-sandbox-guard"
sandbox_root_ok
probe=$(mktemp -d "$SANDBOX_ROOT/flea-rename-timing.XXXXXX")
: > "$probe/$SANDBOX_MARKER"
trap 'sandbox_remove "$probe"' EXIT
TMPDIR="$probe" cargo test --locked --bin flea \
    backend::renametiming::tests::protocol_100k_twenty -- --ignored --exact --nocapture
