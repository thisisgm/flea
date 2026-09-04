#!/usr/bin/env bash
# Runs opt-in destructive network tests inside marker-owned directories on approved test shares.
set -euo pipefail
cd "$(dirname "$0")/.." || exit 1

if [[ "${FLEA_NETWORK_SELF_TEST:-}" == "1" && -z "${FLEA_NETWORK_SELF_TEST_CHILD:-}" ]]; then
    . ./tools/flea-sandbox-guard
    self_dir="$FIXTURE_ROOT/flea-network-live-$$"
    self_cleanup() { sandbox_remove "$self_dir"; }
    trap self_cleanup EXIT HUP INT TERM
    sandbox_make "$self_dir"
    mkdir -p "$self_dir/bin" "$self_dir/remote/home42/tester" "$self_dir/state"
    : > "$self_dir/state/mounts"
    : > "$self_dir/state/operations"

    cat > "$self_dir/bin/timeout" <<'EOS'
#!/bin/sh
shift
exec "$@"
EOS
    cat > "$self_dir/bin/gio" <<'EOS'
#!/bin/sh
operation=$1
shift
sftp_uri="sftp://$FAKE_SLOT_USER@$FAKE_SLOT_HOST/"
webdav_uri="davs://$FAKE_SLOT_USER@$FAKE_SLOT_USER.$FAKE_SLOT_HOST/webdav"

uri_path() {
    case "$1" in
    "$sftp_uri"*) printf '%s/%s\n' "$FAKE_GIO_REMOTE" "${1#"$sftp_uri"}" ;;
    "$webdav_uri") printf '%s/home42/%s\n' "$FAKE_GIO_REMOTE" "$FAKE_SLOT_USER" ;;
    "$webdav_uri/"*) printf '%s/home42/%s/%s\n' "$FAKE_GIO_REMOTE" "$FAKE_SLOT_USER" "${1#"$webdav_uri"/}" ;;
    *) exit 2 ;;
    esac
}

printf '%s %s\n' "$operation" "$*" >> "$FAKE_GIO_STATE/operations"
case "$operation" in
mount)
    if [ "${1:-}" = --anonymous ]; then
        shift
    fi
    if [ "${1:-}" = -u ]; then
        uri=$2
        : > "$FAKE_GIO_STATE/mounts.next"
        while IFS= read -r mounted; do
            [ "$mounted" = "$uri" ] || printf '%s\n' "$mounted" >> "$FAKE_GIO_STATE/mounts.next"
        done < "$FAKE_GIO_STATE/mounts"
        mv "$FAKE_GIO_STATE/mounts.next" "$FAKE_GIO_STATE/mounts"
        exit 0
    fi
    if [ "${1:-}" = -l ]; then
        while IFS= read -r mounted; do
            [ -n "$mounted" ] && printf 'Mount(0): fake -> %s\n' "$mounted"
        done < "$FAKE_GIO_STATE/mounts"
        exit 0
    fi
    uri=$1
    printf 'Password: '
    IFS= read -r password || exit 1
    printf 'Store password? [never/session/permanent] '
    IFS= read -r store || exit 1
    [ "$password" = "$FAKE_GIO_SECRET" ] && [ "$store" = never ] || exit 1
    grep -Fxq -- "$uri" "$FAKE_GIO_STATE/mounts" || printf '%s\n' "$uri" >> "$FAKE_GIO_STATE/mounts"
    ;;
info)
    if [ "${FAKE_GIO_FAIL_INFO_AFTER_ALPHA:-}" = 1 ] && [ -f "$FAKE_GIO_STATE/deny-info" ]; then
        exit 1
    fi
    path=$(uri_path "$1")
    [ -e "$path" ] || exit 1
    printf 'local path: %s\n' "$path"
    ;;
list)
    [ "${FAKE_GIO_FAIL_LIST:-}" != 1 ] || exit 1
    path=$(uri_path "$1")
    [ -d "$path" ] || exit 1
    for child in "$path"/*; do
        [ -e "$child" ] || continue
        name=${child##*/}
        [ -d "$child" ] && name="$name/"
        printf '%s\n' "$name"
    done
    ;;
