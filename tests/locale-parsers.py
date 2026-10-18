#!/usr/bin/env python3
"""Check parser-child locale isolation against the actual shell pipeline and callers."""
import argparse
import locale
import os
from pathlib import Path
import re
import subprocess
import sys

REPO = Path(__file__).resolve().parents[1]
FILTER = re.compile(
    r"(?:^|[|;(<]|\b(?:if|then|do)\b)\s*"
    r"(?P<prefix>(?:(?:LC_ALL=C|LANGUAGE=)\s+)*)"
    r"(?P<program>sort|uniq|comm|join)(?=\s|[);]|$)", re.M)
SHELL_LIBRARIES = {"flea-ipc-pid", "flea-sandbox-guard", "flea-pkgrel-field"}


def filter_sites():
    for path in sorted((REPO / "tools").rglob("*")):
        if not path.is_file():
            continue
        source = path.read_text(errors="replace")
        if not (source.startswith(("#!/bin/bash", "#!/usr/bin/env bash", "#!/bin/sh"))
                or path.name in SHELL_LIBRARIES):
            continue
        for match in FILTER.finditer(source):
            start = match.start("program")
            line = source.count("\n", 0, start) + 1
            if source.splitlines()[line - 1].lstrip().startswith("#"):
                continue
            yield path, line, match["program"], match["prefix"]


def function(source, name):
    start = source.index(name + "() {")
    return source[start:source.index("\n}\n", start) + 3]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--regress-uniq", action="store_true",
                        help="remove uniq's locale scope; this negative control must fail")
    parser.add_argument("--regress-caller", action="store_true",
                        help="restore the assignment-only prefix; this negative control must fail")
    args = parser.parse_args()
    inherited = dict(os.environ)
    requested = inherited.get("LC_ALL", "C")
    try:
        locale.setlocale(locale.LC_ALL, requested)
    except locale.Error as error:
        parser.error(f"requested locale {requested} is unavailable: {error}")
    inherited["LC_ALL"] = requested
    inherited["LANG"] = requested
    inherited["LANGUAGE"] = "de:ar"
    temp_env = dict(inherited, LC_ALL="C")
    temp_env.pop("LANGUAGE", None)
    root = Path(subprocess.check_output(
        ["mktemp", "-d", str(REPO / ".superpowers/i0/locale-regression.XXXXXX")],
        env=temp_env, text=True).strip())
    assert root.is_absolute() and root.parent == REPO / ".superpowers/i0"
    (root / ".flea-test-sandbox").write_text("locale parser regression fixtures\n")
    errors = []
    checks = 0

    def check(label, passed, actual):
        nonlocal checks
        checks += 1
        if not passed:
            errors.append(f"FAIL {label}: {actual}")

    sites = list(filter_sites())
    for path, line, program, prefix in sites:
        check(f"{path.relative_to(REPO)}:{line} {program} scope",
              prefix.split() == ["LC_ALL=C", "LANGUAGE="], repr(prefix))

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
    pipeline = next(line for line in source.splitlines()
                    if ' ls "$dir" |' in line and "uniq -c" in line)
    if args.regress_uniq:
        pipeline = pipeline.replace("LC_ALL=C LANGUAGE= uniq -c", "uniq -c")
    env = dict(inherited, PATH=str(bin_dir) + ":" + inherited["PATH"],
               I0_FILTER_LOG=str(capture))
    result = subprocess.run(["/bin/bash", "-c", "dir=$1\n" + pipeline,
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
              (actual_locale, language) == ("C", ""),
              f"{actual_locale}|{language}")

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
        identity = identity.replace("stat=$(<", "LC_ALL=C LANGUAGE= stat=$(<")
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
    if errors:
        print("\n".join(errors), file=sys.stderr)
        print(f"FAIL locale={requested}; evidence={root}", file=sys.stderr)
        return 1
    print(f"PASS locale={requested} decimal={locale.localeconv()['decimal_point']!r}; "
          f"{len(sites)} filter sites, {checks} checks; evidence={root}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
