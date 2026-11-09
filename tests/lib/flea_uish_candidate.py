"""Assemble an exact-base, allowlisted atomic proof without changing the source tree."""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import stat
import subprocess
import sys

BASE = "2faf8d099e12303cfb5a28af4a0c16c851a2cb07"
POLICIES = {'m9-uish-01': {'leaves': ['tests/ui.sh',
                           'tests/run-all.sh',
                           'tests/ui-uish/manifest.sh',
                           'tests/lib/flea_uish_candidate.py',
                           'tests/lib/flea_uish_inventory.py',
                           'tests/ui-uish-inventory.sh',
                           'tests/ui-uish/launch.sh',
                           'tests/ui-uish/state.sh',
                           'tests/ui-uish/sizes.sh',
                           'tests/ui-uish/inline-rename.sh',
                           'tests/ui-uish/modal-routing.sh',
                           'tests/ui-uish/places.sh',
                           'tests/ui-uish/network-form.sh',
                           'tests/ui-uish/network-service.sh',
                           'tests/ui-uish/devices.sh',
                           'tests/ui-uish/providers.sh',
                           'tests/ui-uish/thumbnails.sh',
                           'tests/ui-uish/quick-look.sh',
                           'tests/ui-uish/documents.sh',
                           'tests/ui-uish/media.sh',
                           'tests/ui-uish/preview-swap.sh',
                           'tests/ui-uish/tabs.sh',
                           'tests/ui-uish/operations.sh',
                           'tests/ui-uish/open-with.sh',
                           'tests/ui-uish/drag.sh'],
                'joins': []},
 'm9-uish-02': {'leaves': ['crates/flea-gui/tests/uish_driver.rs',
                           'tests/lib/flea_uish_headless.py',
                           'tests/ui-gpui-headless.sh',
                           'tests/uish-headless-controls.py'],
                'joins': []},
 'm9-uish-03': {'leaves': ['tests/lib/flea_uish_fixtures.py',
                           'tests/lib/flea_uish_receipts.py',
                           'tests/uish-fixture-controls.py',
                           'tests/ui-uish-fixtures.sh'],
                'joins': []},
 'm9-uish-04': {'leaves': ['crates/flea-gui/src/ipc/native_window.rs',
                           'crates/flea-gui/src/window/native_reader_tests.rs',
                           'tests/ui-uish/common-observers.sh',
                           'crates/flea-gui/tests/uish_04.rs',
                           'tests/ui-uish-common-observers.sh'],
                'joins': ['crates/flea-gui/src/window/events_ipc.rs']},
 'm9-uish-05': {'leaves': ['tests/ui-uish/launch.sh',
                           'tests/ui-renderer-gpui.py',
                           'crates/flea-gui/tests/uish_05.rs',
                           'tests/ui-uish-launch.sh'],
                'joins': []},
 'm9-uish-06': {'leaves': ['tests/ui-uish/state.sh',
                           'crates/flea-gui/src/ipc/settings.rs',
                           'crates/flea-gui/src/window/settings/observe.rs',
                           'crates/flea-gui/tests/uish_06.rs',
                           'tests/ui-uish-state.sh'],
                'joins': ['crates/flea-gui/src/window/mod.rs', 'crates/flea-gui/src/window/events_ipc.rs']},
 'm9-uish-07': {'leaves': ['tests/ui-uish/sizes.sh',
                           'crates/flea-gui/src/ipc/dir_sizes.rs',
                           'crates/flea-gui/src/pane/size_observe.rs',
                           'crates/flea-gui/tests/uish_07.rs',
                           'tests/ui-uish-sizes.sh'],
                'joins': ['crates/flea-gui/src/pane/mod.rs', 'crates/flea-gui/src/window/events_ipc.rs']},
 'm9-uish-08': {'leaves': ['tests/ui-uish/inline-rename.sh',
                           'crates/flea-gui/src/window/rename.rs',
                           'crates/flea-gui/src/ipc/rename.rs',
                           'crates/flea-gui/src/list/rename.rs',
                           'crates/flea-gui/tests/uish_08.rs',
                           'tests/ui-uish-inline-rename.sh'],
                'joins': ['crates/flea-gui/src/state.rs',
                          'crates/flea-gui/src/window/mod.rs',
                          'crates/flea-gui/src/window/input.rs',
                          'crates/flea-gui/src/window/render.rs',
                          'crates/flea-gui/src/window/events_ipc.rs',
                          'crates/flea-gui/src/input/pointer.rs']},
 'm9-uish-09': {'leaves': ['tests/ui-uish/modal-routing.sh',
                           'crates/flea-gui/src/g2/file_ops/ipc.rs',
                           'crates/flea-gui/src/ipc/modal.rs',
                           'crates/flea-gui/tests/uish_09.rs',
                           'tests/ui-uish-modal-routing.sh'],
                'joins': ['crates/flea-gui/src/window/events_ipc.rs']},
 'm9-uish-10': {'leaves': ['tests/ui-uish/places.sh',
                           'crates/flea-gui/src/window/recent.rs',
                           'crates/flea-gui/src/ipc/recent.rs',
                           'crates/flea-gui/src/g2/rail_places/producer_sources.rs',
                           'crates/flea-gui/tests/uish_10.rs',
                           'tests/ui-uish-places.sh'],
                'joins': ['crates/flea-gui/src/window/mod.rs',
                          'crates/flea-gui/src/window/rail.rs',
                          'crates/flea-gui/src/window/events_filesystem.rs',
                          'crates/flea-gui/src/window/events_ipc.rs']},
 'm9-uish-11': {'leaves': ['tests/ui-uish/network-form.sh',
                           'crates/flea-gui/src/g2/network_shares/form.rs',
                           'crates/flea-gui/src/g2/network_shares/readers.rs',
                           'crates/flea-gui/src/ipc/network_form.rs',
                           'crates/flea-gui/tests/uish_11.rs',
                           'tests/ui-uish-network-form.sh'],
                'joins': ['crates/flea-gui/src/window/events_ipc.rs']},
 'm9-uish-12': {'leaves': ['tests/ui-uish/network-service.sh',
                           'crates/flea-gui/src/g2/network_shares/**',
                           'crates/flea-gui/src/window/network.rs',
                           'crates/flea-gui/src/ipc/network.rs',
                           'crates/flea-gui/tests/uish_12.rs',
                           'tests/ui-uish-network-service.sh'],
                'joins': ['crates/flea-gui/src/g2/mod.rs',
                          'crates/flea-gui/src/state.rs',
                          'crates/flea-gui/src/window/mod.rs',
                          'crates/flea-gui/src/window/rail.rs',
                          'crates/flea-gui/src/window/events.rs',
                          'crates/flea-gui/src/window/events_ipc.rs',
                          'crates/flea-gui/src/window/root.rs']},
 'm9-uish-13': {'leaves': ['tests/ui-uish/devices.sh',
                           'crates/flea-gui/src/g2/device_rail/**',
                           'crates/flea-gui/src/window/devices.rs',
                           'crates/flea-gui/src/ipc/devices.rs',
                           'crates/flea-gui/tests/uish_13.rs',
                           'tests/ui-uish-devices.sh'],
                'joins': ['crates/flea-gui/src/state.rs',
                          'crates/flea-gui/src/window/mod.rs',
                          'crates/flea-gui/src/window/rail.rs',
                          'crates/flea-gui/src/window/events_ipc.rs']},
 'm9-uish-14': {'leaves': ['tests/ui-uish/providers.sh',
                           'crates/flea-gui/src/g2/send_share/**',
                           'crates/flea-gui/src/window/providers.rs',
                           'crates/flea-gui/src/ipc/providers.rs',
                           'crates/flea-gui/tests/uish_14.rs',
                           'tests/ui-uish-providers.sh'],
                'joins': ['crates/flea-gui/src/g2/mod.rs',
                          'crates/flea-gui/src/state.rs',
                          'crates/flea-gui/src/window/mod.rs',
                          'crates/flea-gui/src/window/root.rs',
                          'crates/flea-gui/src/window/events_ipc.rs']},
 'm9-uish-15': {'leaves': ['tests/ui-uish/thumbnails.sh',
                           'crates/flea-gui/src/ipc/thumbnails.rs',
                           'crates/flea-gui/src/row/thumbnail_observe.rs',
                           'crates/flea-gui/tests/uish_15.rs',
                           'tests/ui-uish-thumbnails.sh'],
                'joins': ['crates/flea-gui/src/window/events_ipc.rs']},
 'm9-uish-16': {'leaves': ['tests/ui-uish/quick-look.sh',
                           'crates/flea-gui/src/g2/quick_look_overlay/**',
                           'crates/flea-gui/src/window/preview.rs',
                           'crates/flea-gui/src/ipc/preview.rs',
                           'crates/flea-gui/tests/uish_16.rs',
                           'tests/ui-uish-quick-look.sh'],
                'joins': ['crates/flea-gui/src/g2/mod.rs',
                          'crates/flea-gui/src/state.rs',
                          'crates/flea-gui/src/window/mod.rs',
                          'crates/flea-gui/src/window/input.rs',
                          'crates/flea-gui/src/window/root.rs',
                          'crates/flea-gui/src/window/events_ipc.rs']},
 'm9-uish-17': {'leaves': ['tests/ui-uish/documents.sh',
                           'crates/flea-gui/src/g2/doc_preview/**',
                           'crates/flea-gui/src/columns/preview_host.rs',
                           'crates/flea-gui/src/ipc/preview_documents.rs',
                           'crates/flea-gui/tests/uish_17.rs',
                           'tests/ui-uish-documents.sh'],
                'joins': ['crates/flea-gui/src/window/events_ipc.rs']},
 'm9-uish-18': {'leaves': ['tests/ui-uish/media.sh',
                           'crates/flea-gui/src/g2/media_preview/**',
                           'crates/flea-gui/src/ipc/preview_media.rs',
                           'crates/flea-gui/tests/uish_18.rs',
                           'tests/ui-uish-media.sh'],
                'joins': ['crates/flea-gui/src/window/events_ipc.rs']},
 'm9-uish-19': {'leaves': ['tests/ui-uish/preview-swap.sh',
                           'crates/flea-gui/src/window/preview.rs',
                           'crates/flea-gui/src/ipc/preview.rs',
                           'crates/flea-gui/src/columns/preview_observe.rs',
                           'crates/flea-gui/tests/uish_19.rs',
                           'tests/ui-uish-preview-swap.sh'],
                'joins': ['crates/flea-gui/src/window/events_ipc.rs']},
 'm9-uish-20': {'leaves': ['tests/ui-uish/tabs.sh',
                           'crates/flea-gui/src/tabs/**',
                           'crates/flea-gui/src/ipc/tabs.rs',
                           'crates/flea-gui/src/window/tabs.rs',
                           'crates/flea-gui/tests/uish_20.rs',
                           'tests/ui-uish-tabs.sh'],
                'joins': ['crates/flea-gui/src/state.rs',
                          'crates/flea-gui/src/window/mod.rs',
                          'crates/flea-gui/src/window/input.rs',
                          'crates/flea-gui/src/window/render.rs',
                          'crates/flea-gui/src/window/events_ipc.rs']},
 'm9-uish-21': {'leaves': ['tests/ui-uish/operations.sh',
                           'crates/flea-gui/src/g2/file_ops/**',
                           'crates/flea-gui/src/g2/archive_flash/**',
                           'crates/flea-gui/src/g2/print_convert/**',
                           'crates/flea-gui/src/window/file_operations.rs',
                           'crates/flea-gui/tests/uish_21.rs',
                           'tests/ui-uish-operations.sh'],
                'joins': ['crates/flea-gui/src/g2/mod.rs',
                          'crates/flea-gui/src/state.rs',
                          'crates/flea-gui/src/window/mod.rs',
                          'crates/flea-gui/src/window/input.rs',
                          'crates/flea-gui/src/window/events.rs',
                          'crates/flea-gui/src/window/events_ipc.rs']},
 'm9-uish-22': {'leaves': ['tests/ui-uish/open-with.sh',
                           'crates/flea-gui/src/g2/open_with_properties/**',
                           'crates/flea-gui/src/ipc/file_dialogs.rs',
                           'crates/flea-gui/tests/uish_22.rs',
                           'tests/ui-uish-open-with.sh'],
                'joins': ['crates/flea-gui/src/window/mod.rs',
                          'crates/flea-gui/src/window/root.rs',
                          'crates/flea-gui/src/window/events.rs',
                          'crates/flea-gui/src/window/events_ipc.rs']},
 'm9-uish-23': {'leaves': ['tests/ui-uish/drag.sh',
                           'crates/flea-gui/src/input/platform_drag.rs',
                           'crates/flea-gui/src/window/dual_drag.rs',
                           'crates/flea-gui/src/ipc/drag.rs',
                           'crates/flea-gui/tests/uish_23.rs',
                           'tests/ui-uish-drag.sh'],
                'joins': ['crates/flea-gui/src/input/mod.rs',
                          'crates/flea-gui/src/input/pointer.rs',
                          'crates/flea-gui/src/window/mod.rs',
                          'crates/flea-gui/src/window/input.rs',
                          'crates/flea-gui/src/window/events_ipc.rs']},
 'm9-uish-24': {'leaves': ['tests/lib/flea_native_wallpaper.py',
                           'tests/lib/flea_native_envelope_controls.py',
                           'tests/ui-uish-native-admission.sh'],
                'joins': []}}
