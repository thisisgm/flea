#!/usr/bin/env bash
# Mount a marked fixture only after entering independent user and mount namespaces.
set -euo pipefail
if [[ "${1-}" == --enter ]]; then
    shift
    runtime=$(realpath -e -- "${XDG_RUNTIME_DIR:?runtime}")
    [[ "$runtime" == "${FLEA_TEST_WINDOW:?window}/runtime" && -d "$runtime" && ! -L "$runtime" \
        && -f "$runtime/.flea-test-sandbox" && ! -L "$runtime/.flea-test-sandbox" \
        && "$(stat -c '%u:%a' -- "$runtime")" == "$(id -u):700" ]] \
        || { echo 'GVFS fixture requires its owned transport runtime' >&2; exit 2; }
    # The supervisor's /proc alias is unreadable across a user namespace; this descriptor survives exec.
    exec {runtime_fd}< "$runtime"
    export XDG_RUNTIME_DIR="/proc/self/fd/$runtime_fd/."
    exec unshare --user --map-current-user --keep-caps --mount --propagation private /bin/bash "$0" "$@"
fi
source_root=${1:?source}; target_root=${2:?target}; parent_user=${3:?user namespace}; parent_mount=${4:?mount namespace}
shift 4
[[ "$(readlink /proc/self/ns/user)" != "$parent_user" && "$(readlink /proc/self/ns/mnt)" != "$parent_mount" ]] \
    || { echo 'GVFS fixture requires independent user and mount namespaces' >&2; exit 2; }
[[ "$source_root" == /* && -d "$source_root" && -f "$source_root/.flea-test-sandbox" && ! -L "$source_root" ]] \
    || { echo 'GVFS fixture source must be an absolute marked directory' >&2; exit 2; }
[[ "$target_root" =~ ^/run/user/[0-9]+/gvfs$ && -d "$target_root" && ! -L "$target_root" ]] \
    || { echo 'GVFS fixture target must be an existing canonical GVFS directory' >&2; exit 2; }
mount --make-rprivate /
mount --bind "$source_root" "$target_root"
exec setpriv --inh-caps=-all --ambient-caps=-all --bounding-set=-all -- "$@"
