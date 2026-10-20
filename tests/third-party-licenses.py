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