MARKER = ".flea-test-sandbox"


class CandidateError(ValueError):
    pass


def git(repo, *args, **kwargs):
    return subprocess.check_output(["git", "-C", str(repo), *args], **kwargs)


def path_name(value):
    path = PurePosixPath(value)
    if not value or path.is_absolute() or any(p in ("", ".", "..", ".git") for p in value.split("/")):
        raise CandidateError(f"unsafe source path: {value}")
    return str(path)


def allowed(path, patterns):
    path_name(path)
    return any(path.startswith(p[:-2]) if p.endswith("/**") else path == p for p in patterns)


def no_links(root, relative):
    path_name(relative)
    current = root
    for part in PurePosixPath(relative).parts:
        current = current / part
        if current.is_symlink():
            raise CandidateError(f"symlink in mutation path: {current}")
    return current


def validate_output(repo, unit, output):
    if not output.is_absolute() or ".." in output.parts:
        raise CandidateError("output must be absolute without traversal")
    parent = repo / ".superpowers" / unit
    if not output.is_relative_to(parent) or output == parent:
        raise CandidateError("output is outside the unit's marked scratch root")
    no_links(repo, str(output.relative_to(repo)))
    if output.exists():
        raise CandidateError("output already exists; refusing to overwrite a proof")
    return parent


