#!/usr/bin/env python3
"""Compare recipe text without executing either PKGBUILD."""

import difflib
from pathlib import Path
import re
import sys

# Each alternative starts on a different character, so a rejected line fails in linear time.
SHA256SUMS = re.compile(r'''(?m)^sha256sums=\(((?:\s|'[^'\n]*'|"[^"\n]*"|[A-Za-z0-9])*)\)[ \t]*$''')


def normalize(text):
    # Sample input: "# Maintainer: GM <...>" goes; "pkgver=0.4.0", "pkgrel=1" and "sha256sums=('f16c...')" become <release>.
    text = re.sub(r"(?m)^# Maintainer:[^\n]*\n?", "", text)
    for name in ("pkgver", "pkgrel"):
        assignment = rf"(?m)^{name}=('[^'\n]*'|\"[^\"\n]*\"|[A-Za-z0-9_.+:-]+)([ \t]*(?:#[^\n]*)?)$"
        text, count = re.subn(assignment, rf"{name}=<release>\2", text)
        if count != 1:
            raise ValueError(f"expected one {name} assignment, found {count}")
    sums = SHA256SUMS.findall(text)
    if len(sums) != 1:
        raise ValueError(f"expected one static sha256sums array, found {len(sums)}")
    if "SKIP" in sums[0]:
        raise ValueError("sha256sums is SKIP, so makepkg would not verify the source")
    return SHA256SUMS.sub("sha256sums=(<release>)", text)


def controls(text):
    """Return the names of the in-memory drift controls normalize() got wrong on this recipe."""
    def same(edited):
        try:
            return normalize(edited) == normalize(text)
        except ValueError:
            return False
    release = re.sub(r"(?m)^pkgver=.*$", "pkgver=9.9.9", text)
    release = re.sub(r"(?m)^pkgrel=.*$", "pkgrel=7", release)
    release = SHA256SUMS.sub("sha256sums=('" + "1" * 64 + "')", release)
    release = re.sub(r"(?m)^# Maintainer:.*$", "# Maintainer: Someone Else <someone@example.com>", release)
    cases = {
        "release fields may change": (release, True),
        "a dependency may not change": (text.replace("depends=(\n", "depends=(\n  'drift'\n", 1), False),
        "code after pkgver is refused": (re.sub(r"(?m)^pkgver=(.*)$", r"pkgver=\1; true", text), False),
        "a SKIP checksum is refused": (SHA256SUMS.sub("sha256sums=('SKIP')", text), False),
        "an unparsable checksum is refused": (SHA256SUMS.sub("sha256sums=(" + "a" * 40 + "!)", text), False),
    }
    return [name for name, (edited, expected) in cases.items() if same(edited) != expected]


def main():
    try:
        root, opr = (Path(value) for value in sys.argv[1:])
        before, after = (normalize(path.read_text()) for path in (root, opr))
    except (OSError, ValueError) as error:
        print(f"opr-recipe: FAIL {error}", file=sys.stderr)
        return 1
    failed = controls(root.read_text())
    if failed:
        print(f"opr-recipe: FAIL drift controls: {', '.join(failed)}")
        return 1
    if before != after:
        sys.stdout.writelines(difflib.unified_diff(
            before.splitlines(keepends=True), after.splitlines(keepends=True),
            fromfile=str(root), tofile=str(opr),
        ))
        print("opr-recipe: FAIL recipes differ outside release metadata and Maintainer")
        return 1
    print("opr-recipe: PASS recipes match outside release metadata and Maintainer")
    return 0


if __name__ == "__main__":
    sys.exit(main())