mkdir)
    path=$(uri_path "$1")
    mkdir "$path"
    ;;
save)
    [ "$1" = -c ] || exit 2
    path=$(uri_path "$2")
    cat > "$path"
    case "$2" in */alpha.txt) : > "$FAKE_GIO_STATE/deny-info" ;; esac
    ;;
cat)
    case "$1" in
    */alpha.txt) [ "${FAKE_GIO_FAIL_CAT_ALPHA:-}" != 1 ] || exit 1 ;;
    esac
    path=$(uri_path "$1")
    cat "$path"
    ;;
remove)
    path=$(uri_path "$1")
    if [ -d "$path" ]; then
        rmdir "$path"
    else
        rm -f "$path"
    fi
    ;;
*) exit 2 ;;
esac
EOS
    cat > "$self_dir/bin/flea" <<'EOS'
#!/bin/sh
exit 0
EOS
    chmod +x "$self_dir/bin/timeout" "$self_dir/bin/gio" "$self_dir/bin/flea"

    self_failures=0
    self_fail() {
        printf 'network-live-self-test: FAIL %s\n' "$*" >&2
        self_failures=$((self_failures + 1))
    }
    run_id=$$
    fake_secret='not-a-real-network-password'
    bash_dir=$(dirname "$(command -v bash)")
    child_env=(
        FLEA_NETWORK_LIVE=1
        FLEA_BIN="$self_dir/bin/flea"
        FLEA_NETWORK_SELF_TEST_CHILD=1
        FLEA_NETWORK_TEST_GVFS_ROOT="$self_dir/remote"
        FLEA_NETWORK_TEST_CASE_ID="$run_id"
        FAKE_GIO_STATE="$self_dir/state"
        FAKE_GIO_REMOTE="$self_dir/remote"
        FAKE_GIO_SECRET="$fake_secret"
        FAKE_SLOT_USER=tester
        FAKE_SLOT_HOST=slot.test
        PATH="$self_dir/bin:$bash_dir:/usr/bin:/bin"
    )

    webdav_output="$self_dir/webdav.output"
    if printf '%s\n%s\n' "$fake_secret" "$fake_secret" | env "${child_env[@]}" \
        FLEA_NETWORK_TEST_CASE=webdav-failure ./tests/network-live.sh tester slot.test unraid.test \
        > "$webdav_output" 2>&1; then
        webdav_status=0
    else
        webdav_status=$?
    fi
    [[ "$webdav_status" -ne 0 ]] || self_fail "WebDAV failure hook returned zero"
    webdav_last=$(tail -n 1 "$webdav_output")
    grep -Fq 'WebDAV intentional failure after marker' "$webdav_output" \
        || self_fail "WebDAV failure hook did not run: $webdav_last"
    webdav_dir="$self_dir/remote/home42/tester/000-flea-network-test-webdav-$run_id"
    [[ ! -e "$webdav_dir" ]] || self_fail "WebDAV fallback left $webdav_dir"
    grep -Fq 'list sftp://tester@slot.test/' "$self_dir/state/operations" \
        || self_fail "WebDAV cleanup did not resolve Ultra home over SFTP"
    ! PATH="$self_dir/bin:/usr/bin:/bin" FAKE_GIO_STATE="$self_dir/state" \
        FAKE_GIO_REMOTE="$self_dir/remote" FAKE_SLOT_USER=tester FAKE_SLOT_HOST=slot.test \
        gio mount -l | grep -Fq -- '-> davs://tester@tester.slot.test/webdav' \
        || self_fail "WebDAV test URI remained mounted"
    ! grep -Fq -- "$fake_secret" "$webdav_output" \
        || self_fail "WebDAV output contained password"

    : > "$self_dir/state/operations"
    printf '%s\n' 'sftp://tester@slot.test/' > "$self_dir/state/mounts"
    sftp_output="$self_dir/sftp.output"
    if printf '%s\n%s\n' "$fake_secret" "$fake_secret" | env "${child_env[@]}" \
        FAKE_GIO_FAIL_CAT_ALPHA=1 FLEA_NETWORK_TEST_CASE=premounted-sftp \
        ./tests/network-live.sh tester slot.test unraid.test > "$sftp_output" 2>&1; then
        sftp_status=0
    else
        sftp_status=$?
    fi
    [[ "$sftp_status" -ne 0 ]] || self_fail "pre-mounted SFTP returned zero"
    sftp_last=$(tail -n 1 "$sftp_output")
    grep -Fq 'REFUSED' "$sftp_output" || self_fail "pre-mounted SFTP was not refused: $sftp_last"
    ! grep -Eq '^(mkdir|save) ' "$self_dir/state/operations" \
        || self_fail "pre-mounted SFTP performed a write"
    grep -Fxq 'sftp://tester@slot.test/' "$self_dir/state/mounts" \
        || self_fail "pre-mounted SFTP setup mount was removed"
    ! grep -Fq -- "$fake_secret" "$sftp_output" \
        || self_fail "pre-mounted SFTP output contained password"
    PATH="$self_dir/bin:/usr/bin:/bin" FAKE_GIO_STATE="$self_dir/state" \
        FAKE_GIO_REMOTE="$self_dir/remote" FAKE_SLOT_USER=tester FAKE_SLOT_HOST=slot.test \
    gio mount -u 'sftp://tester@slot.test/'
    [[ ! -s "$self_dir/state/mounts" ]] || self_fail "setup teardown did not remove SFTP mount"

    : > "$self_dir/state/operations"
    failure_id=$((run_id + 1))
    cleanup_output="$self_dir/cleanup.output"
    if printf '%s\n%s\n' "$fake_secret" "$fake_secret" | env "${child_env[@]}" \
        FLEA_NETWORK_TEST_CASE_ID="$failure_id" FAKE_GIO_FAIL_LIST=1 \
        FLEA_NETWORK_TEST_CASE=webdav-failure ./tests/network-live.sh tester slot.test unraid.test \
        > "$cleanup_output" 2>&1; then
        cleanup_status=0
    else
        cleanup_status=$?
    fi
    [[ "$cleanup_status" -ne 0 ]] || self_fail "failed fallback returned zero"
    grep -Fq 'network-live: FAIL cleanup did not remove owned test state' "$cleanup_output" \
        || self_fail "failed fallback was not surfaced"
    cleanup_dir="$self_dir/remote/home42/tester/000-flea-network-test-webdav-$failure_id"
    [[ -f "$cleanup_dir/.flea-test-sandbox" ]] \
        || self_fail "failed fallback did not preserve marker"
    [[ ! -s "$self_dir/state/mounts" ]] || self_fail "failed fallback left a mount"
    ! grep -Fq -- "$fake_secret" "$cleanup_output" \
        || self_fail "failed fallback output contained password"

    : > "$self_dir/state/operations"
    rm -f "$self_dir/state/deny-info"
    inaccessible_id=$((run_id + 2))
    inaccessible_output="$self_dir/inaccessible.output"
    if printf '%s\n%s\n' "$fake_secret" "$fake_secret" | env "${child_env[@]}" \
        FLEA_NETWORK_TEST_CASE_ID="$inaccessible_id" FAKE_GIO_FAIL_INFO_AFTER_ALPHA=1 \
        FAKE_GIO_FAIL_CAT_ALPHA=1 FLEA_NETWORK_TEST_CASE=post-marker-sftp \
        ./tests/network-live.sh tester slot.test unraid.test > "$inaccessible_output" 2>&1; then
        inaccessible_status=0
    else
        inaccessible_status=$?
    fi
    [[ "$inaccessible_status" -ne 0 ]] || self_fail "inaccessible marker-owned SFTP returned zero"
    grep -Fq 'network-live: FAIL cleanup did not remove owned test state' "$inaccessible_output" \
        || self_fail "inaccessible marker-owned SFTP was not surfaced"
    inaccessible_dir="$self_dir/remote/home42/tester/000-flea-network-test-sftp-$inaccessible_id"
    [[ -f "$inaccessible_dir/.flea-test-sandbox" ]] \
        || self_fail "inaccessible marker-owned SFTP lost its marker"
    [[ ! -s "$self_dir/state/mounts" ]] || self_fail "inaccessible marker-owned SFTP left a mount"
    ! grep -Fq -- "$fake_secret" "$inaccessible_output" \
        || self_fail "inaccessible marker-owned SFTP output contained password"

    if [[ "$self_failures" -ne 0 ]]; then
        printf 'network-live-self-test: %s failure(s)\n' "$self_failures" >&2
        exit 1
    fi
    printf 'network-live-self-test: auth=trusted marker=fallback mount=owned refusal=ok cleanup=fails-loud sftp-inaccessible=fails-loud redaction=ok\n'
    exit 0