def snapshot(repo, base):
    if base != BASE or git(repo, "rev-parse", "HEAD", text=True).strip() != BASE:
        raise CandidateError(f"mismatched base: expected {BASE}")
    records = []
    for line in git(repo, "ls-tree", "-rz", base).split(b"\0"):
        if not line:
            continue
        metadata, raw_path = line.split(b"\t", 1)
        mode, kind, blob = metadata.decode().split()
        path = path_name(os.fsdecode(raw_path))
        if kind != "blob" or mode not in ("100644", "100755", "120000"):
            raise CandidateError(f"unsupported Git entry: {path}")
        records.append((path, mode, blob))
    raw = subprocess.check_output(["git", "-C", str(repo), "cat-file", "--batch"],
                                  input="".join(blob + "\n" for _, _, blob in records).encode())
    entries = {}
    offset = 0
    for path, mode, blob in records:
        end = raw.index(b"\n", offset)
        found, kind, size = raw[offset:end].split()
        size = int(size)
        data = raw[end + 1:end + 1 + size]
        if found.decode() != blob or kind != b"blob" or len(data) != size:
            raise CandidateError("Git batch object mismatch")
        entries[path] = {"mode": mode, "data": data, "base_blob": blob,
                         "base_sha256": hashlib.sha256(data).hexdigest(), "origin": "base"}
        offset = end + size + 2
    return entries


