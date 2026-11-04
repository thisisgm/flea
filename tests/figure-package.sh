#!/usr/bin/env bash
# Execute each package() into an owned scratch tree and resolve all installed relative ES imports.
set -euo pipefail
cd "$(dirname "$0")/.."
repo=$PWD
. tools/flea-sandbox-guard
sandbox_require "$FIXTURE_ROOT/flea-figure-package"
mkdir -p "$SANDBOX_ROOT"
box=$(mktemp -d "$SANDBOX_ROOT/flea-figure-package.XXXXXX")
printf 'figure package regression\n' > "$box/$SANDBOX_MARKER"
trap 'sandbox_remove "$box"' EXIT
# flea-bin's tree is the real binary tarball, staged around a stub binary that prints the version, as tests/third-party-licenses.py does.
arch=$(uname -m)
version=$(. packaging/flea-bin/PKGBUILD && printf '%s' "$pkgver")
mkdir -p "$box/stub"
printf '#include <stdio.h>\nint main(void) { puts("%s"); return 0; }\n' "$version" | cc -x c -o "$box/stub/flea" -
cp "$box/stub/flea" "$box/stub/flea-gui"
TMPDIR="$box" SOURCE_DATE_EPOCH=1 packaging/flea-bin-tarball "$box/stub/flea" "$version" "$arch" "$box/stub" >/dev/null
for name in root flea-bin; do
    (
        if [ "$name" = root ]; then
            . "$repo/PKGBUILD"
            startdir="$repo"
        else
            . "$repo/packaging/$name/PKGBUILD"
        fi
        printf '%s\n' "${depends[@]}" > "$box/$name.depends"
        printf '%s\n' "${license[@]}" > "$box/$name.licenses"
        srcdir="$box/$name/src"
        pkgdir="$box/$name/pkg"
        CARCH=$arch
        mkdir -p "$srcdir/target/release"
        printf 'package binary fixture\n' > "$srcdir/target/release/flea"
        printf 'GUI package binary fixture\n' > "$srcdir/target/release/flea-gui"
        case "$name" in
            root) tree="$srcdir/$pkgname-$pkgver" ;;
            flea-bin) tree="$srcdir/$_pkgname-$pkgver-linux-$CARCH" ;;
        esac
        mkdir -p "$tree"
        if [ "$name" = flea-bin ]; then
            tar -xzf "$box/stub/flea-v$pkgver-linux-$CARCH.tar.gz" -C "$srcdir"
        else
            for data in ui tools packaging shelf i18n LICENSE THIRD-PARTY-LICENSES.txt Makefile; do
                ln -s "$repo/$data" "$tree/$data"
            done
        fi
        cp "$srcdir/target/release/flea" "$tree/flea"
        cp "$srcdir/target/release/flea-gui" "$tree/flea-gui"
        # The root recipe's make install reads the binaries from the source tree's own target/.
        cp -r "$srcdir/target" "$tree/target"
        cd "$srcdir"
        package
    )
done
python3 - "$box" "$repo" <<'PY'
import os
import pathlib
import re
import shlex
import shutil
import subprocess
import sys

root = pathlib.Path(sys.argv[1])
checks = 0
failures = 0
# Every file git tracks under ui/ (a tree without git: every file but dotfiles and caches), so a package glob that misses one (ui/MarkdownWorker.js once) fails by name.
source_ui = pathlib.Path(sys.argv[2]) / "ui"
tracked = subprocess.run(["git", "-C", sys.argv[2], "ls-files", "-z", "--", "ui"], capture_output=True)
# Sample input: b"ui/qmldir\0ui/Pane.qml\0" gives {"qmldir", "Pane.qml"}; a tracked name no longer on disk is left out.
if tracked.returncode == 0 and tracked.stdout:
    source_files = {name.removeprefix("ui/") for name in tracked.stdout.decode().split("\0")
                    if name and (pathlib.Path(sys.argv[2]) / name).is_file() and not (pathlib.Path(sys.argv[2]) / name).is_symlink()}
else:
    source_files = {path.relative_to(source_ui).as_posix() for path in source_ui.rglob("*")
                    if path.is_file() and not path.is_symlink() and not any(part.startswith(".") or part == "__pycache__" for part in path.relative_to(source_ui).parts)}
