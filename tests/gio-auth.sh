#!/usr/bin/env bash
# Proves trust warnings stop before one redacted password-only GIO exchange.
set -u
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1

dir="$FIXTURE_ROOT/flea-gio-auth-$$"
cleanup() { sandbox_remove "$dir"; }
trap cleanup EXIT HUP INT TERM
sandbox_make "$dir"
mkdir -p "$dir/bin"

fake_secret='not-a-real-password'
identity_status='unset'
certificate_status='unset'
cat > "$dir/bin/gio" <<'EOS'
#!/bin/sh
record_answer() {
    IFS= read -r answer || answer=
    printf '%s\n' "$answer" >> "$FAKE_GIO_RECEIVED"
}

case "$FAKE_GIO_FLOW" in
identity)
    printf 'Identity Verification Failed\n[1] Log In Anyway\n[2] Cancel Login\nChoice: '
    record_answer
    printf 'Password: '
    record_answer
    ;;
certificate)
    printf 'Certificate trust warning\nContinue anyway? y/n '
    record_answer
    printf 'Password: '
    record_answer
    ;;
password)
    printf 'Password: '
    record_answer
    printf 'Store password? [never/session/permanent] '
    record_answer
    ;;
no-prompt)
    exit 0
    ;;
*) exit 2 ;;
esac
EOS
chmod +x "$dir/bin/gio"

run_helper() {
    local flow=$1 received=$2 output=$3
    : > "$received"
    printf '%s\n' "$fake_secret" | FAKE_GIO_FLOW="$flow" FAKE_GIO_RECEIVED="$received" \
        FLEA_GIO_AUTH_TIMEOUT=5 PATH="$dir/bin:/usr/bin:/bin" \
        ./tools/flea-gio-auth 'sftp://user@example.test/' > "$output" 2>&1
}

for flow in identity certificate; do
    received="$dir/$flow.received"
    output="$dir/$flow.output"
    run_helper "$flow" "$received" "$output"
    rc=$?
    case "$flow" in
    identity) identity_status=$rc ;;
    certificate) certificate_status=$rc ;;
    esac
    [[ "$rc" -ne 0 ]] || { printf 'gio-auth: FAIL %s warning accepted\n' "$flow"; exit 1; }
    ! grep -Fq -- "$fake_secret" "$received" \
        || { printf 'gio-auth: FAIL %s warning received password\n' "$flow"; exit 1; }
    [[ ! -s "$output" ]] \
        || { printf 'gio-auth: FAIL %s warning produced output\n' "$flow"; exit 1; }
done

received="$dir/no-prompt.received"
output="$dir/no-prompt.output"
run_helper no-prompt "$received" "$output"
rc=$?
no_prompt_status=$rc
[[ "$rc" -ne 0 ]] || { printf 'gio-auth: FAIL no-prompt child accepted\n'; exit 1; }
[[ ! -s "$received" ]] || { printf 'gio-auth: FAIL no-prompt child received input\n'; exit 1; }
[[ ! -s "$output" ]] || { printf 'gio-auth: FAIL no-prompt helper produced output\n'; exit 1; }

received="$dir/password.received"
output="$dir/password.output"
run_helper password "$received" "$output"
rc=$?
[[ "$rc" -eq 0 ]] || { printf 'gio-auth: FAIL password-only helper=%s\n' "$rc"; exit 1; }
[[ "$(grep -Fxc -- "$fake_secret" "$received")" -eq 1 ]] \
    || { printf 'gio-auth: FAIL password delivery was not exactly once\n'; exit 1; }
[[ "$(sed -n '2p' "$received")" == never ]] \
    || { printf 'gio-auth: FAIL storage response was not never\n'; exit 1; }
[[ ! -s "$output" ]] \
    || { printf 'gio-auth: FAIL password-only helper produced output\n'; exit 1; }

printf 'gio-auth: identity=%s certificate=%s no-prompt=%s password=%s once=ok storage=never redaction=ok\n' \
    "$identity_status" "$certificate_status" "$no_prompt_status" "$rc"