def changes(repo, policy, entries):
    paths = git(repo, "diff", "--no-renames", "--name-only", "-z", "HEAD").split(b"\0")
    paths += git(repo, "ls-files", "--others", "--exclude-standard", "-z").split(b"\0")
    changed = sorted({os.fsdecode(p) for p in paths if p})
    for path in changed:
        if path.startswith(".superpowers/"):
            continue
        if not allowed(path, policy["leaves"]):
            raise CandidateError(f"unlisted leaf delta: {path}")
        source = repo / path
        # Read a link itself, while forbidding a linked ancestor.
        parent = str(PurePosixPath(path).parent)
        if parent != ".":
            no_links(repo, parent)
        previous = entries.get(path, {})
        if not source.exists() and not source.is_symlink():
            entries.pop(path, None)
            continue
        info = source.lstat()
        if stat.S_ISLNK(info.st_mode):
            mode, data = "120000", os.fsencode(os.readlink(source))
        elif stat.S_ISREG(info.st_mode):
            mode, data = ("100755" if info.st_mode & 0o111 else "100644"), source.read_bytes()
        else:
            raise CandidateError(f"leaf is not a regular file or symlink: {path}")
        entries[path] = {**previous, "mode": mode, "data": data, "origin": "leaf",
                         "leaf_sha256": hashlib.sha256(data).hexdigest()}
    return [p for p in changed if not p.startswith(".superpowers/")]


