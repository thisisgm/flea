#!/usr/bin/env python3
"""Compile eight source mutants and require the real CLI checks to reject each."""
import os
from pathlib import Path
import shutil
import subprocess

import sandbox

repo = Path(__file__).resolve().parents[2]
root = sandbox.make("cli-mutants-")
shutil.copytree(repo / "src", root / "src")
for name in ("ui", "shelf", "keys.toml", "i18n", "docs"):
    (root / name).symlink_to(repo / name, target_is_directory=(repo / name).is_dir())
manifest = (repo / "Cargo.toml").read_text()
version = next(line for line in manifest.splitlines() if line.startswith("version ="))
# The scratch crate takes the root crate's own dependencies at their locked versions, so it builds offline.
# Sample input: '\n[dependencies]\nfoldhash = "0.2.0"\n\n[features]' answers 'foldhash = "0.2.0"\n'.
_, header, rest = manifest.partition("\n[dependencies]\n")
assert header, f"no [dependencies] table in {repo / 'Cargo.toml'}"
dependencies = rest.split("\n[", 1)[0]
(root / "Cargo.toml").write_text('[package]\nname = "flea"\n' + version + '\nedition = "2021"\n[workspace]\n[dependencies]\n' + dependencies + '\n[features]\nflash-test-seam = []\n')
shutil.copyfile(repo / "Cargo.lock", root / "Cargo.lock")
mutants = [
    ("unknown", "usage.rs", '    exit(2)\n}', '    exit(0)\n}'),
    ("backend", "main.rs", 'if args.iter().any(|a| a == "--backend") {', 'if false {'),
    ("gui-desktop", "launchargs.rs", 'let mut options = Self::default();', 'let mut options = Self { want_tui: true, ..Self::default() };'),
    ("tui-pipe", "main.rs", 'if !interactive {', 'if false {'),
    ("select-path", "main.rs", 'Some(target.to_string_lossy().into_owned())', 'Some(String::new())'),
    ("pick", "gui/launch.rs", 'cmd.arg("--pick").arg(reply);', 'cmd.arg("--pick").arg("");'),
    ("open-folder", "main.rs", 'exit(open::open_many(&args[2..]));', 'exit(0);'),
    ("ui-state", "uistatecli.rs", 'print!("{}", jsondoc::render(&state));', 'print!("wrong state");'),
]
env = os.environ.copy()
env["CARGO_TARGET_DIR"] = str(root / "target")
# A run that checked no mutant must fail, never print a vacuous pass.
assert mutants, "no mutants defined: refusing a zero-check run"
# The unmutated scratch crate must pass every row, so a rejection can only come from its mutation.
build = subprocess.run(["cargo", "build", "--offline", "--manifest-path", str(root / "Cargo.toml"), "--bin", "flea"], env=env, capture_output=True, timeout=300)
assert build.returncode == 0, build.stderr.decode(errors="replace")
baseline = subprocess.run(["python3", str(repo / "tests/compat/cli_contract.py"), str(root / "target/debug/flea")], capture_output=True, timeout=120)
assert baseline.returncode == 0, ("unmutated scratch crate fails the contract", baseline.stdout, baseline.stderr)
for row, filename, before, after in mutants:
    source = root / "src" / filename
    original = source.read_text()
    assert original.count(before) == 1, f"mutation anchor changed: {row}"
    source.write_text(original.replace(before, after, 1))
    try:
        build = subprocess.run(["cargo", "build", "--offline", "--manifest-path", str(root / "Cargo.toml"), "--bin", "flea"], env=env, capture_output=True, timeout=120)
        assert build.returncode == 0, build.stderr.decode(errors="replace")
        check = subprocess.run(["python3", str(repo / "tests/compat/cli_contract.py"), str(root / "target/debug/flea"), "--case", row], capture_output=True, timeout=15)
        assert check.returncode != 0 and (row + ": expected").encode() in check.stderr, (row, check.stdout, check.stderr)
        print(f"mutation {row}: contract rejected source change")
    finally:
        source.write_text(original)
# The count is printed, so an evidence rule can key on the inventory size.
print(f"cli-contract-mutations: all {len(mutants)} source mutations rejected")
