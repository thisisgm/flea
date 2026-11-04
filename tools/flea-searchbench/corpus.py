"""One prepared corpus registry for engine, matcher and walker stages."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import fcntl
import tempfile

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE / 'engine'))
from fixtures import synthetic, inventory
from contracts import identities

DEFAULT_ROOT = Path('/home/gm/flea-work/bench-corpora')
COMMIT = '0ebbcffae14609a3663c90db6d5f73c5e86c97a4'
SOURCE_DIGEST = '13e5fe5a7904f9a5474d5fa915968d42070060d8741bc26a01423c05b3cefaa9'
SOURCE_COUNT = 4902
SOURCE_MANIFEST = HERE / 'source-manifest.json'
SOURCE_QUERIES = [
    ('exact', 'fuzzy.rs', 'src/backend/fuzzy.rs'),
    ('typo', 'fzuzy.rs', 'src/backend/fuzzy.rs'),
    ('path', 'backend/searchindex', 'src/backend/searchindex.rs'),
    ('extension', '.qml', 'ui/SearchStrip.qml'),
    ('deep', 'src/backend/searchw', 'src/backend/searchwalk.rs'),
]


def identity_digest(items):
    digest = hashlib.sha256()
    for path, kind in sorted(items):
        raw = os.fsencode(path)
        digest.update(len(raw).to_bytes(8, 'little'))
        digest.update(raw)
        digest.update(kind.encode('ascii'))
    return digest.hexdigest()


def checked_source(result, origin):
    if len(result) != SOURCE_COUNT:
        raise ValueError(f'pinned source manifest differs: {origin} lists {len(result)} entries, expected {SOURCE_COUNT}')
    digest = identity_digest(result)
    if digest != SOURCE_DIGEST:
        raise ValueError(f'pinned source manifest differs: {origin} digest {digest}, expected {SOURCE_DIGEST}')
    return result


def source_manifest(repo):
    # A source archive has no Git history, so it reads COMMIT's inventory packaged beside this file.
    if not (Path(repo) / '.git').exists():
        # Sample input: [["Cargo.lock", "f"], ["src", "d"], ["src/main.rs", "f"]]
        packaged = [tuple(item) for item in json.loads(SOURCE_MANIFEST.read_text())]
        return checked_source(packaged, SOURCE_MANIFEST)
    records = subprocess.run(['git', '-C', str(repo), 'ls-tree', '-r', '-t', '-z', COMMIT],
                             check=True, capture_output=True).stdout.split(b'\0')
    result = []
    for record in filter(None, records):
        meta, name = record.split(b'\t', 1)
        mode, kind, _ = meta.split()
        if mode in (b'120000', b'160000') or any(p.startswith(b'.') for p in name.split(b'/')):
            continue
        result.append((os.fsdecode(name), 'd' if kind == b'tree' else 'f'))
    result.sort()
    return checked_source(result, f'git ls-tree {COMMIT}')


def source_fixture(root, repo):
    manifest = source_manifest(repo)
    fixture = root / 'fixture-flea-source'
    fixture.mkdir()
    (fixture / '.findbench-root').write_text('flea-searchbench\n')
    tree = fixture / 'tree'
    tree.mkdir()
    for rel, kind in manifest:
        path = tree / rel
        if kind == 'd':
            path.mkdir(parents=True, exist_ok=True)
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            content = subprocess.run(['git', '-C', str(repo), 'show', COMMIT + ':' + rel],
                                     check=True, capture_output=True).stdout
            with path.open('xb') as out:
                out.write(content)
    return fixture, tree, SOURCE_QUERIES, dict(inventory(tree), kind='flea-source', commit=COMMIT)


def prepare(root, repo, labels):
    if not root.is_absolute() or root.is_symlink():
        raise ValueError('corpus root must be absolute and nonsymlink')
    root.mkdir(parents=True, exist_ok=True)
    if root != root.resolve():
        raise ValueError('corpus root must be canonical')
    with (root / '.prepare.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        registry_path = root / 'registry.json'
        registry = json.loads(registry_path.read_text()) if registry_path.exists() else {}
        for label in labels:
            if label not in {'100000', '1000000', 'flea-source'}:
                raise ValueError('unknown corpus ' + label)
            if label in registry:
                load(root, label)
                continue
            fixture, tree, queries, info = source_fixture(root, repo) if label == 'flea-source' else synthetic(root, int(label))
            expected = [(os.fsdecode(path), kind) for path, kind in identities(tree)]
            digest = identity_digest(expected)
            if label == 'flea-source' and digest != SOURCE_DIGEST:
                raise ValueError('materialized source inventory differs')
            paths = fixture / 'paths.txt'
            paths.write_text(''.join(path + '\n' for path, _ in expected))
            (fixture / 'manifest.json').write_text(json.dumps(expected) + '\n')
            registry[label] = dict(fixture=str(fixture), tree=str(tree), paths=str(paths),
                                   queries=queries, info=info, identity_sha256=digest,
                                   paths_sha256=hashlib.sha256(paths.read_bytes()).hexdigest())
            with tempfile.NamedTemporaryFile(mode='w', prefix='.registry-', dir=root, delete=False) as stream:
                json.dump(registry, stream, indent=2)
                stream.write('\n')
                staged = Path(stream.name)
            staged.replace(registry_path)
        return registry


def load(root, label):
    registry = json.loads((root / 'registry.json').read_text())
    value = registry[label]
    for key in ['fixture', 'tree', 'paths']:
        path = Path(value[key])
        path.resolve(strict=True).relative_to(root.resolve(strict=True))
        if path.is_symlink() or path != path.resolve():
            raise ValueError('aliased corpus path')
    fixture = Path(value['fixture'])
    if not (fixture / '.findbench-root').is_file():
        raise ValueError('corpus marker missing')
    expected = json.loads((fixture / 'manifest.json').read_text())
    actual = identities(Path(value['tree']))
    if len(actual) != value['info']['entries'] or len(expected) != len(actual):
        raise ValueError('shared corpus count differs from its manifest')
    if identity_digest(actual) != value['identity_sha256'] or identity_digest(expected) != value['identity_sha256']:
        raise ValueError('shared corpus identities changed')
    if hashlib.sha256(Path(value['paths']).read_bytes()).hexdigest() != value['paths_sha256']:
        raise ValueError('matcher paths changed')
    return value