def join_targets(patch, policy, entries):
    if not patch.strip():
        return []
    text = patch.decode("utf-8")
    blocks = re.split(r"(?=^diff --git )", text, flags=re.M)
    if blocks[0]:
        raise CandidateError("join must be a plain full-index Git diff")
    targets = []
    for block in blocks[1:]:
        header = re.match(r"diff --git a/([^\n ]+) b/([^\n ]+)\n", block)
        if not header or header[1] != header[2]:
            raise CandidateError("join renames, quoted paths and copies are not supported")
        path = path_name(header[1])
        if path not in policy["joins"] or path in targets:
            raise CandidateError(f"unlisted or repeated join target: {path}")
        index = re.search(r"^index ([0-9a-f]{40})\.\.([0-9a-f]{40})(?: [0-9]+)?$", block, re.M)
        original = entries.get(path)
        if not index or index[1] != (original or {}).get("base_blob", "0" * 40):
            raise CandidateError(f"join lacks exact base blob identity: {path}")
        if original and original["origin"] != "base":
            raise CandidateError(f"join also replaces a leaf: {path}")
        if re.search(r"^(?:old mode|new mode|new file mode) 120000$", block, re.M):
            raise CandidateError("join may not introduce a symlink")
        for prefix, side in (("--- ", "a/"), ("+++ ", "b/")):
            headers = re.findall(r"^" + re.escape(prefix) + r"([^\n]+)$", block, re.M)
            if len(headers) != 1 or headers[0] not in (side + path, "/dev/null"):
                raise CandidateError(f"join filename header disagrees with target: {path}")
        if re.search(r"^(?:rename |copy |GIT binary patch)", block, re.M):
            raise CandidateError("join requires ordinary textual file hunks")
        targets.append(path)
    return targets


def owned_root(root):
    marker = root / MARKER
    if not root.is_absolute() or root.is_symlink() or not root.is_dir() or marker.is_symlink() or not marker.is_file():
        raise CandidateError("proof root lacks a regular ownership marker")
    if root.stat().st_uid != os.geteuid() or marker.stat().st_uid != os.geteuid():
        raise CandidateError("proof root belongs to another owner")


