"""Binary-only reuse retains original build receipts while the collector evolves."""
import hashlib
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent
CANDIDATE = '0ebbcffae14609a3663c90db6d5f73c5e86c97a4'
ANCHOR = 'f21789e56cc4b0877caa3156fff7bce52a8dd9c2'


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def candidate(binary):
    root = binary.parents[2]
    receipt = json.loads((root / 'candidate.json').read_text())
    if receipt['commit'] != CANDIDATE or receipt['sha256'] != digest(binary):
        raise ValueError('candidate binary identity differs')
    if receipt['metrics_patch_sha256'] != digest(HERE / 'engine/flea-metrics.patch') or receipt['stage_sha256'] != digest(HERE / 'stage.rs'):
        raise ValueError('candidate instrumentation changed after build')
    return receipt


def engine(root):
    if not (root / 'BUILD-COMPLETE').is_file():
        raise ValueError('engine build is not complete')
    if (root / 'flea-commit.txt').read_text().strip() != CANDIDATE:
        raise ValueError('engine candidate source commit differs')
    if (root / 'anchor-commit.txt').read_text().strip() != ANCHOR:
        raise ValueError('tagged anchor source commit differs')
    expected_pins = dict(line.split('\t') for line in (HERE / 'engine/rival-pins.tsv').read_text().splitlines())
    actual = dict(line.split('\t') for line in (root / 'current-sources.tsv').read_text().splitlines())
    for name in ['fff', 'fsearch', 'fsearch-nd']:
        if actual.get(name) != expected_pins[name]:
            raise ValueError('rival source differs: ' + name)
    expected = {
        'target/release/fff-driver', 'target/release/fsearch-driver', 'target/release/fsearch-nd-driver',
        'target-historical/release/fff-driver', 'flea-target/release/flea-engine',
        'flea-anchor-target/release/flea-engine',
    }
    verified = {}
    for line in (root / 'binaries.sha256').read_text().splitlines():
        sha, raw = line.split(maxsplit=1)
        suffix = raw.split(root.name + '/', 1)[-1]
        if suffix not in expected:
            raise ValueError('unknown binary receipt path: ' + raw)
        binary = root / suffix
        if digest(binary) != sha:
            raise ValueError('binary hash differs: ' + suffix)
        verified[suffix] = sha
    if set(verified) != expected:
        raise ValueError('engine binary roster incomplete')
    return dict(binaries=verified, rival_pins=actual, anchor=ANCHOR,
                anchor_label='tagged source-core 0.3.12', image=(root / 'image-id').read_text().strip())


def roster(binary):
    receipt = json.loads((binary.parents[2] / 'rosters.json').read_text())
    name = binary.name
    expected = receipt[name]
    source = HERE.parent / name
    hashes = {str(p.relative_to(source)): digest(p) for p in sorted(source.rglob('*'))
              if p.is_file() and p.suffix in {'.rs', '.toml'} and 'target' not in p.parts}
    if expected['sha256'] != digest(binary) or expected['sources'] != hashes or expected['lock'] != digest(source / 'Cargo.lock'):
        raise ValueError('roster binary or source identity differs: ' + name)
    return expected
