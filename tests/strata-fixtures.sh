#!/usr/bin/env bash
# Builds and verifies the full disk fixture once; the ordinary contracts avoid this 6 GiB write.
set -euo pipefail
repo=$(cd "$(dirname "$0")/.." && pwd -P)
mkdir -p "$repo/.superpowers/m9-integ-07"
scratch=$(mktemp -d "$repo/.superpowers/m9-integ-07/fixtures.XXXXXXXX")
[[ -n "$scratch" && "$scratch" == /* ]] || exit 1
touch "$scratch/.flea-test-sandbox"
trap 'case $scratch in "$repo"/.superpowers/m9-integ-07/fixtures.?*) [ -f "$scratch/.flea-test-sandbox" ] && rm -rf -- "$scratch" ;; esac' EXIT
python3 -B - "$repo" "$scratch" <<'PY'
import importlib.machinery
import importlib.util
import json
from pathlib import Path
import subprocess
import struct
import sys

repo, root = map(Path, sys.argv[1:])
loader = importlib.machinery.SourceFileLoader('strata_fixtures', str(repo / 'tools/flea-strata-bench'))
spec = importlib.util.spec_from_loader(loader.name, loader)
tool = importlib.util.module_from_spec(spec)
loader.exec_module(tool)
raw_generator = tool.load_module('hermetic_dng_generator', repo / 'tests/fixtures/raw/generate.py')
hermetic = root / 'hermetic-only'
hermetic.mkdir()
seed = hermetic / 'seed.dng'
seed.write_bytes(raw_generator.build())
seed_digest, seed_bytes = tool.sha(seed), seed.stat().st_size
identity = {'kind': 'hermetic fixture test only, not a native RAW corpus', 'root': str(hermetic),
            'manifest_sha256': '0' * 64,
            'sets': {name: [{'path': str(seed), 'format': 'dng', 'fixture_name': 'sample_%03d.dng' % i,
                            'bytes': seed_bytes, 'sha256': seed_digest} for i in range(count)]
                     for name, count in (('twelve', 12), ('hundred', 100))}}
original_loader = tool.load_raw_corpus
try:
    tool.load_raw_corpus = lambda value: identity
    tool.fixtures(root, raw_corpus=hermetic)
finally:
    tool.load_raw_corpus = original_loader
data = tool.validate_fixtures(root)
for kind in ('jpg', 'png', 'webp', 'tiff'):
    for path in sorted((root / 'fixtures' / kind).iterdir()):
        size = subprocess.check_output(['magick', 'identify', '-format', '%wx%h', str(path)], text=True)
        assert size == '1920x1080', (path, size)
for kind in ('mp4', 'webm'):
    path = next((root / 'fixtures' / kind).iterdir())
    stream = json.loads(subprocess.check_output(['ffprobe', '-v', 'error', '-show_streams', '-of', 'json', str(path)], text=True))['streams'][0]
    assert (stream['width'], stream['height']) == (1920, 1080)
    subprocess.run(['ffmpeg', '-v', 'error', '-xerror', '-i', str(path), '-f', 'null', '-'], check=True)
seed = root / 'repeat.png'
subprocess.run(['magick', '-limit', 'thread', '1', '-seed', '7', '-size', '1920x1080', 'plasma:fractal',
                '-depth', '8', '-strip', '-define', 'png:exclude-chunks=date,time', str(seed)], check=True)
assert tool.sha(seed) == tool.sha(root / 'fixtures/png/sample_00.png'), 'image generation is not reproducible'
raw = tool.load_module('strata_raw_check', repo / 'tests/fixtures/raw/check.py')
dng = next((root / 'fixtures/raw').iterdir()).read_bytes()
assert dng[:4] == b'II\x2a\0'
fields = raw.ifd(dng, struct.unpack_from('<I', dng, 4)[0])
assert fields[0xc612] == bytes((1, 4, 0, 0))
assert fields[0x0100] == struct.pack('<I', 6000) and fields[0x0101] == struct.pack('<I', 4000)
assert data['sets']['raw']['files'] == 12 and data['sets']['cpu-raw']['files'] == 100
print('strata-fixtures: fixture_manifest_sha256=' + tool.sha(root / 'fixtures.json'))
print('strata-fixtures: total_bytes=' + str(sum(s['bytes'] for s in data['sets'].values())))
print('strata-fixtures: PASS 100,000 entries, 2,000 JPEGs, all eight 12-file sets, CPU sets, hashes and decode checks')
PY