# Every file git tracks under shelf/ (a tree without git: every file but dotfiles and caches), so a package glob that misses one fails by name.
source_shelf = pathlib.Path(sys.argv[2]) / "shelf"
tracked_shelf = subprocess.run(["git", "-C", sys.argv[2], "ls-files", "-z", "--", "shelf"], capture_output=True)
# Sample input: b"shelf/Panel.qml\0shelf/Model.js\0" gives {"Panel.qml", "Model.js"}; a tracked name no longer on disk is left out.
if tracked_shelf.returncode == 0 and tracked_shelf.stdout:
    shelf_files = {name.removeprefix("shelf/") for name in tracked_shelf.stdout.decode().split("\0")
                    if name and (pathlib.Path(sys.argv[2]) / name).is_file() and not (pathlib.Path(sys.argv[2]) / name).is_symlink()}
else:
    shelf_files = {path.relative_to(source_shelf).as_posix() for path in source_shelf.rglob("*")
                    if path.is_file() and not path.is_symlink() and not any(part.startswith(".") or part == "__pycache__" for part in path.relative_to(source_shelf).parts)}
# Every catalog git tracks as i18n/<code>/flea.json (a tree without git: every such file), the only i18n/ files the runtime reads.
source_i18n = pathlib.Path(sys.argv[2]) / "i18n"
tracked_i18n = subprocess.run(["git", "-C", sys.argv[2], "ls-files", "-z", "--", "i18n"], capture_output=True)
# Sample input: b"i18n/de/flea.json\0i18n/rtl-policy.json\0" gives {"de/flea.json"}.
if tracked_i18n.returncode == 0 and tracked_i18n.stdout:
    catalogs = {name.removeprefix("i18n/") for name in tracked_i18n.stdout.decode().split("\0")
                if re.fullmatch(r"i18n/[^/]+/flea\.json", name) and (source_i18n.parent / name).is_file()}
else:
    catalogs = {path.relative_to(source_i18n).as_posix() for path in source_i18n.glob("*/flea.json") if path.is_file()}
print(f"catalogs checked: {len(catalogs)}")
# Every file that can name a /usr/lib/flea helper: tracked sources plus the packaging registrations.
repo_root = pathlib.Path(sys.argv[2])
notices = (repo_root / "THIRD-PARTY-LICENSES.txt").read_bytes()
# Sample input: "Package licenses: Apache-2.0 MIT Zlib" splits at ": " and then on spaces into {"Apache-2.0", "MIT", "Zlib"}.
license_line = next(line for line in notices.decode().splitlines() if line.startswith("Package licenses: "))
expected_licenses = set(license_line.split(": ", 1)[1].split())
tracked_all = subprocess.run(["git", "-C", sys.argv[2], "ls-files", "-z", "--", "src", "ui", "shelf", "packaging"], capture_output=True)
# Sample input: b"src/main.rs\0ui/App.qml\0" gives ["src/main.rs", "ui/App.qml"].
if tracked_all.returncode == 0 and tracked_all.stdout:
    candidates = [name for name in tracked_all.stdout.decode().split("\0") if name]
else:
    candidates = [path.relative_to(repo_root).as_posix() for base in ("src", "ui", "shelf") for path in (repo_root / base).rglob("*")
                    if path.is_file() and not path.is_symlink() and not any(part.startswith(".") or part == "__pycache__" for part in path.relative_to(repo_root / base).parts)]
    candidates += [path.relative_to(repo_root).as_posix() for path in (repo_root / "packaging").glob("*")
                    if path.is_file() and not path.is_symlink()]
# Sample input: packaging/flea-bin/PKGBUILD is out, packaging/flea.portal is in.
def names_helper(name):
    if name.startswith("src/") and name.endswith(".rs") and not name.endswith("_tests.rs"):
        return True
    if name.startswith("ui/") or name.startswith("shelf/"):
        return True
    return re.fullmatch(r"packaging/[^/]+\.(service|portal|desktop|hook)", name) is not None
