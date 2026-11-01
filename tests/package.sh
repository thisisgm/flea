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

required_packages=(expect gvfs gvfs-smb gvfs-dnssd gvfs-nfs gvfs-mtp gvfs-gphoto2 gvfs-afc usbmuxd)
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
helper_sha=f4c75e616dd1381b285219415841a2deb9bc9a861998a52aa03a2ace8522d3c8
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

    # The flasher's helper ships executable; its bench seam never ships, so an installed helper refuses --loop-root.
    udisks_listing=$(bsdtar -tvf "$package_file" 2>/dev/null || true)
    # bsdtar -tvf: -rwxr-xr-x  0 root root 18214 Sep 24 12:00 usr/lib/flea/flea-udisks
    if grep -Eq '^-rwxr-xr-x[[:space:]].*[[:space:]]usr/lib/flea/flea-udisks$' <<< "$udisks_listing" \
        && ! grep -Fq 'flea-udisks-seam' <<< "$udisks_listing"; then
        printf 'PASS package flasher helper without its bench seam\n'
    else
        printf 'FAIL package flasher helper is absent, not 0755, or ships its bench seam\n'
        failed=$((failed + 1))
    fi

    # Issue 173: the removal note is a tracked alpm hook, never a scriptlet, so it is a member like any other.
    hook_path=usr/share/libalpm/hooks/flea.hook
    # bsdtar -tvf: -rw-r--r--  0 root root 612 Sep 24 12:00 usr/share/libalpm/hooks/flea.hook
    hook_metadata=$(bsdtar -tvf "$package_file" 2>/dev/null | grep -F " $hook_path" || true)
    if [[ "$hook_metadata" =~ ^-rw-r--r--[[:space:]]+[0-9]+[[:space:]]+root[[:space:]]+root[[:space:]].*[[:space:]]$hook_path$ ]]; then
        printf 'PASS package removal hook %s root:root 0644\n' "$hook_path"
    else
        printf 'FAIL package removal hook %s is absent or not root:root 0644\n' "$hook_path"
        failed=$((failed + 1))
    fi
    # Sample input, packaging/flea.hook's own lines: [Trigger], Operation = Remove, Target = flea, When = PreTransaction.
    if bsdtar -xf "$package_file" -C "$extract_root" "$hook_path" 2>/dev/null && [ -f "$extract_root/$hook_path" ]; then
        for hook_line in '[Trigger]' 'Type = Package' 'Operation = Remove' '[Action]' 'When = PreTransaction'; do
            if grep -Fxq -- "$hook_line" "$extract_root/$hook_path"; then
                printf 'PASS package removal hook has %s\n' "$hook_line"
            else
                printf 'FAIL package removal hook lacks %s\n' "$hook_line"
                failed=$((failed + 1))
            fi
        done
        # Only flea, the one package Flea publishes, triggers the note.
        hook_targets=$(grep '^Target = ' "$extract_root/$hook_path")
        if [ "$hook_targets" = 'Target = flea' ]; then
            printf 'PASS package removal hook targets flea alone\n'
        else
            printf 'FAIL package removal hook targets %s, not flea alone\n' "$(printf '%s' "$hook_targets" | tr '\n' ' ')"
            failed=$((failed + 1))
        fi
        # Sample input: Exec = /usr/bin/printf %s\n "Flea: ..." "Flea: ...", whose first word pacman executes.
        hook_program=$(sed -n 's/^Exec = \([^ ]*\).*$/\1/p' "$extract_root/$hook_path")
        if [ -n "$hook_program" ] && [ "${hook_program#/}" != "$hook_program" ] && [ -x "$hook_program" ]; then
            printf 'PASS package removal hook runs %s, which is here\n' "$hook_program"
        else
            printf "FAIL package removal hook's Exec program '%s' is not an absolute executable\n" "$hook_program"
            failed=$((failed + 1))
        fi
    else
        printf 'FAIL package removal hook could not be extracted\n'
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
