"""Replay genuine external providers without learning evaluation targets."""
import hashlib
import json
import os
import pathlib
import shutil
import subprocess
import time
import xml.etree.ElementTree as ET
from datetime import datetime, timezone

from fixtures import marked_root

# fff/rich tracks each target this many times (engine/rust/fff.rs prepare_prior_seed), so flea/rich records as many opens.
RICH_OPENS = 3


def replay(state, tree, expected, targets, profile, env):
    env = dict(env, _ZO_DATA_DIR=str(pathlib.Path(env['XDG_DATA_HOME']) / 'zoxide'))
    # The rich profile replays the evaluation targets themselves, the history fff/rich is seeded with.
    rich = profile == 'rich'
    held_out = set() if rich else {os.fsencode(target) for target in targets}
    candidates = [(path, kind) for path, kind in expected if path not in held_out]
    files = sorted(set(targets)) if rich else [os.fsdecode(path) for path, kind in candidates if kind == 'f'][:32]
    folders = [] if rich else [os.fsdecode(path) for path, kind in candidates if kind == 'd'][:16]
    active = profile in ('equal-history', 'rich')
    root = ET.Element('xbel', version='1.0')
    namespace = 'http://www.freedesktop.org/standards/desktop-bookmarks'
    stamp = datetime.now(timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')
    for rel in files if active else []:
        bookmark = ET.SubElement(root, 'bookmark', href=(tree / rel).as_uri(),
                                 added=stamp, modified=stamp, visited=stamp)
        metadata = ET.SubElement(ET.SubElement(bookmark, 'info'), 'metadata', owner='http://freedesktop.org')
        applications = ET.SubElement(metadata, '{' + namespace + '}applications')
        ET.SubElement(applications, '{' + namespace + '}application', name='findbench-replay',
                      exec='findbench-replay %u', count=str(RICH_OPENS if rich else 1), modified=stamp)
    xbel = pathlib.Path(env['XDG_DATA_HOME']) / 'recently-used.xbel'
    xbel.write_bytes(ET.tostring(root, encoding='utf-8', xml_declaration=True))
    history = files + folders if active else []
    (state / 'history.json').write_text(json.dumps(history) + '\n')
    zoxide = dict(supported=True, entries=len(folders) if active else 0)
    at = time.perf_counter_ns()
    if active:
        try:
            command = ['zoxide', 'add', '--', *(str(tree / rel) for rel in folders)]
            if folders:
                subprocess.run(command, env=env, check=True, capture_output=True, timeout=5)
        except (OSError, subprocess.SubprocessError) as error:
            zoxide = dict(supported=False, reason=str(error))
    report = dict(profile=profile, replayed_at=stamp, held_out=sorted(os.fsdecode(p) for p in held_out),
                  replay=history, xbel_sha256=hashlib.sha256(xbel.read_bytes()).hexdigest(),
                  zoxide=zoxide, replay_us=(time.perf_counter_ns() - at) / 1000,
                  policy='read-only consumers; no evaluation target opens; no learning during typing')
    (state / 'priors.json').write_text(json.dumps(report, indent=2) + '\n')
    return report


def prepare(parent, tree, expected, targets, build=None, native_profiles=()):
    seed = marked_root(parent, 'shared-priors-')
    data = seed / 'data'
    data.mkdir()
    env = dict(os.environ, XDG_DATA_HOME=str(data), _ZO_DATA_DIR=str(data / 'zoxide'))
    replay(seed, tree, expected, targets, 'equal-history', env)
    for profile in sorted(set(native_profiles) & {'equal-history', 'rich'}):
        if build is None:
            raise ValueError('native prior preparation requires the built adapter')
        native = seed / ('fff-' + profile)
        native.mkdir()
        (native / '.findbench-root').write_text('findbench1\n')
        history = sorted(set(targets)) if profile == 'rich' else json.loads((seed / 'history.json').read_text())
        (native / 'history.json').write_text(json.dumps(history) + '\n')
        command = [str(pathlib.Path(build) / 'target/release/fff-driver'), str(tree), str(native), 'seed-' + profile, '2']
        at = time.perf_counter_ns()
        reply = subprocess.run(command, env=env, check=True, capture_output=True, text=True, timeout=30)
        report = json.loads(reply.stdout)
        if report.get('t') != 'priorseed' or report.get('prior_replay_inside_open') is not False:
            raise ValueError('native prior seed lacks out-of-clock replay proof')
        report['observed_replay_us'] = (time.perf_counter_ns() - at) / 1000
        report['files'] = database_hashes(native / 'frecency')
        (native / 'fff-prior-seed.json').write_text(json.dumps(report, indent=2) + '\n')
    return seed


def clone(seed, state, env):
    at = time.perf_counter_ns()
    source, destination = seed / 'data', pathlib.Path(env['XDG_DATA_HOME'])
    shutil.copyfile(source / 'recently-used.xbel', destination / 'recently-used.xbel')
    if (source / 'zoxide').is_dir():
        shutil.copytree(source / 'zoxide', destination / 'zoxide')
    shutil.copyfile(seed / 'history.json', state / 'history.json')
    report = json.loads((seed / 'priors.json').read_text())
    report['shared_seed'] = str(seed)
    report['copy_us'] = (time.perf_counter_ns() - at) / 1000
    report['provider_hashes'] = {str(path.relative_to(destination)): hashlib.sha256(path.read_bytes()).hexdigest()
                                 for path in destination.rglob('*') if path.is_file()}
    (state / 'priors.json').write_text(json.dumps(report, indent=2) + '\n')
    return report


def database_hashes(directory):
    if not directory.is_dir() or directory.is_symlink():
        raise ValueError('native prior database missing')
    files = {str(path.relative_to(directory)): hashlib.sha256(path.read_bytes()).hexdigest()
             for path in directory.rglob('*') if path.is_file() and not path.is_symlink()}
    if not files:
        raise ValueError('native prior database empty')
    return files


def clone_fff(seed, profile, state):
    source = seed / ('fff-' + profile)
    report = json.loads((source / 'fff-prior-seed.json').read_text())
    if report['files'] != database_hashes(source / 'frecency'):
        raise ValueError('native prior seed changed after replay')
    at = time.perf_counter_ns()
    shutil.copytree(source / 'frecency', state / 'frecency')
    if report['files'] != database_hashes(state / 'frecency'):
        raise ValueError('native prior copy differs from fixture seed')
    report['copy_us'] = (time.perf_counter_ns() - at) / 1000
    report['shared_seed'] = str(source)
    (state / 'fff-prior-seed.json').write_text(json.dumps(report, indent=2) + '\n')
    return report
