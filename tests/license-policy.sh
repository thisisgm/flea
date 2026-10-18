#!/bin/bash
# A matching exception passes, a bumped version is rejected, and the product workspace passes deny.toml.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1
mkdir -p .superpowers/g10l || exit 1
root=$(mktemp -d "$PWD/.superpowers/g10l/licenses.XXXXXX") || exit 1
touch "$root/.flea-test-sandbox" || exit 1
trap 'case "$root" in /*/.superpowers/g10l/licenses.?*) [ -n "$root" ] && [ -f "$root/.flea-test-sandbox" ] && rm -rf -- "$root" ;; esac' EXIT
cp deny.toml "$root/deny.toml" || exit 1
mkdir "$root/src" || exit 1
printf '%s\n' 'pub fn license_fixture() {}' > "$root/src/lib.rs"
python3 - "$root" <<'PY'
from pathlib import Path
import sys
import tomllib
root = Path(sys.argv[1])
policy = tomllib.loads((root / "deny.toml").read_text())["licenses"]
restricted = {"MPL-2.0", "ISC", "CC0-1.0", "Unicode-3.0"}
assert not restricted.intersection(policy["allow"])
seen = set()
for exception in policy["exceptions"]:
    assert exception["version"].startswith("=")
    key = exception["name"], exception["version"]
    assert key not in seen
    seen.add(key)
assert ("libloading", "=0.8.9") in seen
(root / "Cargo.toml").write_text('[package]\nname="libloading"\nversion="0.8.9"\nedition="2021"\nlicense="ISC"\n[workspace]\n')
PY
[ "$?" -eq 0 ] || exit 1
cargo generate-lockfile --manifest-path "$root/Cargo.toml" > "$root/lock.log" 2>&1 || { cat "$root/lock.log"; exit 1; }
(cd "$root" && cargo deny --locked --workspace check licenses) > "$root/control.log" 2>&1
rc=$?
cat "$root/control.log"
printf 'LICENSE control rc=%s\n' "$rc"
[ "$rc" -eq 0 ] || exit 1
sed -i 's/version="0.8.9"/version="0.8.10"/' "$root/Cargo.toml"
cargo generate-lockfile --manifest-path "$root/Cargo.toml" > "$root/lock.log" 2>&1 || { cat "$root/lock.log"; exit 1; }
(cd "$root" && cargo deny --locked --workspace check licenses) > "$root/mutant.log" 2>&1
rc=$?
cat "$root/mutant.log"
printf 'LICENSE version-bump mutant rc=%s\n' "$rc"
if [ "$rc" -eq 0 ] || ! grep -q 'error\[rejected\]' "$root/mutant.log" || ! grep -q 'libloading v0.8.10' "$root/mutant.log"; then
  echo 'FAIL version bump did not fail for its uncovered license'
  exit 1
fi
echo 'ok exact-version license exception rejects the next version'
cargo deny --locked --offline --workspace check licenses > "$root/product.log" 2>&1
rc=$?
cat "$root/product.log"
printf 'LICENSE product workspace rc=%s\n' "$rc"
[ "$rc" -eq 0 ] || { echo 'FAIL the product workspace has a license deny.toml does not allow'; exit 1; }
echo 'ok product workspace licenses pass deny.toml'
