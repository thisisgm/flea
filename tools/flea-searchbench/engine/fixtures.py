import collections
import json
import os
import pathlib
import shutil
import tempfile

TOPS = ['Documents', 'Downloads', 'Pictures', 'Music', 'Videos', 'projects', 'archive', 'shared']
EXTENSIONS = ['rs', 'c', 'h', 'py', 'js', 'qml', 'md', 'txt', 'pdf', 'jpg', 'mp4', 'flac', 'svg', 'json', 'zip']
WORDS = ['report', 'notes', 'render', 'window', 'invoice', 'meeting', 'holiday', 'budget', 'parser', 'model', 'test', 'draft']
TARGETS = [
    ('exact', 'render_frame.rs', 'projects/editor/src/components/render_frame.rs'),
    ('typo', 'redner_frame.rs', 'projects/editor/src/components/render_frame.rs'),
    ('path', 'editor/src/components/render', 'projects/editor/src/components/render_frame.rs'),
    ('extension', '.svg', 'Documents/reference/architecture_diagram.svg'),
    ('deep', 'archive/2024/client/reports/quarterly', 'archive/2024/client/reports/quarterly_review.pdf'),
]


def marked_root(parent, prefix):
    parent = pathlib.Path(parent).resolve(strict=True)
    root = pathlib.Path(tempfile.mkdtemp(prefix=prefix, dir=parent))
    (root / '.findbench-root').write_text('findbench1\n')
    return root


def checked_mutation(root, path):
    root = pathlib.Path(root)
    path = pathlib.Path(path)
    if not str(path) or not root.is_absolute() or not path.is_absolute():
        raise ValueError('mutation paths must be absolute and nonempty')
    if not (root / '.findbench-root').is_file() or path.is_symlink():
        raise ValueError('missing root marker or symlink mutation')
    path.resolve(strict=False).relative_to(root.resolve(strict=True))
    if path == root or path.name == '.findbench-root':
        raise ValueError('root and marker are never mutation targets')
    return path


def inventory(tree):
    depths, lengths, extensions, fanout = collections.Counter(), collections.Counter(), collections.Counter(), collections.Counter()
    files = dirs = names_bytes = 0
    distinct = set()
    for base, subdirs, names in os.walk(tree, followlinks=False):
        subdirs[:] = sorted(d for d in subdirs if not d.startswith('.'))
        names = sorted(n for n in names if not n.startswith('.'))
        rel = pathlib.Path(base).relative_to(tree)
        depth = len(rel.parts)
        fanout[len(subdirs)] += 1
        for name in subdirs + names:
            distinct.add(name)
            lengths[len(name.encode())] += 1
            depths[depth + 1] += 1
            names_bytes += len(str(rel / name).encode())
        dirs += len(subdirs)
        files += len(names)
        extensions.update(pathlib.Path(n).suffix.lower() for n in names)
    return dict(entries=files + dirs, files=files, dirs=dirs, relative_path_bytes=names_bytes, distinct_basenames=len(distinct),
                depths=dict(depths), basename_lengths=dict(lengths), extensions=dict(extensions), directory_fanout=dict(fanout))


def seed_targets(tree):
    for _, _, rel in TARGETS:
        path = tree / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        if not path.exists():
            with path.open('xb'):
                pass


def synthetic(parent, entries):
    root = marked_root(parent, f'fixture-{entries}-')
    tree = root / 'tree'
    tree.mkdir()
    seed_targets(tree)
    leaf_count = max(8, entries // 32)
    leaves = []
    for i in range(leaf_count):
        digits, value = [], i // len(TOPS)
        while value:
            digits.append(value % 8)
            value //= 8
        # Assumed home shape: eight domains, bounded fan-out, variable depth, repeated folder names.
        leaf = tree / TOPS[i % len(TOPS)]
        for digit in reversed(digits or [0]):
            leaf /= f'folder_{digit:02d}'
        leaf.mkdir(parents=True, exist_ok=True)
        leaves.append(leaf)
    current = inventory(tree)['entries']
    for i in range(entries - current):
        # Repeated basenames and unique variants form separate deduplication conditions in the same fixture.
        stem = WORDS[i % len(WORDS)] + '_' + WORDS[(i // len(WORDS)) % len(WORDS)]
        if i % 3:
            stem += f'_{i:08d}'
        else:
            stem += f'_{i % 97:02d}'
        stem += '_detail' * (i % 5)
        name = stem + '.' + EXTENSIONS[i % len(EXTENSIONS)]
        leaf = leaves[(i * 37) % len(leaves)]
        path = leaf / name
        if path.exists():
            path = leaf / f'{i:08d}_{name}'
        with path.open('xb'):
            pass
    info = inventory(tree)
    if info['entries'] != entries:
        raise RuntimeError(f'exact fixture count failed: {info["entries"]} != {entries}')
    info.update(kind='synthetic', shape='assumption, not sampled from operator Home', seed='deterministic-formula-v1')
    (root / 'fixture.json').write_text(json.dumps(info, indent=2) + '\n')
    return root, tree, TARGETS, info


def source_fixture(parent):
    root = marked_root(parent, 'fixture-source-')
    tree = root / 'tree'
    tree.mkdir()
    for name in ['fff', 'fsearch']:
        source = pathlib.Path('/sources') / name
        target = tree / name
        for base, subdirs, names in os.walk(source, followlinks=False):
            # Copy data only; hidden metadata, symlinks, scripts and hooks are never executed.
            subdirs[:] = sorted(d for d in subdirs if not d.startswith('.') and not (pathlib.Path(base) / d).is_symlink())
            dest = target / pathlib.Path(base).relative_to(source)
            dest.mkdir(parents=True, exist_ok=True)
            for filename in sorted(names):
                original = pathlib.Path(base) / filename
                if filename.startswith('.') or not original.is_file() or original.is_symlink():
                    continue
                destination = dest / filename
                if destination.exists():
                    raise RuntimeError('source fixture destination already exists')
                shutil.copyfile(original, destination)
    queries = [
        ('exact', 'fsearch_database_scan.c', 'fsearch/src/fsearch_database_scan.c'),
        ('typo', 'fsearch_databsae_scan.c', 'fsearch/src/fsearch_database_scan.c'),
        ('path', 'fff-core/src/file_picker', 'fff/crates/fff-core/src/file_picker.rs'),
        ('extension', '.rs', 'fff/crates/fff-core/src/file_picker.rs'),
        ('deep', 'fff/crates/fff-core/src/watch', 'fff/crates/fff-core/src/watcher/background_watcher.rs'),
    ]
    for _, _, rel in queries:
        if not (tree / rel).is_file():
            raise RuntimeError('real source target missing: ' + rel)
    info = inventory(tree)
    if info['entries'] != 682:
        raise RuntimeError('frozen source fixture must hold exactly 682 identities')
    info.update(kind='real-source-copies', sources=['fff@d978416f', 'fsearch@cb6cd67f'],
                excluded='hidden names and symlinks; no Git metadata or ignore files copied')
    (root / 'fixture.json').write_text(json.dumps(info, indent=2) + '\n')
    return root, tree, queries, info