fi

fail() {
    printf 'network-live: FAIL %s\n' "$*" >&2
    exit 1
}

refuse() {
    printf 'network-live: REFUSED %s\n' "$*" >&2
    exit 1
}

[[ "${FLEA_NETWORK_LIVE:-}" == "1" ]] || fail "set FLEA_NETWORK_LIVE=1"
[[ "$#" -eq 3 ]] || fail "usage: network-live.sh USER ULTRA_HOST UNRAID_HOST"
slot_user=$1
slot_host=$2
unraid_host=$3
[[ "$slot_user" =~ ^[A-Za-z0-9._-]+$ ]] || fail "invalid Ultra username"
[[ "$slot_host" =~ ^[A-Za-z0-9._-]+$ ]] || fail "invalid Ultra host"
[[ "$unraid_host" =~ ^[A-Za-z0-9._-]+$ ]] || fail "invalid Unraid host"
flea_bin=${FLEA_BIN:-./target/debug/flea}
[[ -x "$flea_bin" ]] || fail "build Flea before live network tests"
IFS= read -r slot_password || fail "missing Ultra slot password on stdin"
IFS= read -r webdav_password || fail "missing Ultra WebDAV password on stdin"
[[ -n "$slot_password" && -n "$webdav_password" ]] || fail "empty network credential"