# Sample input: const HELPER_PATH: &str = "/usr/lib/flea/flea-udisks"; gives flea-udisks.
helper_ref = re.compile(r"/usr/lib/flea/([A-Za-z0-9._-]+)")
helper_first = {}
for candidate in sorted(filter(names_helper, candidates)):
    path = repo_root / candidate
    if not path.is_file() or path.is_symlink():
        continue
    try:
        text = path.read_text(encoding="utf-8", errors="ignore")
    except OSError:
        continue
    for lineno, line in enumerate(text.splitlines(), 1):
        # A prose mention inside a comment names no helper, so comment lines hold no names.
        if line.lstrip().startswith(("//", "#")):
            continue
        for found in helper_ref.findall(line):
            # A sentence-final path carries its mark, so a trailing mark names no helper.
            name = found.rstrip(".,;)")
            if name:
                helper_first.setdefault(name, f"{candidate}:{lineno}")
helper_names = set(helper_first)
checks += 1
if not helper_names:
    failures += 1
    print("FAIL helpers: no /usr/lib/flea/ name was found, so the coverage check would hold nothing")
print(f"helpers checked: {', '.join(sorted(helper_names))}")
# Sample input: a 755 regular file gives True; a missing path or a 644 file gives False.
def helper_installed(base, name):
    target = base / name
    try:
        return target.is_file() and not target.is_symlink() and bool(target.stat().st_mode & 0o111)
    except OSError:
        return False
# Sample inputs: import { renderFigure } from "../js/FigureWorker.mjs"; await import("./math.mjs").
imports = re.compile(r'\b(?:from\s*|import\s*\(\s*|import\s*)["\'](\.[^"\']+)["\']')
for package in ("root", "flea-bin"):
    checks += 2
    actual_licenses = (root / f"{package}.licenses").read_text().splitlines()
    if len(actual_licenses) != len(expected_licenses) or set(actual_licenses) != expected_licenses:
        failures += 1
        print(f"FAIL {package}: license array does not cover the locked binary inventory")
    package_name = "flea" if package == "root" else package
    installed_notice = root / package / "pkg/usr/share/licenses" / package_name / "THIRD-PARTY-LICENSES.txt"
    if not installed_notice.is_file() or installed_notice.read_bytes() != notices:
        failures += 1
        print(f"FAIL {package}: third-party license inventory is missing or changed")
    for binary, expected in (("flea", b"package binary fixture\n"), ("flea-gui", b"GUI package binary fixture\n")):
        checks += 1
        installed = root / package / "pkg/usr/bin" / binary
        if not helper_installed(installed.parent, binary) or installed.read_bytes() != expected:
            failures += 1
            print(f"FAIL {package}: /usr/bin/{binary} is not the executable binary fixture")
    checks += 1
    dependencies = (root / f"{package}.depends").read_text().splitlines()
    if dependencies.count("quickjs-ng") != 1:
        failures += 1
        print(f"FAIL {package}: depends must declare quickjs-ng exactly once, matching the other PKGBUILDs")
    ui = root / package / "pkg/usr/share/flea/ui"
    modules = sorted(ui.glob("vendor/*.mjs")) + sorted(ui.glob("js/*.mjs"))
    required = ["js/FigureWorker.mjs", "vendor/figure-helper.mjs", "vendor/figure-bytecode.mjs", "vendor/figure-compile.mjs", "vendor/math.mjs", "vendor/mermaid.mjs"]
    for relative in required:
        checks += 1
        if not (ui / relative).is_file():
            failures += 1
            print(f"FAIL {package}: missing {relative}")
    shipped = {path.relative_to(ui).as_posix() for path in ui.rglob("*") if path.is_file()}
    checks += 1
    if not source_files or not shipped:
        failures += 1
        print(f"FAIL {package}: a ui/ file set is empty, so the coverage check would hold nothing")
    for relative in sorted(source_files - shipped):
        checks += 1
        failures += 1
        print(f"FAIL {package}: ui/{relative} is not installed")
    shelf = root / package / "pkg/usr/share/flea/shelf"
    shipped_shelf = {path.relative_to(shelf).as_posix() for path in shelf.rglob("*") if path.is_file()}
    checks += 1
    if not shelf_files or not shipped_shelf:
        failures += 1
        print(f"FAIL {package}: a shelf/ file set is empty, so the coverage check would hold nothing")
    for relative in sorted(shelf_files - shipped_shelf):
        checks += 1
        failures += 1
        print(f"FAIL {package}: shelf/{relative} is not installed")
    i18n = root / package / "pkg/usr/share/flea/i18n"
    checks += 1
    if not catalogs:
        failures += 1
        print(f"FAIL {package}: no i18n/<code>/flea.json was found, so the catalog check would hold nothing")
    for relative in sorted(catalogs):
        checks += 1
        installed = i18n / relative
        if not installed.is_file() or installed.read_bytes() != (source_i18n / relative).read_bytes():
            failures += 1
            print(f"FAIL {package}: /usr/share/flea/i18n/{relative} is missing or differs from i18n/{relative}")
    # Only <code>/flea.json is read at run time; README.md and the policy files are build and test data.
    for path in sorted(i18n.rglob("*")):
        relative = path.relative_to(i18n).as_posix()
        if not path.is_dir() and not re.fullmatch(r"[^/]+/flea\.json", relative):
            checks += 1
            failures += 1
            print(f"FAIL {package}: /usr/share/flea/i18n/{relative} is not a catalog the runtime reads")
    helpers = root / package / "pkg/usr/lib/flea"
    for name in sorted(helper_names):
        checks += 1
        if not helper_installed(helpers, name):
            failures += 1
            print(f"FAIL {package}: /usr/lib/flea/{name} is named in {helper_first[name]} and not installed")
    checks += 1
    if not list(ui.glob("vendor/LICENSES/*")):
        failures += 1
        print(f"FAIL {package}: missing vendor/LICENSES")
    for module in modules:
        for relative in imports.findall(module.read_text()):
            checks += 1
            target = (module.parent / relative).resolve()
            valid = target.is_relative_to(ui.resolve()) and target.is_file()
            if not valid:
                failures += 1
                print(f"FAIL {package}: {module.relative_to(ui)} imports missing {relative}")
