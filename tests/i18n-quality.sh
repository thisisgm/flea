#!/usr/bin/env bash
# Discovers every i18n contract test, including independently owned unit modules.
set -euo pipefail
cd "$(dirname "$0")/.."
python3 -B - <<'PY'
import sys
import unittest

suite = unittest.defaultTestLoader.discover("tests/i18n", pattern="test_*.py")
count = suite.countTestCases()
if count == 0:
    sys.exit("i18n-quality: no tests discovered")
result = unittest.TextTestRunner(verbosity=2).run(suite)
if not result.wasSuccessful():
    sys.exit(1)
print(f"PASS i18n-quality: {count} tests")
PY