mounted_uri=""
case_uri=""
case_token=""
case_marker_owned=0

cleanup_case() {
    local run_status=$? cleanup_status=0 cleanup_dir="" cleanup_token="" marker="" mount_status
    trap - EXIT HUP INT TERM
    if [[ "$case_marker_owned" -eq 1 && "$case_uri" == davs://* && -n "$case_token" ]]; then
        cleanup_dir=${case_uri##*/}
        cleanup_token=$case_token
    elif [[ "$case_marker_owned" -eq 1 && -n "$case_uri" && -n "$case_token" ]]; then
        if gio info "$case_uri" >/dev/null 2>&1; then
            marker=$(gio cat "$case_uri/.flea-test-sandbox" 2>/dev/null) \
                || cleanup_status=1
            if [[ "$marker" == "$case_token" ]]; then
                for name in renamed.txt beta.txt alpha.txt; do
                    if gio info "$case_uri/$name" >/dev/null 2>&1; then
                        gio remove "$case_uri/$name" >/dev/null 2>&1 || cleanup_status=1
                    fi
                done
                if gio info "$case_uri/subdir" >/dev/null 2>&1; then
                    gio remove "$case_uri/subdir" >/dev/null 2>&1 || cleanup_status=1
                fi
                if [[ "$cleanup_status" -eq 0 ]]; then
                    gio remove "$case_uri/.flea-test-sandbox" >/dev/null 2>&1 || cleanup_status=1
                fi
                if [[ "$cleanup_status" -eq 0 ]]; then
                    gio remove "$case_uri" >/dev/null 2>&1 || cleanup_status=1
                fi
            else
                cleanup_status=1
            fi
        else
            cleanup_status=1
        fi
    fi
    if [[ -n "$mounted_uri" ]]; then
        if uri_is_mounted "$mounted_uri"; then
            gio mount -u "$mounted_uri" >/dev/null 2>&1 || cleanup_status=1
        else
            mount_status=$?
            [[ "$mount_status" -eq 1 ]] || cleanup_status=1
        fi
    fi
    mounted_uri=""
    case_uri=""
    case_token=""
    case_marker_owned=0
    if [[ -n "$cleanup_dir" ]]; then
        cleanup_webdav_over_sftp "$cleanup_dir" "$cleanup_token" || cleanup_status=1
    fi
    slot_password=""
    webdav_password=""
    if [[ "$cleanup_status" -ne 0 ]]; then
        printf 'network-live: FAIL cleanup did not remove owned test state\n' >&2
        exit 1
    fi
    exit "$run_status"
}
trap cleanup_case EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM

uri_is_mounted() {
    local mounts
    mounts=$(gio mount -l 2>/dev/null) || return 2
    grep -Fq -- "-> $1" <<< "$mounts"
}

require_uri_unmounted() {
    local uri=$1 label=$2 status
    if uri_is_mounted "$uri"; then
        refuse "$label URI is already mounted"
    else
        status=$?
        [[ "$status" -eq 1 ]] || fail "$label cannot inspect existing mounts"
    fi
}

assert_uri_unmounted() {
    local uri=$1 label=$2 status
    if uri_is_mounted "$uri"; then
        fail "$label survived unmount"
    else
        status=$?
        [[ "$status" -eq 1 ]] || fail "$label cannot inspect mounts"
    fi
}

mount_with() {
    local mode=$1 uri=$2 password=$3
    case "$mode" in
    anonymous) timeout 30 gio mount --anonymous "$uri" >/dev/null 2>&1 ;;
    none) timeout 30 gio mount "$uri" >/dev/null 2>&1 ;;
    password) printf '%s\n' "$password" | FLEA_GIO_AUTH_TIMEOUT=30 ./tools/flea-gio-auth "$uri" ;;
    *) return 2 ;;
    esac
}