# flea-bin's package() must install what the root package's make install does, each package's own licenses directory read as one name.
# Sample input: usr/share/licenses/flea-bin/LICENSE at 644, read for "flea-bin", gives ("usr/share/licenses/$pkgname/LICENSE", "0o644", "").
def package_rows(base, licenses):
    rows = set()
    prefix = f"usr/share/licenses/{licenses}"
    for top, dirs, files in os.walk(base):
        for name in dirs + files:
            path = pathlib.Path(top) / name
            relative = path.relative_to(base).as_posix()
            if relative == prefix or relative.startswith(prefix + "/"):
                relative = "usr/share/licenses/$pkgname" + relative[len(prefix):]
            link = os.readlink(path) if path.is_symlink() else ""
            rows.add((relative, "link" if link else oct(path.lstat().st_mode & 0o7777), link))
    return rows
root_rows = package_rows(root / "root" / "pkg", "flea")
bin_rows = package_rows(root / "flea-bin" / "pkg", "flea-bin")
checks += 1
if root_rows != bin_rows:
    failures += 1
    for row in sorted(root_rows - bin_rows):
        print(f"FAIL flea-bin: lacks {' '.join(filter(None, row))}, which the root package installs")
    for row in sorted(bin_rows - root_rows):
        print(f"FAIL flea-bin: installs {' '.join(filter(None, row))}, which the root package does not")
print(f"package trees compared: {len(root_rows)} root, {len(bin_rows)} flea-bin")
# Sample input: `install -Dm644 "$repo"/ui/js/*.js "$repo"/ui/js/*.mjs -t "$root/ui/js"` gives (["$repo/ui/js/*.js", "$repo/ui/js/*.mjs"], "$root/ui/js", True).
def install_statements(text):
    statements = []
    for line in re.sub(r"\\\n\s*", " ", text).splitlines():
        if line.split()[:1] != ["install"]:
            continue
        words = shlex.split(line, comments=True)
        sources, destination, directory, args = [], None, False, words[1:]
        while args:
            word = args.pop(0)
            if word == "-t":
                destination, directory = args.pop(0), True
            elif not word.startswith("-"):
                sources.append(word)
        if destination is None:
            destination = sources.pop()
        statements.append((sources, destination, directory))
    return statements

# A path the tarball script stages, relative to its $root: the -t directory plus the source's basename pattern, or the file named last.
def staged_paths(text):
    staged = set()
    for sources, destination, directory in install_statements(text):
        if destination != "$root" and not destination.startswith("$root/"):
            continue
        relative = destination.removeprefix("$root").strip("/")
        if not directory:
            staged.add(relative)
        else:
            staged.update(f"{relative}/{source.rsplit('/', 1)[1]}".lstrip("/") for source in sources if source.startswith("$repo/"))
    return staged

