#!/bin/bash
# Exercises the proposed CI runner only in a private, marked fixture tree.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 2
mkdir -p "$PWD/.superpowers/m9-prove-02" || exit 2
root=$(mktemp -d "$PWD/.superpowers/m9-prove-02/fixture.XXXXXXXX") || exit 2
touch "$root/.flea-test-sandbox" || exit 2
python3 -I tests/prove_runner_contracts.py "$root/contracts" || exit $?
python3 -B tests/prove_ci_runner.py --fixtures "$root"
exit $?
