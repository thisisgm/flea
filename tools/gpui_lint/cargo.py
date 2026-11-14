"""Check manifests, locked sources and Cargo's resolved package identities."""
import json
import os
from pathlib import Path
import re
import subprocess
import tomllib
from urllib.parse import parse_qs, urlsplit


def run(args, root):
    process = subprocess.run(args, cwd=root, capture_output=True, text=True, check=False)
    if process.returncode:
        raise ValueError(f"{' '.join(args)} exited {process.returncode}: {process.stderr.strip()}")
    return process.stdout


def tracked(root):
    # Only a checkout has a .git entry; an archive export has none and needs no Git call.
    if (root / ".git").exists():
        process = subprocess.run(["git", "ls-files", "-z"], cwd=root, capture_output=True, text=True)
        if process.returncode:
            raise ValueError(f"git ls-files exited {process.returncode}: {process.stderr.strip()}")
        return [Path(name) for name in process.stdout.split("\0") if name]
    # CI exports have no index; checking every exported source is a superset of tracked inputs.
    paths = []
    for directory, folders, files in os.walk(root, followlinks=False):
        current = Path(directory)
        if current == root:
            folders[:] = [f for f in folders if f not in (".git", ".superpowers", ".flea-local", "target")]
        for name in files:
            path = (current / name).relative_to(root)
            if path.suffix in (".rs", ".toml") or name in ("Cargo.lock", "config"):
                paths.append(path)
        for folder in folders:
            path = current / folder
            if path.is_symlink():
                paths.append(path.relative_to(root))
    return sorted(paths)


def read_metadata(root):
    return json.loads(run(["cargo", "metadata", "--locked", "--format-version", "1"], root))


def gpui(name):
    return name == "gpui" or name.startswith(("gpui-", "gpui_"))


# Sample input: "git+https://example.org/fork.git?rev=<sha>#<sha>" for the registered fork URL and revision.
def source_matches(source, fork):
    if not isinstance(source, str) or not source.startswith("git+"):
        return False
    url = urlsplit(source[4:])
    return (url._replace(query="", fragment="").geturl() == fork["url"]
            and parse_qs(url.query) == {"rev": [fork["rev"]]} and url.fragment == fork["rev"])


def source_checks(root, paths, metadata, policy):
    errors, count = [], 0
    fork = policy["fork"]
    if not re.fullmatch(r"[0-9a-f]{40}", fork["rev"]):
        errors.append("fork registration needs a full lowercase commit SHA")
    packages = metadata.get("packages", [])
    if not packages or not metadata.get("resolve", {}).get("nodes"):
        errors.append("Cargo metadata has no resolved package graph")
    for package in packages:
        source = package.get("source")
        if gpui(package["name"]):
            count += 1
            if not source_matches(source, fork):
                errors.append(f"{package['name']} {package['version']}: GPUI source is not the single fork pin: {source}")
        elif isinstance(source, str) and source.startswith("git+"):
            errors.append(f"{package['name']}: unknown git source {source}")
        elif source is None and package["id"] not in metadata.get("workspace_members", []):
            errors.append(f"{package['name']}: unknown path dependency {package.get('manifest_path')}")
    if not count:
        errors.append("Cargo metadata resolves no GPUI packages")
    lock = tomllib.loads((root / "Cargo.lock").read_text())
    for package in lock.get("package", []):
        if gpui(package["name"]) and not source_matches(package.get("source"), fork):
            errors.append(f"Cargo.lock: {package['name']} {package['version']} has an unapproved GPUI source")
    for path in paths:
        native_input = path.parts[0] in ("crates", ".cargo", "vendor", "vendored") or path.name.startswith("Cargo.")
        if native_input and not (root / path).resolve().is_relative_to(root.resolve()):
            errors.append(f"{path}: source symlink escapes the product tree")
            continue
        if path.name == "Cargo.toml":
            data = tomllib.loads((root / path).read_text())
            if data.get("patch") or data.get("replace"):
                errors.append(f"{path}: [patch] and [replace] are forbidden in the product tree")

            def walk(node):
                if not isinstance(node, dict):
                    return
                for key, value in node.items():
                    # Only a string renames a dependency; [workspace.package] is a table of package fields.
                    name = value.get("package", key) if isinstance(value, dict) else key
                    if isinstance(value, dict) and gpui(name if isinstance(name, str) else key):
                        if value.get("workspace") is not True and (
                            value.get("git") != fork["url"] or value.get("rev") != fork["rev"]
                            or any(k in value for k in ("path", "branch", "tag"))
                        ):
                            errors.append(f"{path}: GPUI declaration {key} does not use the registered pin")
                    walk(value)
            walk(data)
        if path.name in ("config", "config.toml") and ".cargo" in path.parts:
            data = tomllib.loads((root / path).read_text())
            if data.get("source") or data.get("paths") or data.get("patch"):
                errors.append(f"{path}: vendored/source/path overrides are forbidden")
        if path.suffix == ".rs" and any(part.lower() in ("vendor", "vendored", "gpui", "gpui-omarchy") for part in path.parts):
            errors.append(f"{path}: vendored GPUI source is forbidden")
        if (root / path).is_symlink() and (path.parts[0] in ("crates", ".cargo") or path.name.startswith("Cargo.")):
            errors.append(f"{path}: native source and manifest symlinks are forbidden")
    return sorted(set(errors))


