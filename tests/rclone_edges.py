"""Real-rclone coverage for source traversal, skipped descendants and remote paths."""
import os
import socket

FOLDER_ERROR = 'An rclone folder cannot contain a .. component.'


def local_remote(f):
    (f.root / 'bin/rclone').symlink_to(f.control['real_binary'])


def refused(f, source, destination, omitted):
    ident = f.send(paths=[source], folder=str(destination))
    receipt = f.backend.wait(lambda e: e['t'] == 'rclonesent' and e['id'] == ident)
    assert not receipt['ok'] and receipt['check'] == 'none' and not receipt['receipt'], receipt
    assert str(omitted) in receipt['error'], receipt
    assert f.backend.jobs[ident]['state'] == 'failed', f.backend.jobs[ident]
    return receipt


def source_link(f):
    tree = f.root / 'source/tree'
    tree.mkdir()
    (tree / 'payload.txt').write_text('selected content')
    outside = f.root / 'unselected.txt'
    outside.write_text('unselected fixture content')
    link = tree / 'outside-link'
    if f.control.get('late_link'):
        f.configure(real=f.control['real_binary'])
        (f.root / 'bin/rclone').symlink_to(f.control['real_stub'])
    else:
        local_remote(f)
        link.symlink_to(outside)
    destination = f.root / 'remote'
    ident = f.send(paths=[tree], folder=str(destination))
    receipt = f.backend.wait(lambda e: e['t'] == 'rclonesent' and e['id'] == ident)
    target = destination / 'tree/outside-link'
    assert not target.exists() and not target.is_symlink(), 'unselected link target was uploaded'
    assert not receipt['ok'] and receipt['check'] == 'none', receipt
    assert link.name in receipt['error'], receipt
    assert outside.read_text() == 'unselected fixture content'


def skipped_descendant(f):
    local_remote(f)
    tree = f.root / 'source/tree'
    nested = tree / 'deep'
    nested.mkdir(parents=True)
    (nested / 'payload.txt').write_text('selected content')
    kind = f.control['unsupported']
    omitted = nested / kind
    held_socket = None
    if kind == 'symlink':
        omitted.symlink_to('payload.txt')
    elif kind == 'directory-link':
        omitted.symlink_to(f.root / 'source', target_is_directory=True)
    elif kind == 'dangling-link':
        omitted.symlink_to('absent')
    elif kind == 'fifo':
        os.mkfifo(omitted)
    elif kind == 'socket':
        held_socket = socket.socket(socket.AF_UNIX)
        held_socket.bind(str(omitted))
    else:
        raise AssertionError(kind)
    try:
        refused(f, tree, f.root / 'remote', omitted)
        assert not (f.root / 'remote/tree').exists(), 'unsupported tree partially uploaded'
    finally:
        if held_socket:
            held_socket.close()


def reporting(f):
    local_remote(f)
    destination = f.root / 'remote'
    ident = f.send(folder=str(destination))
    receipt = f.backend.wait(lambda e: e['t'] == 'rclonesent' and e['id'] == ident)
    assert receipt['ok'] and receipt['check'] == 'checksum', receipt
    assert all((destination / p.name).read_bytes() == p.read_bytes() for p in f.paths)


def transient_skip(f):
    f.configure(real=f.control['real_binary'])
    (f.root / 'bin/rclone').symlink_to(f.control['real_stub'])
    tree = f.root / 'source/tree'
    tree.mkdir()
    (tree / 'payload.txt').write_text('selected content')
    refused(f, tree, f.root / 'remote', 'transient-fifo')
    assert not (tree / 'transient-fifo').exists(), 'injection did not disappear before verification'


def colon_folder(f):
    local_remote(f)
    destination = f.root / 'remote:'
    if f.control['directory']:
        source = f.root / 'source/tree'
        source.mkdir()
        (source / 'payload.txt').write_text('selected content')
    else:
        source = f.paths[0]
    ident = f.send(paths=[source], folder=str(destination))
    receipt = f.backend.wait(lambda e: e['t'] == 'rclonesent' and e['id'] == ident)
    assert receipt['ok'] and receipt['check'] == 'checksum', receipt
    intended = destination / source.name
    actual = f.root / ('remote:' + source.name)
    assert intended.exists() and not actual.exists(), 'colon folder joined to a sibling'
    if source.is_dir():
        assert (intended / 'payload.txt').read_bytes() == (source / 'payload.txt').read_bytes()
    else:
        assert intended.read_bytes() == source.read_bytes()


def parent_folder(f):
    local_remote(f)
    cwd = f.root / f.control['cwd']
    source = f.paths[1]
    nested = f.send(paths=[source], folder='control/nested')
    receipt = f.backend.wait(lambda e: e['t'] == 'rclonesent' and e['id'] == nested)
    assert receipt['ok'] and receipt['check'] == 'checksum', receipt
    assert (cwd / 'control/nested' / source.name).read_bytes() == source.read_bytes()
    for request, folder in enumerate(['../../local-escaped', '..', 'control/..',
                                      'control/../../../local-escaped'], 210):
        f.backend.send(c='rclone', op='send', id=request, remote='gdrive', folder=folder, paths=[str(source)])
        reply = f.backend.wait(lambda e: e['t'] == 'rclonestarted' and e['requestId'] == request)
        assert not reply['ok'] and 'id' not in reply and reply['error'] == FOLDER_ERROR, (folder, reply)
    f.backend.send(c='jobs', op='list')
    assert [j['id'] for j in f.backend.wait(lambda e: e['t'] == 'jobs')['jobs']] == [nested]
    copies = sorted(str(p.relative_to(f.root)) for p in f.root.rglob(source.name))
    assert copies == sorted(['source/' + source.name, f.control['cwd'] + '/control/nested/' + source.name]), copies


def cases(real, stub):
    result = [
        ('inherited local copy-links cannot escape the selected tree', source_link,
         dict(stub=False, inherited={'RCLONE_LOCAL_COPY_LINKS': 'true'})),
        ('late symlink cannot be followed by inherited traversal aliases', source_link,
         dict(stub=False, late_link=True, inherited={'RCLONE_LOCAL_COPY_LINKS': 'true',
              'RCLONE_COPY_LINKS': 'true', 'RCLONE_CONFIG_LOCAL_COPY_LINKS': 'true',
              'RCLONE_LOCAL_SKIP_LINKS': 'true', 'RCLONE_SKIP_SPECIALS': 'true'})),
        ('inherited quiet logging cannot abort a send', reporting,
         dict(stub=False, inherited={'RCLONE_QUIET': 'true'})),
        ('inherited verbose logging cannot abort a send', reporting,
         dict(stub=False, inherited={'RCLONE_VERBOSE': '1'})),
        ('a reported skipped descendant fails even after it disappears', transient_skip,
         dict(stub=False, transient_skip=True)),
    ]
    for kind in ['symlink', 'directory-link', 'dangling-link', 'fifo', 'socket']:
        result.append((kind + ' descendant is refused by name', skipped_descendant,
                       dict(stub=False, unsupported=kind)))
    for directory in [False, True]:
        result.append(('colon folder joins ' + ('directory' if directory else 'file') + ' correctly',
                       colon_folder, dict(stub=False, directory=directory)))
    result.append(('parent folder components are refused before a job', parent_folder,
                   dict(stub=False, cwd='work/a/b')))
    for _, _, options in result:
        options.update(real_binary=real, real_stub=str(stub))
    return result
