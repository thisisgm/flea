#!/usr/bin/env python3
"""Real fontconfig and filesystem contract, with HOME/data/cache entirely inside a temporary root."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

binary = str(Path('target/debug/flea').resolve())
font = Path(subprocess.check_output(['fc-match', '-f', '%{file}'], text=True))
assert font.is_file(), 'fontconfig must supply one real font for the fixture'
with tempfile.TemporaryDirectory(prefix='flea-font-test-') as tmp:
    root = Path(tmp)
    env = {**os.environ, 'HOME': str(root / 'home'), 'XDG_DATA_HOME': str(root / 'data'),
           'XDG_CACHE_HOME': str(root / 'cache')}
    original = root / 'two # fonts ? café.ttf'
    original.write_bytes(font.read_bytes())
    def run(verb, path=original, good=True):
        p = subprocess.run([binary, '--font', verb, str(path)], env=env, capture_output=True, text=True, timeout=40)
        result = json.loads(p.stdout)
        assert (p.returncode == 0) == good, (verb, p.returncode, result)
        assert not p.stderr, p.stderr
        return result
    info = run('info')
    assert info['family'] and not info['installed']
    assert not (root / 'data').exists(), 'inspection must not create the install root'
    installed = run('install')
    target = Path(installed['location'])
    assert installed['installed'] and installed['cacheOk'] and target.read_bytes() == original.read_bytes()
    assert target.parent == root / 'data/fonts/flea'
    assert str(target) in subprocess.check_output(['fc-list', '-f', '%{file}\n'], env=env, text=True).splitlines(), 'installed font must be discoverable by other applications'
    assert run('install')['installed'], 'install is idempotent'
    renamed = root / 'renamed.OTF'
    renamed.write_bytes(original.read_bytes())
    assert run('info', renamed)['installed'], 'identity follows bytes, not filename or extension'
    assert len(list(target.parent.glob('*.ttf'))) == 1
    assert run('uninstall')['installed'] is False
    assert original.exists() and renamed.exists() and not target.exists(), 'the download survives uninstall'
    assert str(target) not in subprocess.check_output(['fc-list', '-f', '%{file}\n'], env=env, text=True).splitlines(), 'uninstalled font must leave Fontconfig'
    run('uninstall', good=False)
    # OpenType/CFF takes the same real install path when the host supplies a fixture.
    otfs = [Path(p) for p in subprocess.check_output(['fc-list', '-f', '%{file}\n'], text=True).splitlines() if p.endswith('.otf')]
    if otfs:
        otf = root / 'opentype.otf'
        otf.write_bytes(otfs[0].read_bytes())
        installed_otf = run('install', otf)
        assert Path(installed_otf['location']).suffix == '.otf'
        assert installed_otf['installed'] and run('uninstall', otf)['installed'] is False
        assert otf.exists()
    missing = subprocess.run([binary, '--font', 'install', str(original)], env={**env, 'PATH': ''}, capture_output=True, text=True)
    assert missing.returncode == 2 and 'error' in json.loads(missing.stdout)
    assert not target.exists(), 'missing fontconfig must not install anything'
    missing_home = subprocess.run([binary, '--font', 'info', str(original)],
        env={**env, 'HOME': '', 'XDG_DATA_HOME': ''}, capture_output=True, text=True)
    assert missing_home.returncode == 2 and 'directory' in json.loads(missing_home.stdout)['error']
    # A planted symlink is never followed or deleted.
    target.symlink_to(original)
    run('install', good=False)
    run('uninstall', good=False)
    assert target.is_symlink() and original.exists()
    target.unlink()
    target.write_bytes(b'other bytes')
    run('install', good=False)
    assert target.read_bytes() == b'other bytes'
    target.unlink()
    # Browsed user installations can be removed; system files cannot.
    existing = root / 'data/fonts/Existing.ttf'
    existing.write_bytes(original.read_bytes())
    assert run('info', existing)['installed']
    run('uninstall', existing)
    assert not existing.exists() and original.exists()
    run('uninstall', font, good=False)
    linked = root / 'data/fonts/Linked.ttf'
    linked.symlink_to(original)
    assert not run('info', linked)['installed']
    run('uninstall', linked, good=False)
    assert linked.is_symlink() and original.exists()
    bad = root / 'bad.ttf'
    for data in (b'no font', b'\0\x01\0\0broken', b'OTTOinvalid'):
        bad.write_bytes(data)
        run('install', bad, good=False)
    fifo = root / 'pipe.ttf'
    os.mkfifo(fifo)
    run('info', fifo, good=False)
    # An installed font with another name must not be clobbered, and no staging file survives.
    assert not list(root.rglob('.flea-font-*.tmp'))
    assert hashlib.md5(original.read_bytes()).hexdigest() in target.name
print('font-management: real install, cache refresh, duplicate, uninstall and refusal checks passed')
