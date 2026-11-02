#!/bin/bash
set -euo pipefail
repo=$(cd "$(dirname "$0")/.." && pwd)
root=$(mktemp -d /tmp/flea-startup-order-test.XXXXXX)
touch "$root/.flea-test-sandbox"
trap '[[ -n "$root" && "$root" == /tmp/flea-startup-order-test.?* && -f "$root/.flea-test-sandbox" ]] && rm -rf -- "$root"' EXIT
mkdir -p "$root/tools" "$root/crates/flea-gui" "$root/bin" "$root/traces" "$root/inherited"
cp "$repo/tools/flea-startup-order" "$root/tools/flea-startup-order"
printf 'must survive\n' > "$root/inherited/sentinel"
printf 'original list\n' > "$root/crates/flea-gui/startup-order.txt"
cat > "$root/bin/nm" <<'SH'
#!/bin/bash
printf '0 T main\n'
SH
cat > "$root/bin/perf" <<'SH'
#!/bin/bash
set -euo pipefail
case $1 in
  record)
    while [ "$#" -gt 0 ]; do
      if [ "$1" = -o ]; then printf 'trace\n' > "$2"; exit 0; fi
      shift
    done
    exit 2
    ;;
  script)
    fixture_count=${FLEA_ORDER_TEST_COUNT:-1200}
    for ((fixture_index = 0; fixture_index < fixture_count; fixture_index++)); do
      printf '0 function%s (/fixture/flea-gui) => 0 [unknown] (/other)\n' "$fixture_index"
    done
    ;;
  *) exit 2 ;;
esac
SH
touch "$root/bin/flea" "$root/bin/flea-gui"
chmod +x "$root/bin/"* "$root/tools/flea-startup-order"
export TMPDIR="$root/traces" tmp="$root/inherited" PATH="$root/bin:$PATH"
"$root/tools/flea-startup-order" record "$root/bin" /fixture/files
test -f "$root/inherited/sentinel" || { echo 'inherited tmp was deleted' >&2; exit 1; }
test -z "$(find "$root/traces" -mindepth 1 -maxdepth 1 -print)" || { echo 'trace leaked after success' >&2; exit 1; }
cp "$root/crates/flea-gui/startup-order.txt" "$root/expected-list"
if FLEA_ORDER_TEST_COUNT=2 "$root/tools/flea-startup-order" record "$root/bin" /fixture/files > "$root/refused.log" 2>&1; then
  echo 'a short trace was accepted' >&2
  exit 1
fi
cmp "$root/expected-list" "$root/crates/flea-gui/startup-order.txt"
test -f "$root/inherited/sentinel"
test -z "$(find "$root/traces" -mindepth 1 -maxdepth 1 -print)" || { echo 'trace leaked after refusal' >&2; exit 1; }
echo 'PASS startup order cleanup: inherited directory preserved, own traces removed, short trace refused'
