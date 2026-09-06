#!/usr/bin/env bash
# Verifies a clean package install supplies every backend Flea advertises.
set -u
cd "$(dirname "$0")/.." || exit 1

. ./PKGBUILD

failed=0
extract_root=
cleanup_extract() {
    [ -n "$extract_root" ] || return
    [ "$extract_root" != / ] || return
    [ "${extract_root#/}" != "$extract_root" ] || return
    [ -f "$extract_root/.flea-package-test-owned" ] || return
    rm -rf -- "$extract_root"
}
trap cleanup_extract EXIT

required_packages=(expect gvfs gvfs-smb gvfs-dnssd gvfs-nfs)
for package in "${required_packages[@]}"; do
    found=false
    for dependency in "${depends[@]}"; do
        if [ "$dependency" = "$package" ]; then
            found=true
            break
        fi
    done
    if [ "$found" = true ]; then
        printf 'PASS package dependency %s\n' "$package"
    else
        printf 'FAIL package dependency %s is absent\n' "$package"
        failed=$((failed + 1))
    fi
done

helper_path=usr/lib/flea/flea-gio-auth
helper_sha=b8b519d96e2a219ee28588732807083b6556a88f705992d6847fdd645c9b2113
package_file=${FLEA_PACKAGE_FILE:-}
if [ -z "$package_file" ] && command -v makepkg >/dev/null 2>&1; then
    mapfile -t package_files < <(makepkg --packagelist)
    if [ "${#package_files[@]}" -eq 1 ]; then
        package_file=${package_files[0]}
    fi
fi

if [ -z "$package_file" ] || [ ! -f "$package_file" ]; then
    printf 'FAIL package archive is absent; set FLEA_PACKAGE_FILE to a built makepkg archive\n'
    failed=$((failed + 1))
else
    package_info=$(bsdtar -xOf "$package_file" .PKGINFO 2>/dev/null || true)
    for package in "${required_packages[@]}"; do
        if grep -Fxq "depend = $package" <<< "$package_info"; then
            printf 'PASS package metadata dependency %s\n' "$package"
        else
            printf 'FAIL package metadata dependency %s is absent\n' "$package"
            failed=$((failed + 1))
        fi
    done

    member_count=$(bsdtar -tf "$package_file" 2>/dev/null | grep -Fxc "$helper_path")
    if [ "$member_count" -eq 1 ]; then
        printf 'PASS package helper path %s\n' "$helper_path"
    else
        printf 'FAIL package helper path %s occurs %s time(s)\n' "$helper_path" "$member_count"
        failed=$((failed + 1))
    fi

    # bsdtar -tvf: -rwxr-xr-x  0 root root 1327 Sep 04 12:00 usr/lib/flea/flea-gio-auth
    member_metadata=$(bsdtar -tvf "$package_file" 2>/dev/null | grep -F " $helper_path" || true)
    if [[ "$member_metadata" =~ ^-rwxr-xr-x[[:space:]]+[0-9]+[[:space:]]+root[[:space:]]+root[[:space:]].*[[:space:]]$helper_path$ ]]; then
        printf 'PASS package helper metadata root:root 0755\n'
    else
        printf 'FAIL package helper metadata is not root:root 0755\n'
        failed=$((failed + 1))
    fi

    extract_root=$(mktemp -d) || exit 1
    case "$extract_root" in
        /*) : ;;
        *) printf 'FAIL package extraction root is not absolute\n'; exit 1 ;;
    esac
    : > "$extract_root/.flea-package-test-owned" || exit 1
    if bsdtar -xf "$package_file" -C "$extract_root" "$helper_path" 2>/dev/null \
        && [ -f "$extract_root/$helper_path" ] && [ -x "$extract_root/$helper_path" ]; then
        archived_sha=$(sha256sum "$extract_root/$helper_path" | cut -d' ' -f1)
        if [ "$archived_sha" = "$helper_sha" ]; then
            printf 'PASS package helper frozen SHA-256\n'
        else
            printf 'FAIL package helper SHA-256 differs from frozen helper\n'
            failed=$((failed + 1))
        fi
    else
        printf 'FAIL package helper could not be extracted as an executable file\n'
        failed=$((failed + 1))
    fi
    if cleanup_extract; then
        extract_root=
    else
        printf 'FAIL package extraction root could not be removed safely\n'
        failed=$((failed + 1))
    fi
fi

printf 'package: %d failure(s)\n' "$failed"
exit "$failed"
