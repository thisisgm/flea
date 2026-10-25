#!/usr/bin/env python3
"""Exercise the actual fleet CLI with complete captured fixtures and independent negative controls."""
import copy
import hashlib
import io
import json
from pathlib import Path
import subprocess
import sys
import tarfile


ROOT = Path(sys.argv[1])
assert ROOT.is_absolute() and (ROOT / ".flea-test-sandbox").is_file()
assert ROOT.parent == Path.cwd() / ".superpowers/m9-bench-05"
TOOL = Path.cwd() / "tools/flea-bench-fleet"
NOW = "2026-10-24T12:00:00Z"
ALIASES = ("flea-candidate", "flea-0.3.12", "strata-stable", "strata-nightly", "dolphin", "nautilus",
           "nemo", "thunar", "krusader", "pcmanfm", "yazi", "mc", "broot", "nnn", "lf", "ranger",
           "xplr", "superfile", "fsearch-nd", "fff-bare", "fff-rich", "fsearch")
count = 0


def path(name):
    result = ROOT / name
    assert result.is_absolute() and result.is_relative_to(ROOT)
    return result


def write(name, data):
    target = path(name)
    target.write_bytes(data)
    return target


def identity(target):
    data = target.read_bytes()
    return {"path": str(target), "sha256": hashlib.sha256(data).hexdigest(), "bytes": len(data)}


def command(*args):
    return subprocess.run(args, capture_output=True, check=True)


command("openssl", "genpkey", "-algorithm", "ED25519", "-out", str(path("key.pem")))
command("openssl", "pkey", "-in", str(path("key.pem")), "-pubout", "-outform", "DER", "-out", str(path("key.der")))
evidence = identity(write("upstream.json", b'{"saved_primary_response":"test fixture, not live provenance"}\n'))


def artifact(row):
    alias = row["alias"]
    target = "x86_64-unknown-linux-gnu"
    archive_name = f"strata-{row['version']}-{target}.tar.gz"
    archive_path = path(alias + ".tar.gz")
    with tarfile.open(archive_path, "w:gz") as archive:
        for name, data in (("bundle/strata", Path(row["binary"]["path"]).read_bytes()),
                           ("bundle/SOURCE_COMMIT", (row["source"]["build_commit"] + "\n").encode())):
            member = tarfile.TarInfo(name)
            member.size = len(data)
            archive.addfile(member, io.BytesIO(data))
    archive_id = identity(archive_path)
    manifest = {"schema": 1, "repository": "lgse/strata", "tag": row["source"]["tag"],
                "source_commit": row["source"]["build_commit"], "artifacts": [
                    {"name": archive_name, "target": target, "sha256": archive_id["sha256"], "size": archive_id["bytes"]}]}
    manifest_bytes = json.dumps(manifest).encode()
    manifest_path = write(alias + ".json", manifest_bytes)
    message = write(alias + ".message", b"strata-update-manifest-v1\0" + manifest_bytes)
    signature = path(alias + ".sig")
    command("openssl", "pkeyutl", "-sign", "-rawin", "-inkey", str(path("key.pem")), "-in", str(message), "-out", str(signature))
    return {"archive": archive_id, "manifest": identity(manifest_path), "signed_message": identity(message),
            "signature": identity(signature), "key_der": identity(path("key.der")), "target": target,
            "public_key": path("key.der").read_bytes()[-32:].hex(), "name": archive_name,
            "binary_member": "bundle/strata", "source_member": "bundle/SOURCE_COMMIT"}


def entrant(alias):
    kind = "candidate" if alias == "flea-candidate" else "anchor" if alias == "flea-0.3.12" else "stable"
    version = "0.4.0" if kind == "candidate" else "0.3.12" if kind == "anchor" else "1.1.0"
    if alias == "strata-nightly":
        kind, version = "nightly-build", "1.0.1-nightly.20261018"
    if alias == "fff-rich":
        kind = "anchor"
    binary = write(alias, ("#!/bin/sh\nexit 89\n# " + alias + "\n").encode())
    binary.chmod(0o700)
    commit = "f21789e56cc4b0877caa3156fff7bce52a8dd9c2" if kind == "anchor" else "a" * 40
    row = {"alias": alias, "profile": alias.removeprefix("fff-") if alias.startswith("fff-") else "default",
           "version": version, "kind": kind, "binary": identity(binary), "version_args": ["--version"],
           "version_stdout": alias + " " + version, "source": {"url": "https://example.org/source",
               "tag": "v" + version, "tag_object": "b" * 40, "tag_commit": "c" * 40,
               "build_commit": commit, "evidence": evidence},
           "package": {"name": alias, "version": version + "-1", "method": "system-package"},
           "dependencies": {"runtime-lib": "6.1-2", "thumbnailer": "4.0-1"},
           "thumbnailers": {"policy": "external", "packages": ["thumbnailer"]},
           "latest": {"channel": kind, "version": version, "checked_at": "2026-10-24T11:00:00Z",
                      "url": "https://example.org/releases", "evidence": evidence}}
    if alias.startswith("strata-"):
        row["package"]["method"] = "release-archive"
        row["artifact"] = artifact(row)
    return row