def native_sources(root, paths, metadata):
    units = []
    members = set(metadata.get("workspace_members", []))
    origin = Path(metadata["workspace_root"])
    roots = []
    target_roots = set()
    for package in metadata["packages"]:
        if package["id"] not in members or package["name"] == "flea":
            continue
        relative = Path(package["manifest_path"]).relative_to(origin).parent
        if relative.parts[:1] != ("crates",):
            raise ValueError(f"native package outside crates/: {relative}")
        roots.append((relative, package["name"].replace("-", "_")))
        if not package.get("targets"):
            raise ValueError(f"native package resolves no target source paths: {package['name']}")
        # Cargo kind distinguishes test/bench roots from product targets, including libraries with test=true.
        for target in package["targets"]:
            path = str(Path(target["src_path"]).relative_to(origin))
            target_roots.add((path, target["kind"] in (["test"], ["bench"])))
    from .lex import tokens, use_aliases
    from .rust import functions
    for path in paths:
        for base, name in roots:
            if path.suffix != ".rs" or not path.is_relative_to(base):
                continue
            if (root / path).is_symlink() or not (root / path).resolve().is_relative_to(root.resolve()):
                raise ValueError(f"{path}: native source must be a file inside the product tree")
            source_base = base / "src" if path.is_relative_to(base / "src") else base
            parts = list(path.relative_to(source_base).with_suffix("").parts)
            if parts[-1] in ("lib", "main", "mod"):
                parts.pop()
            module = "::".join([name] + parts)
            ts = tokens((root / path).read_text())
            found, elements, file_mods = functions(ts, module, str(path))
            units.append({"path": str(path), "module": module, "tokens": ts, "functions": found,
                          "elements": elements, "aliases": use_aliases(ts), "file_mods": file_mods})
    # Only files reached exclusively through test declarations are excluded; a product alias keeps shared code visible.
    by_path = {unit["path"]: unit for unit in units}
    edges = {}

    def children(path, owns_directory):
        key = path, owns_directory
        if key in edges:
            return edges[key]
        unit = by_path[path]
        declarations = (functions(unit["tokens"], unit["module"], path, owns_directory=True)[2]
                        if owns_directory else unit["file_mods"])
        edges[key] = []
        for choices, test, child_owns_directory in declarations:
            for choice in choices:
                target = os.path.normpath(choice)
                if target in by_path:
                    edges[key].append((target, child_owns_directory, test))
        return edges[key]

    pending = [(path, True, test) for path, test in target_roots if path in by_path]
    reached, contexts = set(), {}
    while pending:
        path, owns_directory, test = pending.pop()
        if (path, owns_directory, test) in reached:
            continue
        reached.add((path, owns_directory, test))
        contexts.setdefault(path, set()).add(test)
        pending.extend((target, child_owns_directory, test or child_test)
                       for target, child_owns_directory, child_test in children(path, owns_directory))
    for unit in units:
        test_only = contexts.get(unit["path"]) == {True}
        unit.pop("file_mods")
        if test_only:
            unit["elements"] = set()
        for function in unit["functions"]:
            function.test = function.test or test_only
    return units