repo = pathlib.Path(sys.argv[2])
body = re.search(r'^package\(\) \{\n(.*?)^\}', (repo / "packaging/flea-bin/PKGBUILD").read_text(), re.S | re.M).group(1)
required = [source for sources, _, _ in install_statements(body) for source in sources]
tarball = (repo / "packaging/flea-bin-tarball").read_text()
checks += 1
if "ui/qmldir" not in required:
    failures += 1
    print("FAIL flea-bin: no install path could be read from package()")
staged = staged_paths(tarball)
# The binary is staged from the script's first argument to $root/flea, so package()'s flea is asserted like any other path.
for source in required:
    checks += 1
    if source not in staged:
        failures += 1
        print(f"FAIL flea-bin: package() installs {source}, which packaging/flea-bin-tarball does not stage there")
# Red controls: a doctored copy of the script text must report exactly the path it broke, and nothing else.
vendor = 'install -Dm644 "$repo"/ui/vendor/*.mjs -t "$root/ui/vendor"'
controls = [
    ("vendor modules staged in ui/js", tarball.replace(vendor, vendor.replace("$root/ui/vendor", "$root/ui/js")), "ui/vendor/*.mjs"),
    ("ui/js/*.mjs token deleted", tarball.replace(' "$repo"/ui/js/*.mjs', "", 1), "ui/js/*.mjs"),
    ("vendor install turned into a comment", tarball.replace(vendor, "# " + vendor), "ui/vendor/*.mjs"),
]
for label, doctored, expected in controls:
    checks += 1
    reported = [source for source in required if source not in staged_paths(doctored)]
    if doctored == tarball or reported != [expected]:
        failures += 1
        print(f"FAIL control {label}: reported {reported}, want only {expected}")
    else:
        print(f"control {label}: reported only {expected}, as a broken script must")
# A copied package tree with one shelf file and one helper removed must report exactly those two names.
control_pkg = root / "control" / "pkg"
shutil.copytree(root / "root" / "pkg", control_pkg, symlinks=True)
shelf_victim = sorted(shelf_files)[0] if shelf_files else None
helper_victim = sorted(helper_names)[0] if helper_names else None
# An empty set or a victim the root package lacks reports one named FAIL and skips the control.
control_ready = (shelf_victim is not None and helper_victim is not None
    and (control_pkg / "usr/share/flea/shelf" / shelf_victim).is_file()
    and (control_pkg / "usr/lib/flea" / helper_victim).is_file())
checks += 1
if not control_ready:
    failures += 1
    print("FAIL control removed-file: an empty set or a missing victim leaves nothing to remove")
else:
    (control_pkg / "usr/share/flea/shelf" / shelf_victim).unlink()
    (control_pkg / "usr/lib/flea" / helper_victim).unlink()
    # Sample input: {"Panel.qml"} shipped against {"Model.js", "Panel.qml"} reports ["Model.js"].
    control_shipped = {path.relative_to(control_pkg / "usr/share/flea/shelf").as_posix() for path in (control_pkg / "usr/share/flea/shelf").rglob("*") if path.is_file()}
    checks += 1
    reported = sorted(shelf_files - control_shipped)
    if reported != [shelf_victim]:
        failures += 1
        print(f"FAIL control shelf file removed: reported {reported}, want only {shelf_victim}")
    else:
        print(f"control shelf file removed: reported only {shelf_victim}, as a broken script must")
    checks += 1
    reported_helpers = sorted(name for name in helper_names if not helper_installed(control_pkg / "usr/lib/flea", name))
    if reported_helpers != [helper_victim]:
        failures += 1
        print(f"FAIL control helper removed: reported {reported_helpers}, want only {helper_victim}")
    else:
        print(f"control helper removed: reported only {helper_victim}, as a broken script must")
    checks += 1
    reported_rows = sorted(row[0] for row in root_rows - package_rows(control_pkg, "flea"))
    want_rows = sorted([f"usr/share/flea/shelf/{shelf_victim}", f"usr/lib/flea/{helper_victim}"])
    if reported_rows != want_rows:
        failures += 1
        print(f"FAIL control package trees: reported {reported_rows}, want only {want_rows}")
    else:
        print(f"control package trees: reported only {', '.join(want_rows)}, as a drifted package() must")
print(f"figure-package: {checks} check(s), {failures} failed")
sys.exit(bool(failures))
PY
