#!/usr/bin/env python3
"""Read installed palette inputs without switching the desktop theme."""

import json
import os
from pathlib import Path
import re
import subprocess
import sys


def render_shell(omarchy, colors):
    shell = colors.with_name("shell.toml")
    if shell.is_file():
        text = shell.read_text()
    else:
        output = subprocess.check_output(
            ["bash", str(omarchy / "bin/omarchy-theme-color"), "--file", str(colors), "--all"],
            text=True,
        )
        values = dict(line.split("\t", 1) for line in output.splitlines())

        def reference(key, fallback=""):
            return values.get(key, values.get(fallback, fallback or key))

        def replace(match):
            parts = match.group(1).split()
            if len(parts) == 1:
                return reference(parts[0])
            if parts[0] == "shell_gradient" and len(parts) in (2, 3):
                return reference(*parts[1:])
            if parts[0] == "mix" and len(parts) == 4:
                a, b = reference(parts[1]), reference(parts[2])
                amount = float(parts[3].rstrip("%"))
                if parts[3].endswith("%") or amount > 1:
                    amount /= 100
                amount = max(0, min(1, amount))
                if not all(re.fullmatch(r"#[0-9A-Fa-f]{6}", c) for c in (a, b)):
                    raise ValueError(f"unsupported mix colors: {a}, {b}")
                channels = [int(int(a[i:i + 2], 16) * (1 - amount) + int(b[i:i + 2], 16) * amount + 0.5)
                            for i in (1, 3, 5)]
                return "#" + "".join(f"{c:02x}" for c in channels)
            raise ValueError(f"unsupported shell template token: {match.group(0)}")

        template = (omarchy / "default/themed/shell.toml.tpl").read_text()
        text = re.sub(r"\{\{\s*(.*?)\s*\}\}", replace, template)
    for override in sorted(colors.parent.glob("shell.*.toml")):
        section = override.name[len("shell."):-len(".toml")]
        if not re.fullmatch(r"[A-Za-z0-9_-]+", section):
            continue
        lines, inside = [], False
        for line in text.splitlines():
            header = re.fullmatch(r"\s*\[([^]]+)\]\s*(?:#.*)?", line)
            if header:
                inside = header[1] == section
            if not inside:
                lines.append(line)
        body = re.sub(r"(?m)^\s*\[" + re.escape(section) + r"\]\s*(?:#.*)?$", "", override.read_text())
        text = "\n".join(lines) + f"\n[{section}]\n" + body
    return text


def main():
    scratch, commons = map(Path, sys.argv[1:3])
    assert scratch.is_absolute() and (scratch / ".g05-test-root").is_file()
    os.environ["HOME"] = str(scratch / "home")
    palettes = {}
    for root in map(Path, sys.argv[3:]):
        if not root.is_dir():
            continue
        for colors in sorted(root.glob("*/colors.toml")):
            name = colors.parent.name
            if name in palettes:
                raise ValueError(f"duplicate palette name: {name}; pass disjoint palette roots")
            palettes[name] = dict(name=name, colors=colors.read_text(), shell=render_shell(commons.parent, colors))
    if not palettes:
        raise ValueError("no installed palettes found")
    (scratch / "palettes.json").write_text(json.dumps(list(palettes.values())))


if __name__ == "__main__":
    main()
