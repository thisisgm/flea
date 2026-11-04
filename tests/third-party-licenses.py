#!/usr/bin/env python3
"""Regression controls for the release notice inventory and package metadata."""

import importlib.machinery
import importlib.util
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tarfile
import tempfile
import tomllib
import unittest
from contextlib import contextmanager
from contextlib import redirect_stdout
from io import StringIO
from unittest.mock import patch

ROOT = Path(__file__).resolve().parent.parent
loader = importlib.machinery.SourceFileLoader("third_party", str(ROOT / "tools/flea-third-party-licenses"))
spec = importlib.util.spec_from_loader(loader.name, loader)
licenses = importlib.util.module_from_spec(spec)
loader.exec_module(licenses)


@contextmanager
def owned_fixture(scratch):
    scratch.mkdir(parents=True, exist_ok=True)
    root = Path(tempfile.mkdtemp(prefix="license-fixture.", dir=scratch))
    (root / ".flea-test-sandbox").touch()
    try:
        yield root
    finally:
        assert root.is_absolute() and root != scratch and root.is_relative_to(scratch)
        assert root.joinpath(".flea-test-sandbox").is_file()
        shutil.rmtree(root)


class InventoryTests(unittest.TestCase):
    def test_shipped_gui_packages_declare_bsd_and_carry_notices(self):
        notice = ROOT / licenses.OUTPUT
        self.assertTrue(notice.is_file(), "flea-gui ships without third-party notices")
        text = notice.read_text()
        expected = set(next(line for line in text.splitlines() if line.startswith("Package licenses: ")).split(": ", 1)[1].split())
        self.assertTrue({"BSD-2-Clause", "BSD-3-Clause", "Zlib", "MPL-2.0", "ISC", "CC0-1.0", "Unicode-3.0"} <= expected)
        licenses.package_licenses(ROOT, expected)
        for crate in ("rav1e", "av1-grain", "v_frame", "arrayref", "option-ext", "libloading", "hexf-parse", "tiny-keccak"):
            self.assertRegex(text, rf"(?m)^{crate} [0-9]")

    def test_expression_precedence_and_existing_alternatives(self):
        policy = {"allow": ["MIT", "Apache-2.0", "BSD-3-Clause", "Unicode-3.0"]}
        cases = {"MIT/Apache-2.0": {"MIT"}, "0BSD OR MIT OR Apache-2.0": {"MIT"},
                 "Apache-2.0 OR GPL-2.0-only": {"Apache-2.0"},
                 "(Apache-2.0 OR MIT) AND BSD-3-Clause": {"MIT", "BSD-3-Clause"},
                 "(MIT OR Apache-2.0) AND Unicode-3.0": {"MIT", "Unicode-3.0"},
                 "Apache-2.0 WITH LLVM-exception OR MIT": {"MIT"}}
        for expression, expected in cases.items():
            with self.subTest(expression=expression):
                self.assertEqual(licenses.chosen_license({"name": "test", "version": "1", "license": expression}, policy), expected)
        for expression in ("MIT AND", "MIT OR (Apache-2.0", "MIT ? Apache-2.0", "MIT WITH OR"):
            with self.assertRaises(ValueError):
                licenses.alternatives(expression)

    def test_restricted_exceptions_remain_version_pinned(self):
        policy = {"allow": ["MIT"], "exceptions": [{"name": "libloading", "version": "=0.8.9", "allow": ["ISC"]}]}
        package = {"name": "libloading", "version": "0.8.9", "license": "ISC"}
        self.assertEqual(licenses.chosen_license(package, policy), {"ISC"})
        with self.assertRaises(ValueError):
            licenses.chosen_license(dict(package, version="0.8.10"), policy)

    def test_clarified_file_only_license_keeps_its_complete_notice_and_exact_version(self):
        scratch = ROOT / ".superpowers/third-party-licenses"
        with owned_fixture(scratch) as root:
            (root / "Cargo.toml").write_text('[package]\nname="flea"\nversion="1"\nlicense="MIT"\n')
            (root / "Cargo.lock").write_text('version = 4\n')
            (root / "deny.toml").write_text('[licenses]\nallow=["MIT"]\n[[licenses.clarify]]\nname="omi"\nversion="=1.0.0"\nexpression="MIT"\nlicense-files=[{path="LICENSE",hash=0x9a5bab21}]\n')
            (root / "LICENSE").write_text("workspace license\n")
            (root / "ui/vendor/LICENSES").mkdir(parents=True)
            crate = root / "omi"
            crate.mkdir()
            complete = (ROOT / "tests/fixtures/license-fingerprint/omi-1.0.0-LICENSE").read_text()
            (crate / "LICENSE").write_text(complete)
            packages = [{"id": "flea", "name": "flea", "version": "1", "license": "MIT",
                         "source": None, "manifest_path": str(root / "Cargo.toml"), "targets": [{"kind": ["bin"]}]},
                        {"id": "omi", "name": "omi", "version": "1.0.0", "license": None,
                         "license_file": "LICENSE", "source": "registry+fixture",
                         "manifest_path": str(crate / "Cargo.toml"), "targets": [{"kind": ["lib"]}]}]
            graph = {"packages": packages, "workspace_members": ["flea"], "workspace_root": str(root),
                     "resolve": {"nodes": [{"id": "flea", "deps": [{"pkg": "omi", "dep_kinds": [{"kind": None}]}]},
                                            {"id": "omi", "deps": []}]}}
            text, selected = licenses.render(root, {target: graph for target in licenses.TARGETS})
            self.assertEqual(selected, {"MIT"})
            self.assertIn(complete, text)
            self.assertEqual(text.count(complete), 1)
            self.assertIn("Expression source: deny.toml version-pinned clarification", text)
            (crate / "LICENSE").write_text("Changed license terms.\n")
            with self.assertRaisesRegex(ValueError, "license clarification fingerprint mismatch for LICENSE"):
                licenses.render(root, {target: graph for target in licenses.TARGETS})
            (crate / "LICENSE").write_text(complete)
            (crate / "LICENSE").rename(crate / "COPYING")
            packages[1].update(license="GPL-3.0-only", license_file="COPYING")
            with self.assertRaisesRegex(ValueError, "cannot read license-file LICENSE"):
                licenses.render(root, {target: graph for target in licenses.TARGETS})
            (crate / "COPYING").rename(crate / "LICENSE")
            packages[1].update(license=None, license_file="LICENSE")
            packages[1]["version"] = "1.0.1"
            with self.assertRaisesRegex(ValueError, "has no SPDX license"):
                licenses.render(root, {target: graph for target in licenses.TARGETS})

    def clarification_fixture(self, root):
        crate = root / "omi"
        crate.mkdir()
        notice = crate / "LICENSE"
        notice.write_bytes((ROOT / "tests/fixtures/license-fingerprint/omi-1.0.0-LICENSE").read_bytes())
        package = {"name": "omi", "version": "1.0.0", "license": None,
                   "manifest_path": str(crate / "Cargo.toml")}
        policy = tomllib.loads((ROOT / "deny.toml").read_text())["licenses"]
        return package, policy, notice

    def test_clarification_rejects_changed_notice_through_real_api(self):
        with owned_fixture(ROOT / ".superpowers/third-party-licenses") as root:
            package, policy, notice = self.clarification_fixture(root)
            notice.write_text("GPL-3.0-only\nChanged terms at the same version.\n")
            for declared in (None, "GPL-3.0-only", "MIT"):
                package["license"] = declared
                for api in (licenses.license_expression, licenses.chosen_license):
                    with self.subTest(declared=declared, api=api.__name__):
                        with self.assertRaisesRegex(ValueError, "license clarification fingerprint mismatch for LICENSE"):
                            api(package, policy)

    def test_clarification_requires_the_configured_file_not_another_notice(self):
        with owned_fixture(ROOT / ".superpowers/third-party-licenses") as root:
            package, policy, notice = self.clarification_fixture(root)
            notice.rename(notice.with_name("COPYING"))
            package.update(license="GPL-3.0-only", license_file="COPYING")
            with self.assertRaisesRegex(ValueError, "cannot read license-file LICENSE"):
                licenses.chosen_license(package, policy)
            notice.write_text("Different terms.\n")
            with self.assertRaisesRegex(ValueError, "fingerprint mismatch for LICENSE"):
                licenses.chosen_license(package, policy)

    def test_clarification_rejects_missing_guards_and_paths_outside_the_crate(self):
        with owned_fixture(ROOT / ".superpowers/third-party-licenses") as root:
            package, policy, notice = self.clarification_fixture(root)
            clarification = policy["clarify"][0]
            configured = clarification["license-files"][0]
            for files in ([], [{"path": "LICENSE"}], [dict(configured, hash=-1)],
                          [dict(configured, hash=True)], [dict(configured, hash=2**32)],
                          [dict(configured, path=str(notice))], [dict(configured, path="")]):
                clarification["license-files"] = files
                with self.subTest(files=files), self.assertRaises(ValueError):
                    licenses.chosen_license(package, policy)
            clarification["license-files"] = [configured]
            with self.assertRaisesRegex(ValueError, "requires license-files and a manifest_path"):
                licenses.chosen_license({key: value for key, value in package.items() if key != "manifest_path"}, policy)
            outside = root / "outside-LICENSE"
            notice.rename(outside)
            notice.symlink_to(outside)
            with self.assertRaisesRegex(ValueError, "must stay inside the crate"):
                licenses.chosen_license(package, policy)
            clarification["license-files"] = [dict(configured, path="../outside-LICENSE")]
            with self.assertRaisesRegex(ValueError, "must stay inside the crate"):
                licenses.chosen_license(package, policy)

    def test_clarification_checks_every_configured_file_and_preserves_exact_version(self):
        with owned_fixture(ROOT / ".superpowers/third-party-licenses") as root:
            package, policy, notice = self.clarification_fixture(root)
            self.assertEqual(licenses.license_expression(package, policy), ("MIT", True))
            self.assertEqual(licenses.chosen_license(package, policy), {"MIT"})
            for field, value in (("name", "other"), ("version", "1.0.1")):
                with self.subTest(field=field), self.assertRaisesRegex(ValueError, "has no SPDX license"):
                    licenses.chosen_license(dict(package, **{field: value}), policy)
            with self.assertRaisesRegex(ValueError, "does not allow GPL-3.0-only"):
                licenses.chosen_license(dict(package, version="1.0.1", license="GPL-3.0-only"), policy)
            policy["clarify"][0]["license-files"].append({"path": "NOTICE", "hash": 0x9a5bab21})
            with self.assertRaisesRegex(ValueError, "cannot read license-file NOTICE"):
                licenses.chosen_license(package, policy)
            notice.with_name("NOTICE").write_bytes(notice.read_bytes())
            self.assertEqual(licenses.chosen_license(package, policy), {"MIT"})

    def test_cargo_deny_hash_matches_c_reference_vectors(self):
        # twox-hash 2.1.2 src/xxhash32.rs names these as C-implementation vectors.
        for raw, expected in ((b"", 0x02cc5d05), (bytes([42]), 0xe0fe705f),
                              (b"Hello, world!\0", 0x9e5e7e93), (bytes(range(100)), 0x7f89ba44)):
            with self.subTest(raw=raw):
                self.assertEqual(licenses.xxhash32(raw), expected)

    def test_cargo_deny_normalization_preserves_pinned_omi_and_notice_content(self):
        with owned_fixture(ROOT / ".superpowers/third-party-licenses") as root:
            package, policy, notice = self.clarification_fixture(root)
            original = notice.read_bytes()
            for content in (original, original.replace(b"\n", b"\r\n"), original.replace(b"\n", b"\r\r\n")):
                notice.write_bytes(content)
                with self.subTest(content=content[:30]):
                    self.assertEqual(licenses.license_file_hash(notice), 0x9a5bab21)
                    self.assertEqual(licenses.chosen_license(package, policy), {"MIT"})
            def fingerprint(content):
                notice.write_bytes(content)
                return licenses.license_file_hash(notice)
            self.assertEqual(fingerprint(b"last line"), fingerprint(b"last line\n"))
            self.assertEqual(fingerprint(b"last line\r\r"), fingerprint(b"last line\n"))
            self.assertNotEqual(fingerprint(b"last line \n"), fingerprint(b"last line\n"))
            self.assertNotEqual(fingerprint(b"one\rtwo"), fingerprint(b"one\ntwo"))
            self.assertNotEqual(fingerprint(b""), fingerprint(b"\n"))
            notice.write_bytes(original + b"\xff")
            with self.assertRaisesRegex(ValueError, "cannot read license-file LICENSE"):
                licenses.chosen_license(package, policy)

    def test_binary_and_source_tarball_artifacts_carry_the_inventory(self):
        scratch = ROOT / ".superpowers/third-party-licenses"
        with owned_fixture(scratch) as box:
            binary = box / "flea"
            subprocess.run(["cc", "-x", "c", "-o", str(binary), "-"],
                           input=b'#include <stdio.h>\nint main(void) { puts("0.4.0"); return 0; }\n', check=True)
            shutil.copy2(binary, box / "flea-gui")
            environment = dict(os.environ, TMPDIR=str(box), SOURCE_DATE_EPOCH="1")
            output = box / "binary-output"
            subprocess.run([str(ROOT / "packaging/flea-bin-tarball"), str(binary), "0.4.0", platform.machine(), str(output)],
                           env=environment, check=True, stdout=subprocess.PIPE)
            with tarfile.open(next(output.glob("*.tar.gz"))) as archive:
                name = f"flea-0.4.0-linux-{platform.machine()}/{licenses.OUTPUT}"
                self.assertEqual(archive.extractfile(name).read(), (ROOT / licenses.OUTPUT).read_bytes())
            checkout = box / "source"
            (checkout / "packaging").mkdir(parents=True)
            shutil.copy2(ROOT / "packaging/flea-source-tarball", checkout / "packaging/flea-source-tarball")
            shutil.copy2(ROOT / licenses.OUTPUT, checkout / licenses.OUTPUT)
            shim = box / "shim"
            shim.mkdir()
            git = shim / "git"
            git.write_text('''#!/usr/bin/env python3
import io, os, pathlib, sys, tarfile
args = sys.argv[1:]
if args[0] == "-C":
    root = pathlib.Path(args[1])
    args = args[2:]
if args[0] == "rev-parse":
    sys.exit(0)
assert args[0] == "archive", args
prefix = next(arg.split("=", 1)[1] for arg in args if arg.startswith("--prefix="))
with tarfile.open(fileobj=sys.stdout.buffer, mode="w|") as archive:
    directory = tarfile.TarInfo(prefix)
    directory.type, directory.mtime = tarfile.DIRTYPE, 1
    archive.addfile(directory)
    for path in sorted(root.glob("*.txt")):
        data = path.read_bytes()
        info = tarfile.TarInfo(prefix + path.name)
        info.size, info.mtime = len(data), 1
        archive.addfile(info, io.BytesIO(data))
''')
            git.chmod(0o755)
            environment["PATH"] = str(shim) + os.pathsep + os.environ["PATH"]
            output = box / "source-output"
            command = [str(checkout / "packaging/flea-source-tarball"), "v0.4.0", str(output)]
            subprocess.run(command, env=environment, check=True, stdout=subprocess.PIPE)
            with tarfile.open(next(output.glob("*.tar.gz"))) as archive:
                self.assertEqual(archive.extractfile("flea-0.4.0/" + licenses.OUTPUT).read(), (ROOT / licenses.OUTPUT).read_bytes())
            victim = checkout / licenses.OUTPUT
            assert victim.is_absolute() and victim.is_relative_to(box) and (box / ".flea-test-sandbox").is_file()
            victim.unlink()
            failed = subprocess.run(command, env=environment, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            self.assertNotEqual(failed.returncode, 0)
            self.assertIn(b"no third-party license inventory", failed.stderr)

    def test_target_union_notices_git_ancestors_and_lock_drift(self):
        scratch = ROOT / ".superpowers/third-party-licenses"
        scratch.mkdir(parents=True, exist_ok=True)
        with owned_fixture(scratch) as root:
            (root / "Cargo.toml").write_text('[package]\nname="flea"\nversion="1"\nlicense="MIT"\n')
            (root / "Cargo.lock").write_text('version = 4\n')
            (root / "deny.toml").write_text('[licenses]\nallow=["MIT", "Apache-2.0", "BSD-2-Clause"]\n')
            (root / "LICENSE").write_text("shared license text\n")
            (root / "ui/vendor/LICENSES").mkdir(parents=True)
            (root / "ui/vendor/LICENSES/Apache-2.0-fixture").write_text("vendor license\n")
            checkout = root / "git"
            (checkout / ".git").mkdir(parents=True)
            (checkout / "LICENSE").write_text("git root license\n")
            (checkout / "NOTICE").write_text("git root notice\n")
            packages = []
            for name, expression, source in (("flea", "MIT", None), ("normal", "BSD-2-Clause", "registry+fixture"),
                                             ("arm", "MIT", "registry+fixture"), ("macro", "MIT", "registry+fixture"),
                                             ("missing", "MIT", "registry+fixture"), ("build", "MIT", "registry+fixture"),
                                             ("dev", "MIT", "registry+fixture"), ("fork", "Apache-2.0", "git+fixture#123")):
                directory = checkout / "crates/fork" if name == "fork" else root / name
                directory.mkdir(parents=True)
                if name not in {"fork", "missing"}:
                    (directory / "LICENSE").write_text("shared license text\n")
                packages.append({"id": name, "name": name, "version": "1", "license": expression,
                                 "source": source, "manifest_path": str(directory / "Cargo.toml"),
                                 "targets": [{"kind": ["bin"] if name == "flea" else ["proc-macro"] if name == "macro" else ["lib"]}]})
            def metadata(arm):
                deps = [{"pkg": name, "dep_kinds": [{"kind": kind}]} for name, kind in
                        (("normal", None), ("macro", None), ("missing", None), ("fork", None), ("build", "build"), ("dev", "dev"))]
                if arm:
                    deps.append({"pkg": "arm", "dep_kinds": [{"kind": None}]})
                return {"packages": packages, "workspace_members": ["flea"], "workspace_root": str(root),
                        "resolve": {"nodes": [{"id": p["id"], "deps": deps if p["id"] == "flea" else []} for p in packages]}}
            (root / "normal/LICENSES/components").mkdir(parents=True)
            (root / "normal/LICENSES/components/BSD-2-Clause.txt").write_text("nested component license\n")
            graphs = {licenses.TARGETS[0]: metadata(False), licenses.TARGETS[1]: metadata(True)}
            text, selected = licenses.render(root, graphs)
            self.assertEqual(selected, {"MIT", "Apache-2.0", "BSD-2-Clause"})
            self.assertEqual(text.count("shared license text"), 1)
            self.assertIn("git root license", text)
            self.assertIn("git root notice", text)
            self.assertIn("nested component license", text)
            self.assertIn("No license or notice file supplied", text)
            self.assertIn("arm 1", text)
            self.assertIn("macro 1", text)
            self.assertNotIn("build 1", text)
            self.assertNotIn("dev 1", text)
            self.assertEqual(text, licenses.render(root, dict(reversed(list(graphs.items()))))[0])
            for filename in licenses.PACKAGES:
                path = root / filename
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("license=('MIT' 'Apache-2.0' 'BSD-2-Clause')\n")
            (root / licenses.OUTPUT).write_text(text)
            def cargo_result(argv, **_kwargs):
                import json
                return subprocess.CompletedProcess(argv, 0, json.dumps(graphs[argv[-1]]).encode())
            with patch.object(licenses, "__file__", str(root / "tools/flea-third-party-licenses")), \
                    patch("sys.argv", ["flea-third-party-licenses", "--check"]), \
                    patch.object(licenses.subprocess, "run", side_effect=cargo_result), redirect_stdout(StringIO()):
                licenses.main()
                (root / "Cargo.lock").write_text('version = 4\n[[package]]\nname="new-uncovered-crate"\nversion="1"\n')
                with self.assertRaisesRegex(ValueError, "is stale"):
                    licenses.main()
            self.assertNotEqual(text, licenses.render(root, graphs)[0], "a new locked crate must invalidate the committed inventory")
            for filename in licenses.PACKAGES:
                path = root / filename
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("license=('MIT' 'Apache-2.0')\n")
            with self.assertRaisesRegex(ValueError, "BSD-2-Clause"):
                licenses.package_licenses(root, selected)


if __name__ == "__main__":
    unittest.main()