ultra_home_dir() {
    local uri=$1 candidate_home candidate_uri
    while IFS= read -r candidate_home; do
        candidate_home=${candidate_home%/}
        [[ "$candidate_home" =~ ^home[0-9]+$ ]] || continue
        candidate_uri="${uri%/}/$candidate_home/$slot_user"
        if timeout 5 gio info "$candidate_uri" >/dev/null 2>&1; then
            printf '%s\n' "$candidate_home"
            return 0
        fi
    done < <(timeout 10 gio list "$uri" 2>/dev/null)
    return 1
}

cleanup_webdav_over_sftp() (
    local test_dir=$1 token=$2 uri local_path home_dir target marker mount_owned=0
    cleanup_mount() {
        local cleanup_status=$?
        trap - EXIT
        if [[ "$mount_owned" -eq 1 ]] && ! gio mount -u "$uri" >/dev/null 2>&1; then
            printf 'network-live: cleanup stage=unmount failed\n' >&2
            cleanup_status=1
        fi
        exit "$cleanup_status"
    }
    trap cleanup_mount EXIT
    [[ "$test_dir" =~ ^000-flea-network-test-webdav-[0-9]+$ ]] || return 1
    uri="sftp://$slot_user@$slot_host/"
    require_uri_unmounted "$uri" "WebDAV cleanup"
    mount_with password "$uri" "$slot_password" || { printf 'network-live: cleanup stage=mount failed\n' >&2; return 1; }
    mount_owned=1
    local_path=$(gio info "$uri" 2>/dev/null | sed -n 's/^local path: //p')
    [[ -n "$local_path" ]] || { printf 'network-live: cleanup stage=fuse failed\n' >&2; return 1; }
    home_dir=$(ultra_home_dir "$uri") || { printf 'network-live: cleanup stage=home failed\n' >&2; return 1; }
    target="$local_path/$home_dir/$slot_user/$test_dir"
    [[ -e "$target" ]] || return 0
    [[ -f "$target/.flea-test-sandbox" ]] \
        || { printf 'network-live: cleanup stage=marker-missing failed\n' >&2; return 1; }
    marker=$(<"$target/.flea-test-sandbox")
    [[ "$marker" == "$token" ]] \
        || { printf 'network-live: cleanup stage=marker-mismatch failed\n' >&2; return 1; }
    rm -f "$target/renamed.txt" "$target/beta.txt" "$target/alpha.txt" \
        || { printf 'network-live: cleanup stage=payload failed\n' >&2; return 1; }
    if [[ -d "$target/subdir" ]]; then
        rmdir "$target/subdir" \
            || { printf 'network-live: cleanup stage=subdir failed\n' >&2; return 1; }
    fi
    rm -f "$target/.flea-test-sandbox" \
        || { printf 'network-live: cleanup stage=marker failed\n' >&2; return 1; }
    rmdir "$target" || { printf 'network-live: cleanup stage=rmdir failed\n' >&2; return 1; }
)

