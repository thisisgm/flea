"""Product identities shared by the standing graphical benchmark instruments."""
import hashlib
import json
import os
import platform
import subprocess
import shutil
from pathlib import Path


def archive_identity(receipt_path, archive_id, binary):
    """Match the executed archive to B5's version and extraction receipt."""
    path = Path(receipt_path).resolve(strict=True)
    receipt = json.loads(path.read_text())
    matches = [asset for asset in receipt['archives'] if asset['id'] == archive_id]
    if len(matches) != 1:
        raise ValueError('receipt must name exactly one archive: ' + archive_id)
    asset = matches[0]
    for field in ('tag', 'url', 'sha256', 'expected_version', 'binary_path', 'binary_sha256',
                  'source_commit', 'tag_commit'):
        if field not in asset:
            raise ValueError('archive receipt missing ' + field)
    for field in ('sha256', 'binary_sha256'):
        digest = asset[field]
        if len(digest) != 64 or any(c not in '0123456789abcdef' for c in digest):
            raise ValueError('invalid archive receipt ' + field)
    binary = Path(binary).resolve(strict=True)
    if binary != Path(asset['binary_path']).resolve(strict=True):
        raise ValueError('launched binary differs from archive receipt: ' + str(binary))
    with binary.open('rb') as stream:
        digest = hashlib.file_digest(stream, 'sha256').hexdigest()
    version = output([str(binary), '--version'])
    checked = asset['version_check']
    if (digest != asset['binary_sha256'] or version != asset['expected_version']
            or checked.get('rc') != 0 or checked.get('stdout') != version):
        raise ValueError('archive binary hash or version differs from receipt: ' + archive_id)
    return dict(binary=str(binary), version=version, sha256=digest, archive=asset,
                rival_manifest=dict(path=str(path), sha256=hashlib.sha256(path.read_bytes()).hexdigest(),
                                    receipt=receipt))


def frontend(arm):
    value = arm.get("frontend", os.environ.get("FLEA_FRONTEND", "qml"))
    if value not in ("gpui", "qml"):
        raise ValueError("frontend must be gpui or qml: %r" % value)
    return value


def flea_binary(arm):
    return arm.get("flea_bin") or os.path.join(arm["flea_dir"], "target/release/flea")


def window_class(arm):
    return "flea" if frontend(arm) == "gpui" else "com.thisisgm.flea"


def output(argv):
    result = subprocess.run(argv, capture_output=True, text=True, timeout=10,
                            env=dict(os.environ, LC_ALL="C.UTF-8", LANGUAGE=""))
    if result.returncode or not result.stdout.strip():
        raise RuntimeError("identity command failed: %r: %s" % (argv, result.stderr.strip()))
    return result.stdout.strip()


def package_version(binary):
    """The owning package's name and version, the source rival-versions.json records."""
    path = os.path.realpath(binary)
    owner = output(["pacman", "-Qqo", path]).splitlines()[0]
    return output(["pacman", "-Q", owner]).splitlines()[0]


def product_version(binary, version_command):
    """Ask the binary first; a binary with no working --version falls back to its package."""
    try:
        return output(version_command).splitlines()[0], "--version"
    except (RuntimeError, OSError, subprocess.TimeoutExpired) as direct:
        try:
            return package_version(binary), "pacman"
        except (RuntimeError, OSError, subprocess.TimeoutExpired) as packaged:
            raise RuntimeError("no version for %s: %s; package fallback: %s" % (binary, direct, packaged))


def identity(arm, argv):
    binary = os.path.realpath(argv[0]) if os.path.isabs(argv[0]) else argv[0]
    binary = shutil.which(binary) or binary
    version_command = [binary, "--version"]
    if arm["entrant"] == "broot":
        version_command = ["broot", "--version"]
    elif arm["entrant"] == "yazi-z":
        version_command = ["yazi", "--version"]
    binary = shutil.which(version_command[0]) or version_command[0]
    version, source = product_version(binary, version_command)
    result = {"binary": binary, "version": version, "version_source": source,
              "kernel": platform.release()}
    if arm.get("channel"):
        result["channel"] = arm["channel"]
    with open(binary, "rb") as stream:
        result["sha256"] = hashlib.file_digest(stream, "sha256").hexdigest()
    receipt = arm.get('rival_manifest') or os.environ.get('B5_RIVAL_MANIFEST')
    archive_id = arm.get('archive_id')
    if receipt and arm['entrant'] == 'strata':
        archive_id = archive_id or os.environ.get('STRATA_ARCHIVE_ID', 'strata-stable')
    if archive_id:
        if not receipt:
            raise ValueError('archive arm requires rival_manifest: ' + archive_id)
        result.update(archive_identity(receipt, archive_id, binary))
    if arm["entrant"] == "flea":
        result["frontend"] = frontend(arm)
        if arm.get("flea_dir"):
            result["build_sha"] = output(["git", "-C", arm["flea_dir"], "rev-parse", "HEAD"])
        if frontend(arm) == "gpui":
            gui = os.path.join(os.path.dirname(binary), "flea-gui")
            if not os.access(gui, os.X_OK):
                raise RuntimeError("GPUI benchmark needs the sibling product binary: " + gui)
            with open(gui, "rb") as stream:
                result["gui_sha256"] = hashlib.file_digest(stream, "sha256").hexdigest()
    return result


if __name__ == '__main__':
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('receipt')
    parser.add_argument('archive_id')
    parser.add_argument('binary')
    parser.add_argument('--row', metavar='KIND')
    args = parser.parse_args()
    checked = archive_identity(args.receipt, args.archive_id, args.binary)
    if args.row:
        print('%s %s archive %s %s' % (args.archive_id, args.row,
              checked['version'].split(' ', 1)[1], json.dumps(checked, sort_keys=True)))
    else:
        print(json.dumps(checked, sort_keys=True))
