"""Product identities shared by the standing graphical benchmark instruments."""
import hashlib
import json
import os
import platform
import re
import subprocess
import shutil
from pathlib import Path

# A packaged build compiles every bench seam out; each marker is a literal only its feature compiles in.
SEAM_MARKERS = {'bench-seam': ('flea-gui', b'FLEA_GUI_BENCH_VIEWPORT'),
                'flash-test-seam': ('flea', b'--loop-root')}
# The helpers a launch execs beside the binary, under /usr/lib/flea when packaged and tools/ in a source tree.
HELPERS = ('flea-gio-auth', 'flea-udisks', 'flea-portal', 'flea-filemanager1', 'flea-mpris')
UI_ENTRY = 'boot/shell.qml'
# The packaged interface, which the product prefers to the tree's for any binary; the override is for tests only.
PACKAGED_UI = os.environ.get('FLEA_BENCH_PACKAGED_UI') or '/usr/share/flea/ui'
FORK = 'gpui-omarchy'
# Fields two runs must share before an old row can stand beside a new one; anything else is context.
ELIGIBILITY_KEYS = ('host', 'fixture kind', 'fixture payload', 'TUI terminal', 'cpu_s', 'settle gap',
                    'gpu', 'display', 'theme', 'fonts', 'decoders', 'thumbnailers', 'storage', 'cache',
                    'accounting')


def archive_identity(receipt_path, archive_id, binary):
    """Match the executed archive to B5's version and extraction receipt."""
    path = Path(receipt_path).resolve(strict=True)
    receipt = json.loads(path.read_text())
    matches = [asset for asset in receipt['archives'] if asset['id'] == archive_id]
    if len(matches) != 1:
        raise ValueError('receipt must name exactly one archive: ' + archive_id)
    asset = matches[0]
    for field in ('tag', 'url', 'sha256', 'expected_version', 'binary_path', 'binary_sha256',
                  'source_commit', 'tag_commit', 'version_check'):
        if field not in asset:
            raise ValueError('archive receipt missing ' + field)
    if not isinstance(asset['version_check'], dict):
        raise ValueError('invalid archive receipt version_check')
    for field in ('source_commit', 'tag_commit'):
        value = asset[field]
        if not isinstance(value, str) or len(value) != 40 or any(c not in '0123456789abcdef' for c in value):
            raise ValueError('invalid archive source identity: ' + field)
    if not asset['tag'] or not asset['url'].startswith('https://'):
        raise ValueError('archive requires an actual ref and artifact URL')
    for field in ('sha256', 'binary_sha256'):
        digest = asset[field]
        if len(digest) != 64 or any(c not in '0123456789abcdef' for c in digest):
            raise ValueError('invalid archive receipt ' + field)
    channels = {item['id']: item for item in receipt['archives'] if item['id'] in ('strata-stable', 'strata-nightly')}
    if archive_id.startswith('strata'):
        if set(channels) != {'strata-stable', 'strata-nightly'} or any(sum(item['id'] == key for item in receipt['archives']) != 1 for key in channels):
            raise ValueError('both stable and nightly archive identities are mandatory')
        stable, nightly = channels['strata-stable'], channels['strata-nightly']
        for field in ('tag', 'url', 'expected_version', 'binary_sha256'):
            if not stable.get(field) or stable.get(field) == nightly.get(field):
                raise ValueError('stable and nightly require distinct ' + field)
        if Path(stable['binary_path']).resolve() == Path(nightly['binary_path']).resolve():
            raise ValueError('stable and nightly cannot alias the same executable')
        if 'nightly' in stable['expected_version'].lower() or 'nightly' not in nightly['expected_version'].lower():
            raise ValueError('stable and nightly version channels differ from their IDs')
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


