"""Read the immutable test corpus in checkouts and source archives."""

import csv
import hashlib
import os
from pathlib import Path
import subprocess
import zipfile


REVISION = 'f21789e56cc4b0877caa3156fff7bce52a8dd9c2'
SOURCE_COUNT = 533
ARCHIVE_LIMIT = 2_000_000
REVISION_ABSENT = 1


# A shallow or single-branch clone lacks the pinned commit and reads the hash-verified archive; a broken repository fails.
def pinned_commit(repo, check):
    if not (repo / '.git').exists():
        return False
    # Git speaks the C locale here, so a failed lookup names its cause the same way on every machine.
    result = subprocess.run(['git', '-C', str(repo), 'rev-parse', '--verify', '--quiet', '--end-of-options',
                             f'{REVISION}^{{commit}}'], capture_output=True, env={**os.environ, 'LC_ALL': 'C'})
    check(result.returncode in (0, REVISION_ABSENT), f'Git could not look up the frozen commit: {result.stderr!r}')
    return result.returncode == 0


# Sample input: path\tsha256 followed by tests/rename-cost.qml\t<64 hexadecimal digits> (TSV).
def sources(repo, fixture, check):
    manifest = list(csv.DictReader((fixture / 'frozen-sources.tsv').open(), delimiter='\t'))
    paths = [row['path'] for row in manifest]
    check(len(paths) == SOURCE_COUNT and len(set(paths)) == SOURCE_COUNT,
          f'the frozen corpus names all {SOURCE_COUNT} QML/JS sources exactly once')
    archive_path = fixture / 'frozen-sources.zip'
    check(archive_path.stat().st_size < ARCHIVE_LIMIT, 'the committed frozen copy stays below 2 MB')
    checkout = pinned_commit(repo, check)
    with zipfile.ZipFile(archive_path) as archive:
        check(len(archive.namelist()) == SOURCE_COUNT and set(archive.namelist()) == set(paths),
              'the frozen copy contains exactly the manifest paths')
        for row in manifest:
            relative = Path(row['path'])
            check(relative.parent in (Path('tests'), Path('tests/js')) and relative.suffix in ('.qml', '.js'),
                  f'the frozen path stays in the test corpus: {relative}')
            snapshot = archive.read(row['path'])
            check(hashlib.sha256(snapshot).hexdigest() == row['sha256'],
                  f'frozen v0.3.12 source bytes: {relative}')
            if checkout:
                result = subprocess.run(['git', '-C', str(repo), 'show', f'{REVISION}:{relative}'],
                                        capture_output=True)
                check(result.returncode == 0, f'frozen Git object unavailable: {relative}: {result.stderr!r}')
                check(hashlib.sha256(result.stdout).hexdigest() == row['sha256'],
                      f'frozen Git object differs from manifest: {relative}')
                snapshot = result.stdout
            yield relative, snapshot