def write_entry(root, path, entry):
    owned_root(root)
    destination = no_links(root, path)
    destination.parent.mkdir(parents=True, exist_ok=True)
    if entry["mode"] == "120000":
        os.symlink(os.fsdecode(entry["data"]), destination)
    else:
        with destination.open("xb") as handle:
            handle.write(entry["data"])
        destination.chmod(int(entry["mode"], 8) & 0o777)


def assemble(repo, unit, output, join, base=BASE):
    if unit not in POLICIES:
        raise CandidateError(f"unknown unit: {unit}")
    parent = validate_output(repo, unit, output)
    entries = snapshot(repo, base)
    base_records = {p: {k: v for k, v in entry.items() if k != "data"} for p, entry in entries.items()}
    base_paths = set(entries)
    leaf_paths = changes(repo, POLICIES[unit], entries)
    patch = b""
    if join:
        if not join.is_absolute():
            join = repo / join
        if not join.is_relative_to(parent):
            raise CandidateError("join patch must live in this unit's scratch root")
        no_links(repo, str(join.relative_to(repo)))
        patch = join.read_bytes()
    targets = join_targets(patch, POLICIES[unit], entries)
    parent.mkdir(parents=True, exist_ok=True)
    output.mkdir(mode=0o700, parents=True)
    (output / MARKER).write_text(f"flea proof candidate {unit}\n")
    for path, entry in entries.items():
        write_entry(output, path, entry)
    if patch:
        owned_root(output)
        for target in targets:
            no_links(output, target)
        environment = dict(os.environ, GIT_CEILING_DIRECTORIES=str(output.parent))
        for name in ("GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_COMMON_DIR"):
            environment.pop(name, None)
        command = ["git", "apply", "--whitespace=error-all", "-"]
        subprocess.run(command[:2] + ["--check"] + command[2:], input=patch, cwd=output,
                       env=environment, check=True, capture_output=True)
        subprocess.run(command, input=patch, cwd=output, env=environment, check=True, capture_output=True)
        for path in targets:
            destination = no_links(output, path)
            if not destination.exists():
                entries.pop(path, None)
                continue
            before = entries.get(path, {})
            info = destination.stat()
            entries[path] = {**before, "data": destination.read_bytes(), "origin": "join",
                             "mode": "100755" if info.st_mode & 0o111 else "100644"}
    # Brokers need an independent Git root and the original base objects, without any commit.
    owned_root(output)
    git(output, "init", "--quiet")
    objects = git(repo, "rev-parse", "--path-format=absolute", "--git-path", "objects", text=True).strip()
    (output / ".git/objects/info/alternates").write_text(objects + "\n")
    git(output, "update-ref", "HEAD", BASE)
    git(output, "read-tree", BASE)
    records = {}
    for path, entry in sorted(entries.items()):
        records[path] = {k: v for k, v in entry.items() if k != "data"}
        records[path]["sha256"] = hashlib.sha256(entry["data"]).hexdigest()
    manifest = {"base": BASE, "unit": unit, "source_root": str(repo), "candidate": str(output),
                "leaf_paths": leaf_paths, "join_targets": targets,
                "join_sha256": hashlib.sha256(patch).hexdigest(),
                "deleted": {p: base_records[p] for p in sorted(base_paths - entries.keys())}, "sources": records}
    evidence = output / ".superpowers" / unit
    evidence.mkdir(parents=True, exist_ok=True)
    (evidence / "SOURCE-MANIFEST.json").write_text(json.dumps(manifest, indent=2) + "\n")
    return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--unit", required=True)
    parser.add_argument("--repo", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--base", default=BASE)
    parser.add_argument("--join", type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    try:
        manifest = assemble(args.repo.resolve(), args.unit, args.output, args.join, args.base)
        print(json.dumps({"candidate": manifest["candidate"], "sources": len(manifest["sources"]),
                          "leaf_paths": manifest["leaf_paths"], "join_targets": manifest["join_targets"]}))
        return 0
    except (CandidateError, OSError, UnicodeError, subprocess.CalledProcessError) as error:
        print(f"REFUSED: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