run_case() {
    local protocol=$1 uri=$2 mode=$3 password=$4 resolver=$5 ui_mode=${6:-drive}
    local test_dir local_path work_uri work_path product_uri product_root case_path content
    local home_dir candidate_home candidate_uri backend_output rmdir_mode case_id
    local uri_rest authority form_user form_host form_path
    local -a ui_env
    printf 'network-live: %s begin\n' "$protocol"
    require_uri_unmounted "$uri" "$protocol"
    mount_with "$mode" "$uri" "$password" || fail "$protocol mount"
    mounted_uri=$uri

    local_path=$(gio info "$uri" 2>/dev/null | sed -n 's/^local path: //p')
    if [[ -n "${FLEA_NETWORK_SELF_TEST_CHILD:-}" ]]; then
        [[ "$local_path" == "${FLEA_NETWORK_TEST_GVFS_ROOT:?}/"* && -d "$local_path" ]] \
            || fail "$protocol has no self-test FUSE path"
    else
        [[ "$local_path" == "/run/user/$(id -u)/gvfs/"* && -d "$local_path" ]] \
            || fail "$protocol has no GVFS FUSE path"
    fi

    work_uri=${uri%/}
    work_path=$local_path
    if [[ "$resolver" == "ultra-home" ]]; then
        home_dir=$(ultra_home_dir "$uri") || fail "$protocol Ultra home not accessible"
        work_uri="${uri%/}/$home_dir/$slot_user"
        work_path="$local_path/$home_dir/$slot_user"
    fi
    product_uri=$uri
    product_root=$local_path
    if [[ "$resolver" == "ultra-home" ]]; then
        product_uri=$work_uri
        product_root=$work_path
    fi

    case_id=${FLEA_NETWORK_TEST_CASE_ID:-$$}
    [[ "$case_id" =~ ^[0-9]+$ ]] || fail "$protocol invalid test case ID"
    test_dir="000-flea-network-test-${protocol,,}-$case_id"
    case_uri="$work_uri/$test_dir"
    case_path="$work_path/$test_dir"
    ! gio info "$case_uri" >/dev/null 2>&1 || fail "$protocol sandbox already exists"
    case_token="flea-network-live-${protocol,,}-$case_id"
    content="flea-${protocol,,}-roundtrip-$case_id"

    gio mkdir "$case_uri" >/dev/null || fail "$protocol sandbox mkdir"
    printf '%s\n' "$case_token" | gio save -c "$case_uri/.flea-test-sandbox" >/dev/null \
        || fail "$protocol marker write"
    case_marker_owned=1
    printf '%s\n' "$content" | gio save -c "$case_uri/alpha.txt" >/dev/null \
        || fail "$protocol write"
    if [[ "$protocol" == WebDAV && "${FLEA_NETWORK_TEST_CASE:-}" == webdav-failure ]]; then
        fail "$protocol intentional failure after marker"
    fi
    [[ "$(gio cat "$case_uri/alpha.txt")" == "$content" ]] || fail "$protocol read"
    [[ -f "$case_path/alpha.txt" ]] || fail "$protocol FUSE read"

    if [[ "$ui_mode" == drive ]]; then
        gio mount -u "$uri" >/dev/null || fail "$protocol cold-ui setup unmount"
        mounted_uri=""
        uri_rest=${product_uri#*://}
        authority=${uri_rest%%/*}
        form_path=${uri_rest#*/}
        [[ "$form_path" == "$uri_rest" ]] && form_path=""
        if [[ "$authority" == *@* ]]; then
            form_user=${authority%@*}
            form_host=${authority##*@}
        else
            form_user=""
            form_host=$authority
        fi
        ui_env=(
            "FLEA_BIN=$flea_bin"
            "FLEA_GIO_AUTH=$PWD/tools/flea-gio-auth"
            "FLEA_NETWORK_LIVE_URI=$product_uri"
            "FLEA_NETWORK_LIVE_MOUNT_URI=$uri"
            "FLEA_NETWORK_LIVE_ROOT=$product_root"
            "FLEA_NETWORK_LIVE_RELATIVE=${case_path#"$product_root"/}"
            "FLEA_NETWORK_LIVE_PROTOCOL=$protocol"
            "FLEA_NETWORK_LIVE_HOST=$form_host"
            "FLEA_NETWORK_LIVE_PATH=$form_path"
            "FLEA_NETWORK_LIVE_USER=$form_user"
            "FLEA_NETWORK_LIVE_AUTH=$([[ "$mode" == password ]] && printf password || printf none)"
        )
        mounted_uri=$uri
        if [[ "$mode" == password ]]; then
            printf '%s' "$password" | env "${ui_env[@]}" ./tests/ui.sh networklive
        else
            env "${ui_env[@]}" ./tests/ui.sh networklive
        fi || fail "$protocol Flea UI"
        mounted_uri=""
        mount_with "$mode" "$uri" "$password" || fail "$protocol remount after Flea UI"
        mounted_uri=$uri
        local_path=$(gio info "$uri" 2>/dev/null | sed -n 's/^local path: //p')
        if [[ "$resolver" == "ultra-home" ]]; then
            work_path="$local_path/$home_dir/$slot_user"
        else
            work_path=$local_path
        fi
        case_path="$work_path/$test_dir"
        [[ -f "$case_path/alpha.txt" ]] || fail "$protocol file absent after Flea UI"
    elif [[ "$ui_mode" == skip ]]; then
        printf 'network-live: %s focused ui=skipped; full UI remains B3/V2\n' "$protocol"
    else
        fail "$protocol invalid UI mode"
    fi

    cp "$case_path/alpha.txt" "$case_path/beta.txt" || fail "$protocol copy"
    [[ "$(<"$case_path/beta.txt")" == "$content" ]] || fail "$protocol copied content"
    backend_output=$(printf '%s\n' \
        "{\"c\":\"rename\",\"path\":\"$case_path/beta.txt\",\"to\":\"renamed.txt\"}" \
        '{"c":"undo"}' \
        "{\"c\":\"rename\",\"path\":\"$case_path/beta.txt\",\"to\":\"renamed.txt\"}" \
        '{"c":"quit"}' | "$flea_bin" --backend 2>/dev/null)
    [[ "$(grep -c '"t":"renamed","ok":true' <<< "$backend_output")" -eq 2 ]] \
        || fail "$protocol Flea rename"
    [[ "$(grep -c '"t":"undone","op":"rename","ok":true' <<< "$backend_output")" -eq 1 ]] \
        || fail "$protocol Flea rename undo"
    [[ -f "$case_path/renamed.txt" && ! -e "$case_path/beta.txt" ]] || fail "$protocol renamed state"
    rmdir_mode=fuse
    if [[ "$protocol" != "WebDAV" ]]; then
        mkdir "$case_path/subdir" || fail "$protocol nested mkdir"
        rmdir "$case_path/subdir" || fail "$protocol nested rmdir"
    else
        rmdir_mode=sftp-cleanup
    fi
    rm "$case_path/renamed.txt" || fail "$protocol delete"
    rm "$case_path/alpha.txt" || fail "$protocol source delete"
    if [[ "$protocol" == "WebDAV" ]]; then
        gio mount -u "$uri" >/dev/null || fail "$protocol unmount before cleanup"
        mounted_uri=""
        cleanup_webdav_over_sftp "$test_dir" "$case_token" || fail "$protocol SFTP cleanup"
    else
        gio remove "$case_uri/.flea-test-sandbox" >/dev/null || fail "$protocol marker delete"
        gio remove "$case_uri" >/dev/null || fail "$protocol sandbox rmdir"
    fi
    case_marker_owned=0
    case_uri=""
    case_token=""

    if [[ -n "$mounted_uri" ]]; then
        gio mount -u "$uri" >/dev/null || fail "$protocol unmount"
        mounted_uri=""
    fi
    assert_uri_unmounted "$uri" "$protocol"
    mount_with "$mode" "$uri" "$password" || fail "$protocol remount"
    mounted_uri=$uri
    gio mount -u "$uri" >/dev/null || fail "$protocol second unmount"
    mounted_uri=""
    printf 'network-live: %s mount=ok io=ok fuse=ok rmdir=%s unmount=ok remount=ok\n' "$protocol" "$rmdir_mode"
}

expect_mount_failure() {
    local label=$1 uri=$2 mode=$3 password=$4 mount_status
    require_uri_unmounted "$uri" "$label"
    if mount_with "$mode" "$uri" "$password"; then
        gio mount -u "$uri" >/dev/null 2>&1 || fail "$label cleanup unmount"
        fail "$label unexpectedly mounted"
    fi
    if uri_is_mounted "$uri"; then
        gio mount -u "$uri" >/dev/null 2>&1 || fail "$label failed-mount cleanup"
        fail "$label helper failed after creating a mount"
    else
        mount_status=$?
        [[ "$mount_status" -eq 1 ]] || fail "$label cannot inspect failed mount"
    fi
    printf 'network-live: %s rejected=ok\n' "$label"
}

if [[ -n "${FLEA_NETWORK_CLEAN_WEBDAV_DIR:-}" ]]; then
    cleanup_webdav_over_sftp "$FLEA_NETWORK_CLEAN_WEBDAV_DIR" \
        "flea-network-live-webdav-${FLEA_NETWORK_CLEAN_WEBDAV_DIR##*-}" \
        || fail "WebDAV recovery cleanup"
    printf 'network-live: WebDAV recovery cleanup=ok\n'
    exit 0
fi

live_case=${FLEA_NETWORK_LIVE_CASE:-}
[[ -z "$live_case" || "$live_case" == ftps ]] || fail "unknown live network case"
selected_case=${live_case:-${FLEA_NETWORK_TEST_CASE:-}}
case "$selected_case" in
ftps)
    run_case FTPS "ftps://$slot_user@$slot_host/" password "$slot_password" root
    ;;
webdav-failure)
    run_case WebDAV "davs://$slot_user@$slot_user.$slot_host/webdav" password "$webdav_password" root
    ;;