INVENTORY = {"schema": 1, "host": "fixture-host", "entrants": [entrant(alias) for alias in ALIASES]}
INVENTORY["installed_packages"] = {row["package"]["name"]: row["package"]["version"] for row in INVENTORY["entrants"]}
INVENTORY["installed_packages"].update({"runtime-lib": "6.1-2", "thumbnailer": "4.0-1"})
CAPTURE = copy.deepcopy(INVENTORY)
CAPTURE["captured_at"] = "2026-10-24T11:30:00Z"
for row in CAPTURE["entrants"]:
    row["probe"] = {"argv": [row["binary"]["path"], "--version"], "exit": 0,
                    "stdout": row["version_stdout"] + "\n", "stderr": ""}


def run(label, inventory=None, capture=None, error=None, groups=None):
    global count
    inv = INVENTORY if inventory is None else inventory
    cap = CAPTURE if capture is None else capture
    write("inventory.json", json.dumps(inv).encode())
    write("capture.json", json.dumps(cap).encode())
    argv = [sys.executable, "-B", str(TOOL), str(path("inventory.json")), str(path("capture.json")), "--as-of", NOW]
    if groups:
        argv += ["--groups"] + groups
    result = subprocess.run(argv, capture_output=True, text=True)
    report = json.loads(result.stdout)
    assert result.returncode == (1 if error else 0), (label, report, result.stderr)
    assert report["benchmark"] == "UNMEASURED", label
    if error:
        assert report["status"] == "BLOCKED" and any(error in item for item in report["errors"]), (label, report)
    elif groups:
        assert report["status"] == "GROUP_IDENTITIES_VERIFIED_NOT_COMPLETE_FLEET", report
        assert not report["complete_fleet"] and report["validated_groups"] == groups, report
        assert len(report["entrants"]) == 6 and not report["errors"], report
    else:
        assert report["complete_fleet"], report
        assert len(report["entrants"]) == 22 and not report["errors"], report
        assert "krusader" in report["rosters"]["B1"] and "pcmanfm" in report["rosters"]["field"]
        assert {"krusader", "pcmanfm"} <= set(report["rosters"]["B7"])
        assert report["entrants"][2]["source"]["build_commit"] != report["entrants"][2]["source"]["tag_commit"]
    count += 1
    print("PASS bench-fleet-contract: " + label)


def change_both(index, field, value):
    inv, cap = copy.deepcopy(INVENTORY), copy.deepcopy(CAPTURE)
    inv["entrants"][index][field] = copy.deepcopy(value)
    cap["entrants"][index][field] = copy.deepcopy(value)
    return inv, cap


run("full roster, separate signed source and peeled tag, no entrant execution")
source = {**INVENTORY["entrants"][4]["source"], "tag": "release-1.1.0", "tag_version": "1.1.0"}
inv, cap = change_both(4, "source", source)
run("explicit upstream tag/version mapping for nonstandard tag names", inv, cap)
source = {**INVENTORY["entrants"][4]["source"], "tag": "v1.0.0", "tag_version": "1.1.0"}
inv, cap = change_both(4, "source", source)
run("non-Strata 1.0.0 tag cannot map to 1.1.0", inv, cap, "tag_version does not match tag")
search = {"flea-candidate", "flea-0.3.12", "fsearch-nd", "fff-bare", "fff-rich", "fsearch"}
inv, cap = copy.deepcopy(INVENTORY), copy.deepcopy(CAPTURE)
inv["entrants"] = [row for row in inv["entrants"] if row["alias"] in search]
cap["entrants"] = [row for row in cap["entrants"] if row["alias"] in search]
run("West search inventory without unrelated GUI installations", inv, cap, groups=["search"])
run("search group cannot claim complete fleet", inv, cap, "missing entrant krusader")
inv["entrants"] = [row for row in inv["entrants"] if row["alias"] != "fsearch-nd"]
cap["entrants"] = [row for row in cap["entrants"] if row["alias"] != "fsearch-nd"]
run("search group still requires fsearch-nd", inv, cap, "missing entrant fsearch-nd", ["search"])
for alias in ALIASES:
    inv, cap = copy.deepcopy(INVENTORY), copy.deepcopy(CAPTURE)
    inv["entrants"] = [row for row in inv["entrants"] if row["alias"] != alias]
    cap["entrants"] = [row for row in cap["entrants"] if row["alias"] != alias]
    run("missing " + alias, inv, cap, "missing entrant " + alias)

