"""Display-free executable identities shared by inventory and sample checks."""

import re
import subprocess

PACKAGED = frozenset(("krusader", "dolphin", "nautilus", "thunar", "pcmanfm", "nemo"))
CLI = frozenset(("flea", "flea-0312", "strata-stable", "strata-nightly"))
VERSION = re.compile(r"\d+\.\d+(?:\.\d+)?(?:[-.][A-Za-z0-9.]+)?")


def probe(ident, binary, prefix, env, cwd):
    commands = []

    def query(argv):
        result = subprocess.run(prefix + argv, env=env, cwd=cwd, capture_output=True, text=True, timeout=10)
        record = dict(argv=prefix + argv, rc=result.returncode, stdout=result.stdout.strip(), stderr=result.stderr.strip())
        commands.append(record)
        if result.returncode:
            raise ValueError(f"cannot inventory {ident}: {record['stdout']} {record['stderr']}".strip())
        return record

    if ident in PACKAGED:
        owner = query(["pacman", "-Qoq", "--", str(binary)])["stdout"]
        if owner != ident:
            raise ValueError(f"unexpected package owner for {ident}: {owner}")
        record = query(["pacman", "-Q", "--", owner])
        fields = record["stdout"].split()
        if len(fields) != 2 or fields[0] != owner or "\n" in record["stdout"]:
            raise ValueError(f"invalid package version for {ident}: {record['stdout']}")
        package_version = fields[1]
        version = package_version.split(":", 1)[-1].rsplit("-", 1)[0]
        if not VERSION.fullmatch(version) or "-" not in package_version:
            raise ValueError(f"invalid package version for {ident}: {package_version}")
        return dict(version=version, version_output=record["stdout"], version_method="package", package_name=owner,
                    package_version=package_version, version_commands=commands)
    if ident not in CLI:
        raise ValueError(f"no display-free version policy for {ident}")
    record = query([str(binary), "--version"])
    output = "\n".join(value for value in (record["stdout"], record["stderr"]) if value)
    match = VERSION.search(output)
    if not match:
        raise ValueError(f"cannot inventory {ident}: {output}")
    return dict(version=match.group(), version_output=output, version_method="cli", version_commands=commands)


def check(expected, actual):
    ident = expected["id"]
    if actual["version"] != expected["version"]:
        raise ValueError(f"binary differs from inventoried version: {ident}: {actual['version_output']}")
    if actual["version_method"] == "package":
        for key in ("package_name", "package_version"):
            if actual[key] != expected.get(key):
                raise ValueError(f"package version differs from inventory: {ident}: {actual['version_output']}")