premounted-sftp)
    run_case SFTP "sftp://$slot_user@$slot_host/" password "$slot_password" ultra-home
    ;;
post-marker-sftp)
    run_case SFTP "sftp://$slot_user@$slot_host/" password "$slot_password" ultra-home
    ;;
"")
    run_case SFTP "sftp://$slot_user@$slot_host/" password "$slot_password" ultra-home
    run_case FTPS "ftps://$slot_user@$slot_host/" password "$slot_password" root
    run_case WebDAV "davs://$slot_user@$slot_user.$slot_host/webdav" password "$webdav_password" root
    run_case SMB "smb://$unraid_host/data" anonymous "" root
    run_case NFS "nfs://$unraid_host/mnt/user/data" none "" root
    ;;
*) fail "unknown network test case" ;;
esac

if [[ -z "$selected_case" ]]; then
    expect_mount_failure FTP-plaintext "ftp://$slot_user@$slot_host/" password "$slot_password"
    expect_mount_failure WebDAV-plaintext "dav://$slot_user@$slot_user.$slot_host/webdav" password "$webdav_password"
    expect_mount_failure WebDAV-wrong-password "davs://$slot_user@$slot_user.$slot_host/webdav" password "flea-known-wrong-password"
    expect_mount_failure unreachable "smb://198.51.100.1/flea-unreachable" anonymous ""
fi

slot_password=""
webdav_password=""
if [[ -z "$selected_case" ]]; then
    printf 'network-live: 5 backends passed, 4 negative controls passed\n'
fi
