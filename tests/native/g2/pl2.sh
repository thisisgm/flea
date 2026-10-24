#!/bin/bash
# Controller-only network acceptance; the shared harness owns the isolated window and fixtures.
set -euo pipefail
pl2_repo=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../../.." && pwd -P)
: "${FLEA_BIN:?set the exact staged candidate or frozen oracle binary}"
source "$pl2_repo/tools/flea-ipc"
flea_ipc_init "$FLEA_BIN" "${FLEA_FRONTEND:-gpui}"
export FLEA_FRONTEND=${FLEA_FRONTEND:-gpui}
if [ "$#" -eq 0 ]; then
    set -- network netmark networkauth networktimeout gvfs_cleanup gvfs sharebrowser hangshare hanglisting hanginspect
fi
exec /bin/bash "$pl2_repo/tests/ui.sh" "$@"