inv, cap = change_both(3, "binary", INVENTORY["entrants"][2]["binary"])
run("stable/nightly shared executable", inv, cap, "stable/nightly binary collision")
inv, cap = change_both(0, "binary", INVENTORY["entrants"][1]["binary"])
run("candidate cannot replace installed anchor executable", inv, cap, "candidate/anchor binary collision")
inv, cap = change_both(3, "kind", "stable")
run("nightly updater head is stable", inv, cap, "nightly updater head")
inv, cap = change_both(2, "source", {**INVENTORY["entrants"][2]["source"], "tag": "v1.0.0"})
run("historical 1.0.0 cannot become 1.1.0", inv, cap, "tag/build version mismatch")
inv, cap = change_both(2, "latest", {**INVENTORY["entrants"][2]["latest"], "checked_at": "2026-10-23T11:59:59Z"})
run("upstream timestamp older than 24 hours", inv, cap, "controller refresh required")
inv, cap = change_both(2, "latest", {**INVENTORY["entrants"][2]["latest"], "checked_at": "2026-10-24T11:45:00Z"})
run("upstream check after capture", inv, cap, "upstream check is after capture")
cap = copy.deepcopy(CAPTURE)
cap["captured_at"] = "2026-10-23T11:59:59Z"
run("stale installed capture", capture=cap, error="controller refresh required")
cap["captured_at"] = "2026-10-24T13:00:00Z"
run("future installed capture", capture=cap, error="future timestamp")
cap = copy.deepcopy(CAPTURE)
cap["installed_packages"]["runtime-lib"] = "6.2-1"
run("complete dependency package inventory changed", capture=cap, error="changed installed package inventory")
cap = copy.deepcopy(CAPTURE)
cap["errors"] = ["native version capture incomplete"]
run("partial failed native capture", capture=cap, error="capture failed")
cap = copy.deepcopy(CAPTURE)
cap["entrants"][2]["artifact"]["archive"]["sha256"] = "f" * 64
run("captured signed artifact drift", capture=cap, error="changed artifact")
for field, value in (("dependencies", {"runtime-lib": "6.2-1", "thumbnailer": "4.0-1"}),
                     ("dependencies", {"runtime-lib": "6.1-2", "thumbnailer": "4.1-1"}),
                     ("package", {"name": "dolphin", "version": "new", "method": "system-package"})):
    cap = copy.deepcopy(CAPTURE)
    cap["entrants"][4][field] = value
    run("changed observed " + field + " " + str(value), capture=cap, error="changed " + field)
inv, cap = change_both(4, "thumbnailers", {"policy": "external", "packages": ["unversioned"]})
run("missing thumbnailer version", inv, cap, "thumbnailer version missing")
cap = copy.deepcopy(CAPTURE)
cap["entrants"][4]["probe"]["stdout"] = "dolphin 1.0.0\n"
run("actual version output mismatch", capture=cap, error="actual version output mismatch")
cap["entrants"][4]["probe"]["exit"] = 127
run("failed version probe cannot pass", capture=cap, error="version command failed")
cap = copy.deepcopy(CAPTURE)
cap["entrants"][4]["probe"]["stdout"] = 7
run("non-text version output is a BLOCKED report", capture=cap, error="missing text")
binary = path("dolphin")
original = binary.read_bytes()
write("dolphin", original + b"changed\n")
run("actual binary changed after capture", error="file identity changed")
write("dolphin", original)
binary.chmod(0o600)
run("binary lacks executable permission", error="binary is not executable")
binary.chmod(0o700)

signature = path("strata-stable.sig")
original = signature.read_bytes()
write(signature.name, bytes([original[0] ^ 1]) + original[1:])
bad_artifact = {**INVENTORY["entrants"][2]["artifact"], "signature": identity(signature)}
inv, cap = change_both(2, "artifact", bad_artifact)
run("invalid Ed25519 signature despite matching file hash", inv, cap, "signature mismatch")
write(signature.name, original)
archive = path("strata-stable.tar.gz")
original = archive.read_bytes()
write(archive.name, original + b"changed")
bad_artifact = {**INVENTORY["entrants"][2]["artifact"], "archive": identity(archive)}
inv, cap = change_both(2, "artifact", bad_artifact)
run("archive hash disagrees with signed manifest", inv, cap, "signed archive mismatch")
write(archive.name, original)
source = {**INVENTORY["entrants"][2]["source"], "build_commit": "d" * 40}
inv, cap = change_both(2, "source", source)
run("signed build source differs", inv, cap, "signed build source mismatch")
inv, cap = change_both(20, "profile", "bare")
run("fff rich cannot be relabelled bare", inv, cap, "fff bare/rich profile mismatch")
inv, cap = change_both(1, "kind", "stable")
run("installed anchor cannot be relabelled latest stable", inv, cap, "installed 0.3.12 anchor mismatch")
inv, cap = change_both(4, "kind", "anchor")
run("historical rival cannot fill latest fleet", inv, cap, "latest rival cannot be relabelled")
cap = copy.deepcopy(CAPTURE)
cap["entrants"].append(cap["entrants"][0])
run("duplicate alias", capture=cap, error="duplicate alias")
run("restored evidence passes after negative controls")
print(f"PASS bench-fleet-contract: {count} controls, 0 failures; native inventory and benchmarks remain unmeasured")
