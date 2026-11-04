#!/usr/bin/env python3
"""Check parser-child locale isolation against the actual shell pipeline and callers."""
import argparse
import atexit
import ast
import locale
import os
from pathlib import Path
import runpy
import shutil
import subprocess
import sys
import traceback

try:
    REPO = Path(__file__).resolve().parents[1]
    SHELL = runpy.run_path(str(Path(__file__).with_name("locale-shell.py")))
except Exception:
    traceback.print_exc()
    raise SystemExit(3)


def filter_sites():
    for path, source in SHELL["shell_files"](REPO):
        for start, name, prefix in SHELL["commands"](source):
            if name in SHELL["FILTER"]:
                yield path, source.count("\n", 0, start) + 1, name, prefix


def function(source, name):
    start = source.index(name + "() {")
    return source[start:source.index("\n}\n", start) + 3]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--regress-uniq", action="store_true",
                        help="remove uniq's locale scope; this negative control must fail")
    parser.add_argument("--regress-caller", action="store_true",
                        help="restore the assignment-only prefix; this negative control must fail")
    parser.add_argument("--regress-charset", action="store_true",
                        help="restore the ASCII locale in UTF-8 probes; this control must fail")
    parser.add_argument("--regress-decimal", action="store_true",
                        help="remove the numeric formatter pins; this control must fail in de_DE.UTF-8")
    parser.add_argument("--regress-fixture-scope", action="store_true",
                        help="remove the tool's export; the pipeline must detect the lost scope")
    byte_modes = parser.add_mutually_exclusive_group()
    byte_modes.add_argument("--regress-manifest-byte-mode", action="store_true",
                            help="restore UTF-8 matching in the manifest check only; this control must fail")
    byte_modes.add_argument("--regress-fixture-byte-mode", action="store_true",
                            help="restore UTF-8 matching in the matched-fixture check only; this control must fail")
    parser.add_argument("--keep", action="store_true", help="keep the marked fixture root for inspection")
    args = parser.parse_args()
    inherited = dict(os.environ)
    requested = inherited.get("LC_ALL", "C")
    try:
        locale.setlocale(locale.LC_ALL, requested)
    except locale.Error as error:
        parser.error(f"requested locale {requested} is unavailable: {error}")
    try:
        locale.setlocale(locale.LC_ALL, "de_DE.UTF-8")
    except locale.Error:
        print("NOT RUN German sys_seconds and cpu_s use dot decimals: de_DE.UTF-8 is unavailable", file=sys.stderr)
        return 3
    finally:
        locale.setlocale(locale.LC_ALL, requested)
    inherited["LC_ALL"] = requested
    inherited["LANG"] = requested
    inherited["LANGUAGE"] = "de:ar"
    temp_env = dict(inherited, LC_ALL="C.UTF-8")
    temp_env.pop("LANGUAGE", None)
    parent = REPO / ".superpowers/i0"
    if not parent.resolve().is_relative_to(REPO):
        raise RuntimeError("REFUSED locale fixture parent outside the repository")
    parent.mkdir(parents=True, exist_ok=True)
    root = Path(subprocess.check_output(
        ["mktemp", "-d", str(REPO / ".superpowers/i0/locale-regression.XXXXXX")],
        env=temp_env, text=True).strip())
    if not root.is_absolute() or root.parent != parent:
        raise RuntimeError("REFUSED unexpected locale fixture root")
    (root / ".flea-test-sandbox").write_text("locale parser regression fixtures\n")
    success = False

    def cleanup():
        if not args.keep and success:
            if not (str(root) and root.is_absolute() and root.resolve().parent == parent.resolve()
                    and not root.is_symlink() and root.name.startswith("locale-regression.")
                    and (root / ".flea-test-sandbox").is_file()):
                raise RuntimeError("REFUSED removal of an unmarked or unexpected locale fixture")
            shutil.rmtree(root)
        elif not success:
            print(f"kept evidence={root}", file=sys.stderr)

    atexit.register(cleanup)
    errors = []
    observations = []
    checks = 0

    def check(label, passed, actual):
        nonlocal checks
        checks += 1
        if not passed:
            errors.append(f"FAIL {label}: {actual}")
        return passed

    samples = ["sort a", "ok && sort a", "ok || sort a", "! sort a",
               "xargs -0 -n 2 sort", "xargs -I {} sort", "xargs -I{} sort",
               "env -u LANGUAGE LC_ALL=C.UTF-8 sort", "then sort", "do sort",
               "time -f %e sort", "nice -n 4 sort", "command -- sort",
               "ok && time nice -n 4 env -u LANGUAGE LC_ALL=C.UTF-8 sort"]
    for sample in samples:
        check(f"inventory command position {sample}",
              sum(name == "sort" for _, name, _ in SHELL["commands"](sample)) == 1, sample)
    check("inventory ignores data and comments",
          not any(name in SHELL["FILTER"] for _, name, _ in SHELL["commands"](
              "echo 'sort a'\n# uniq a\ncat <<'DATA'\nsort a\nDATA\n")), "quoted commands")
    sites = list(filter_sites())
    inventory = list(SHELL["inventory"](REPO))
    for path, line, program, forced, prefix in inventory:
        check(f"{path.relative_to(REPO)}:{line} {program} scope", forced, repr(prefix))
    (root / "inventory.tsv").write_text("".join(
        f"{path.relative_to(REPO)}:{line}\t{program}\t{'scoped' if forced else 'UNPINNED'}\n"
        for path, line, program, forced, _ in inventory))

    files = root / "files"
    files.mkdir()
    for name in ["one.jpg", "two.jpg", "three.png"]:
        (files / name).touch()
    bin_dir = root / "bin"
    bin_dir.mkdir()
    capture = root / "filters.tsv"
    for program in ["sort", "uniq", "comm", "join"]:
        wrapper = bin_dir / program
        wrapper.write_text(
            "#!/bin/sh\n"
            f"printf '%s\\t%s\\t%s\\n' '{program}' \"$LC_ALL\" "
            '"${LANGUAGE-unset}" >> "$I0_FILTER_LOG"\n'
            f'exec /usr/bin/{program} "$@"\n')
        wrapper.chmod(0o755)
    source = (REPO / "tools/flea-matched-fixture").read_text()
    if args.regress_fixture_scope:
        source = source.replace("export LC_ALL=C.UTF-8\n", "")
    pipeline = next(line for line in source.splitlines()
                    if 'ls "$dir" |' in line and "uniq -c" in line)
    if args.regress_uniq:
        pipeline = pipeline.replace("uniq -c", "env -u LC_ALL uniq -c")
    env = dict(inherited, PATH=str(bin_dir) + ":" + inherited["PATH"],
               I0_FILTER_LOG=str(capture))
    tool_scope = SHELL["export_scope"](source)
    result = subprocess.run(["/bin/bash", "-c", tool_scope + "dir=$1\n" + pipeline,
                             "locale-regression", str(files)],
                            env=env, capture_output=True, text=True)
    check("matched fixture counts", result.returncode == 0 and
          result.stdout.split() == ["2", "jpg", "1", "png"],
          (result.returncode, result.stdout, result.stderr))
    observed = [line.split("\t") for line in capture.read_text().splitlines()]
    check("pipeline filter coverage", sorted(row[0] for row in observed) ==
          ["sort", "sort", "uniq"], observed)
    for program, actual_locale, language in observed:
        check(f"matched fixture {program} child environment",
              (actual_locale, language) == ("C.UTF-8", "unset"),
              f"{actual_locale}|{language}")

    raw_name = os.fsencode(files) + b"/caf\xe9.jpg"
    os.close(os.open(raw_name, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600))
    manifest = (REPO / "tools/flea-bench-manifest").read_text()
    histogram = next(line for line in manifest.splitlines()
                     if 'ls -U "$FIXTURE" |' in line and "uniq -c" in line)
    byte_pipeline = pipeline
    if args.regress_manifest_byte_mode:
        mutated = histogram.replace("LC_ALL=C LANGUAGE= ", "LC_ALL=C.UTF-8 LANGUAGE= ")
        if mutated == histogram:
            print("FAIL --regress-manifest-byte-mode: replacement changed no text", file=sys.stderr)
            return 3
        histogram = mutated
    if args.regress_fixture_byte_mode:
        mutated = byte_pipeline.replace("LC_ALL=C LANGUAGE= ", "LC_ALL=C.UTF-8 LANGUAGE= ")
        if mutated == byte_pipeline:
            print("FAIL --regress-fixture-byte-mode: replacement changed no text", file=sys.stderr)
            return 3
        byte_pipeline = mutated
    counts = root / "extensions.txt"
    result = subprocess.run(["/bin/bash", "-c", SHELL["export_scope"](manifest) +
                             'FIXTURE=$1; OUT=$2\n' + histogram, "extensions", str(files), str(counts)],
                            env=inherited, capture_output=True)
    check("manifest counts a Latin-1 name as jpg", result.returncode == 0 and
          counts.read_bytes().split() == [b"3", b"jpg", b"1", b"png"], counts.read_bytes())
    result = subprocess.run(["/bin/bash", "-c", tool_scope + 'dir=$1\n' + byte_pipeline,
                             "extensions", str(files)], env=inherited, capture_output=True)
    check("matched fixture counts a Latin-1 name as jpg", result.returncode == 0 and
          result.stdout.split() == [b"3", b"jpg", b"1", b"png"], result.stdout)

    capability = (REPO / "tools/flea-bench-capability").read_text()
    field = (REPO / "tools/flea-field-bench").read_text()
    showcase = (REPO / "tools/flea-showcase").read_text()
    trim_id = next(line.strip() for line in capability.splitlines()
                   if "id=${id//[[:space:]]/}" in line)
    field_id = next(line.strip() for line in field.splitlines()
                    if "id=${f[0]:-}" in line)
    instance_id = next(line.strip() for line in showcase.splitlines()
                       if 'id=$(instance_id_of_pid "$pid")' in line)
    identity = function(capability, "process_identity")
    if args.regress_caller:
        identity = identity.replace("stat=$(<", "LC_ALL=C.UTF-8 LANGUAGE= stat=$(<")
    probes = [
        ("process_identity", identity +
         '\nprocess_identity "$$" || exit 1'),
        ("capability identifier", "id=' f i x t u r e '\n" + trim_id +
         '\n[ "$id" = fixture ] || exit 1'),
        ("field identifier", "f=(fixture)\n" + field_id +
         '\n[ "$id" = fixture ] || exit 1'),
        ("showcase instance", "pid=$$\ninstance_id_of_pid() { printf 42; }\n" +
         instance_id + '\n[ "$id" = 42 ] || exit 1'),
    ]
    probe = "\n/bin/sh -c 'printf \"%s|%s|%s\" \"$LC_ALL\" " \
            "\"${LANGUAGE-unset}\" \"$LANG\"'"
    for label, body in probes:
        result = subprocess.run(["/bin/bash", "-c", body + probe],
                                env=inherited, capture_output=True, text=True)
        check(f"{label} preserves subsequent child locale",
              result.returncode == 0 and result.stdout ==
              f"{requested}|de:ar|{requested}",
              (result.returncode, result.stdout, result.stderr))
    utf8_scope = "LC_ALL=C.UTF-8 LANGUAGE= "
    if args.regress_charset:
        utf8_scope = "LC_ALL=C LANGUAGE= "
    name_filter = next(line for line in field.splitlines() if 'grep -E ".{$min_name_chars,}"' in line)
    if args.regress_charset:
        name_filter = name_filter.replace("C.UTF-8", "C")
    names = root / "names"
    names.mkdir()
    for name in ["短名", "四个汉字", "ascii"]:
        (names / name).touch()
    names_output = root / "names.txt"
    body = 'FIXTURE=$1; TUI_NAMES=$2; first_screen_names=10; min_name_chars=4\n' + name_filter
    result = subprocess.run(["/bin/bash", "-c", body, "names", str(names), str(names_output)],
                            env=inherited, capture_output=True, text=True)
    check("minimum name length counts CJK characters", result.returncode == 0 and
          names_output.read_text().splitlines() == ["ascii", "四个汉字"],
          (result.returncode, names_output.read_text(), result.stderr))
    utf8_file = files / "ñandú-é.txt"
    utf8_file.touch()
    result = subprocess.run(["/bin/bash", "-c", utf8_scope + 'gio info "$1"', "gio", str(utf8_file)],
                            env=inherited, capture_output=True, text=True)
    if check("gio preserves UTF-8 display names", result.returncode == 0 and
             "display name: ñandú-é.txt" in result.stdout, result.stdout) and args.regress_charset:
        observations.append(SHELL["SCOPE"]["GIO_UTF8_OBSERVED"])
        print(observations[-1])
    process = subprocess.Popen(["sleep-ñé", "30"], executable="/usr/bin/sleep", env=inherited)
    try:
        result = subprocess.run(["/bin/bash", "-c", utf8_scope + 'ps -o args= -p "$1"',
                                 "ps", str(process.pid)], env=inherited, capture_output=True, text=True)
        check("ps preserves UTF-8 process arguments", result.returncode == 0 and
              "sleep-ñé 30" in result.stdout, result.stdout)
    finally:
        process.terminate()
        process.wait(timeout=5)

    cpu_line = next(line.strip() for line in field.splitlines() if line.strip().startswith("cpu_main_s=$("))
    numeric = function(field, "sys_seconds") + '\nsys_seconds 1.25 50 100; printf "|"\n'
    numeric += 'last_entrant=125; ticks_per_second=100\n' + cpu_line + '\nprintf "%s" "$cpu_main_s"'
    if args.regress_decimal:
        numeric = numeric.replace("LC_ALL=C.UTF-8 LANGUAGE= ", "")
    # POSIX mode makes awk implementations that default to dot decimals honor LC_NUMERIC.
    german = dict(inherited, LC_ALL="de_DE.UTF-8", LANG="de_DE.UTF-8", POSIXLY_CORRECT="1")
    result = subprocess.run(["/bin/bash", "-c", numeric], env=german, capture_output=True, text=True)
    check("German sys_seconds and cpu_s use dot decimals", result.returncode == 0 and
          result.stdout == "1.75|1.25", (result.returncode, result.stdout, result.stderr))

    for tool, function_name in [("flea-field-search", "calibrate"),
                                ("flea-field-instant", "calib_window"),
                                ("flea-figure-bench", "one")]:
        tree = ast.parse((REPO / "tools" / tool).read_text())
        fn = next(n for n in tree.body if isinstance(n, ast.FunctionDef) and n.name == function_name)
        launch = next(n for n in ast.walk(fn) if isinstance(n, ast.Call)
                      and ast.unparse(n.func) == "subprocess.Popen")
        env_expr = next((k.value for k in launch.keywords if k.arg == "env"), None)
        namespace = {"env": inherited, "S": type("Session", (), {"ENV": inherited}),
                     "delay": 0, "str": str, "dict": dict}
        actual_env = inherited if env_expr is None else eval(compile(ast.Expression(env_expr), tool, "eval"), namespace)
        result = subprocess.run(["/bin/bash", "-c", probe], env=actual_env, capture_output=True, text=True)
        check(f"{tool} measured client keeps session locale", result.returncode == 0 and
              result.stdout == f"{requested}|de:ar|{requested}", result.stdout)
    if errors:
        print("\n".join(errors), file=sys.stderr)
        print(f"FAIL locale={requested}; evidence={root}", file=sys.stderr)
        SHELL["SCOPE"]["emit_detection"]("parsers", args, errors, observations)
        return 1
    success = True
    print(f"PASS locale={requested} decimal={locale.localeconv()['decimal_point']!r}; "
          f"{len(sites)} filter sites, {len(inventory)} parsed sites, {checks} checks; evidence={root}")
    return 0


if __name__ == "__main__":
    try:
        result = main()
    except Exception:
        traceback.print_exc()
        result = 3
    sys.exit(result)