def sha256_file(path):
    with open(path, 'rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def tree_digest(root):
    """Every regular file under root, by relative path and sha256, folded into one digest."""
    root = Path(root)
    rows = sorted((str(path.relative_to(root)), sha256_file(path))
                  for path in root.rglob('*') if path.is_file())
    if not rows:
        raise RuntimeError('resource tree is empty: ' + str(root))
    folded = hashlib.sha256(''.join('%s\0%s\n' % row for row in rows).encode()).hexdigest()
    return dict(path=str(root.resolve()), files=len(rows), sha256=folded)


def resolve_ui(binary, ui=None):
    """The interface directory the launch reads, in src/paths.rs ui_dir order: FLEA_UI, the package, the tree."""
    prefix = Path(binary).resolve().parent.parent
    for candidate in (ui, PACKAGED_UI, prefix.parent / 'ui'):
        if candidate and (Path(candidate) / UI_ENTRY).is_file():
            return Path(candidate)
    return None


def helper_dir(binary):
    prefix = Path(binary).resolve().parent.parent
    return prefix / 'lib/flea' if prefix.name == 'usr' else prefix.parent / 'tools'


def compiler(data):
    found = sorted(set(m.decode() for m in re.findall(rb'rustc version [0-9][ -~]{0,80}', data)))
    return found or ['not embedded']


def executed_fork_pins(data):
    return sorted(set(m.decode() for m in re.findall(rb'gpui-omarchy-[0-9a-f]{16}/([0-9a-f]{7,40})/', data)))


def locked_fork_pins(repo):
    lock = Path(repo) / 'Cargo.lock'
    text = lock.read_text()
    pins = sorted(set(re.findall(r'git\+https://github\.com/thisisgm/gpui-omarchy\?rev=([0-9a-f]{40})#', text)))
    if len(pins) != 1:
        raise ValueError('Cargo.lock must pin exactly one %s commit: %r' % (FORK, pins))
    return pins[0]


def source_identity(repo):
    commit = output(['git', '-C', str(repo), 'rev-parse', 'HEAD'])
    status = subprocess.run(['git', '-C', str(repo), 'status', '--porcelain', '--', 'src', 'crates', 'ui',
                             'tools', 'packaging', 'Cargo.toml', 'Cargo.lock', 'PKGBUILD'],
                            capture_output=True, text=True, timeout=10)
    if status.returncode:
        raise RuntimeError('source tree unreadable: ' + str(repo))
    return dict(commit=commit, state='DIRTY' if status.stdout.strip() else 'clean')


def package_owner(binary):
    try:
        return package_version(binary)
    except (RuntimeError, OSError, subprocess.TimeoutExpired):
        return 'unpackaged'


def build_identity(binary, front, repo=None, ui=None, seams_allowed=False):
    """Bind one Flea build: launcher, sibling, resources, package, source, compiler, seams, fork pin."""
    if front not in ('gpui', 'qml'):
        raise ValueError('frontend must be gpui or qml: %r' % front)
    launcher = Path(binary).resolve(strict=True)
    if not os.access(launcher, os.X_OK):
        raise RuntimeError('launcher is not executable: ' + str(launcher))
    data = {'flea': launcher.read_bytes()}
    result = dict(frontend=front, launcher=dict(path=str(launcher), sha256=sha256_file(launcher),
                  version=output([str(launcher), '--version']), package=package_owner(launcher)))
    gui = launcher.with_name('flea-gui')
    if gui.is_file() and os.access(gui, os.X_OK):
        data['flea-gui'] = gui.read_bytes()
        result['sibling'] = dict(path=str(gui), sha256=sha256_file(gui), package=package_owner(gui))
    elif front == 'gpui':
        raise RuntimeError('GPUI benchmark needs the sibling product binary: ' + str(gui))
    else:
        result['sibling'] = 'absent'
    found = resolve_ui(launcher, ui)
    if found:
        result['ui'] = tree_digest(found)
    elif front == 'qml':
        raise RuntimeError('QML benchmark needs its interface files: no %s for %s' % (UI_ENTRY, launcher))
    else:
        result['ui'] = 'absent'
    helpers = helper_dir(launcher)
    result['helpers'] = {name: sha256_file(helpers / name) if (helpers / name).is_file() else 'absent'
                         for name in HELPERS}
    result['compiler'] = compiler(b''.join(data.values()))
    result['seams'] = sorted(name for name, (part, marker) in SEAM_MARKERS.items()
                             if marker in data.get(part, b''))
    if result['seams'] and not seams_allowed:
        raise RuntimeError('product build carries bench seams: ' + ', '.join(result['seams']))
    executed = executed_fork_pins(data.get('flea-gui', b''))
    result['fork_pin'] = dict(executed=executed or 'not embedded')
    if repo:
        tree = os.path.realpath(repo)
        inside = os.path.commonpath([tree, str(launcher)]) == tree
        result['source'] = source_identity(tree) if inside else dict(commit='unknown', state='unknown')
        if front == 'gpui':
            locked = locked_fork_pins(repo)
            result['fork_pin']['locked'] = locked
            if executed and any(not locked.startswith(pin) for pin in executed):
                raise ValueError('executed %s %r differs from the locked pin %s' % (FORK, executed, locked))
    return result


def manifest_fields(path):
    """The '- key: value' lines of a manifest's opening sections, keyed by name."""
    fields = {}
    for line in Path(path).read_text(errors='replace').splitlines():
        if line.startswith('## Run close'):
            break
        match = re.match(r'- ([^:]+): (.*)$', line)
        if match and match.group(1) not in fields:
            fields[match.group(1)] = match.group(2)
    return fields


def row_eligibility(old_manifest, new_manifest, row_id, old_version=None, new_version=None):
    """Whether one historical row may stand beside the current run; every refusal names its field."""
    old, new = manifest_fields(old_manifest), manifest_fields(new_manifest)
    refusals = []
    for key in ELIGIBILITY_KEYS:
        if old.get(key, 'not recorded') != new.get(key, 'not recorded'):
            refusals.append('%s: %r against %r' % (key, old.get(key, 'not recorded'), new.get(key, 'not recorded')))
    # The manifest line is '<front end>; profile: <profile>'; the front end binds only Flea's own rows.
    old_front, _, old_profile = old.get('front end', 'not recorded').partition('; profile: ')
    new_front, _, new_profile = new.get('front end', 'not recorded').partition('; profile: ')
    if old_profile != new_profile:
        refusals.append('profile: %r against %r' % (old_profile or 'not recorded', new_profile or 'not recorded'))
    if row_id.startswith('flea') and old_front != new_front:
        refusals.append('front end: %r against %r' % (old_front, new_front))
    if 'complete run cgroup' not in old.get('cpu_s', ''):
        refusals.append('cpu_s: the historical row is main-process CPU, not whole-cgroup CPU')
    if old_version != new_version:
        refusals.append('version: %r against %r; an old row is never relabelled' % (old_version, new_version))
    return dict(id=row_id, eligible=not refusals, refusals=refusals)


def frontend(arm):
    value = arm.get("frontend", os.environ.get("FLEA_FRONTEND", "gpui"))
    if value not in ("gpui", "qml"):
        raise ValueError("frontend must be gpui or qml: %r" % value)
    return value


def flea_binary(arm):
    return arm.get("flea_bin") or os.path.join(arm["flea_dir"], "target/release/flea")


def window_class(arm):
    frontend(arm)
    return "com.thisisgm.flea"


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
        # The field tools launch an arm with FLEA_UI=<flea_dir>/ui, so the identity reads the same tree first.
        ui = arm.get("ui") or (os.path.join(arm["flea_dir"], "ui") if arm.get("flea_dir") else None)
        result["build"] = build_identity(binary, frontend(arm), arm.get("flea_dir"), ui)
        if arm.get("flea_dir"):
            tree = os.path.realpath(arm["flea_dir"])
            inside = os.path.commonpath([tree, os.path.realpath(binary)]) == tree
            result["build_sha"] = output(["git", "-C", tree, "rev-parse", "HEAD"]) if inside else "unknown"
        if frontend(arm) == "gpui":
            result["gui_sha256"] = result["build"]["sibling"]["sha256"]
    return result


def main_build(argv):
    import argparse
    parser = argparse.ArgumentParser(prog='flea_bench_product.py build')
    parser.add_argument('binary')
    parser.add_argument('frontend')
    parser.add_argument('--repo')
    parser.add_argument('--ui')
    parser.add_argument('--seams-allowed', action='store_true')
    args = parser.parse_args(argv)
    print(json.dumps(build_identity(args.binary, args.frontend, args.repo, args.ui, args.seams_allowed),
                     sort_keys=True))


def main_eligible(argv):
    import argparse
    parser = argparse.ArgumentParser(prog='flea_bench_product.py eligible')
    parser.add_argument('old_manifest')
    parser.add_argument('new_manifest')
    parser.add_argument('row_id')
    parser.add_argument('old_version')
    parser.add_argument('new_version')
    args = parser.parse_args(argv)
    verdict = row_eligibility(args.old_manifest, args.new_manifest, args.row_id, args.old_version, args.new_version)
    print(json.dumps(verdict, sort_keys=True))
    return 0 if verdict['eligible'] else 1


if __name__ == '__main__':
    import argparse
    import sys
    if sys.argv[1:2] == ['build']:
        sys.exit(main_build(sys.argv[2:]))
    if sys.argv[1:2] == ['eligible']:
        sys.exit(main_eligible(sys.argv[2:]))
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('receipt')
    parser.add_argument('archive_id')
    parser.add_argument('binary')
    parser.add_argument('--row', metavar='KIND')
    args = parser.parse_args()
    checked = archive_identity(args.receipt, args.archive_id, args.binary)
    if args.row:
        name, _, version = checked['version'].partition(' ')
        version = version or name
        if ' ' in version:
            raise ValueError('archive version is not "name version" or one word: ' + checked['version'])
        print('%s %s archive %s %s' % (args.archive_id, args.row, version, json.dumps(checked, sort_keys=True)))
    else:
        print(json.dumps(checked, sort_keys=True))
