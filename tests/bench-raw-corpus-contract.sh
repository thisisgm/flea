#!/usr/bin/env bash
# Holds the real camera RAW corpus contract: wrong set directory, a missing file, an altered hash and a generated-only corpus refuse.
set -euo pipefail
repo=$(cd "$(dirname "$0")/.." && pwd -P)
mkdir -p "$repo/.superpowers/bench-raw-corpus-contract"
scratch=$(mktemp -d "$repo/.superpowers/bench-raw-corpus-contract/test.XXXXXXXX")
[[ -n "$scratch" && "$scratch" == /* ]] || exit 1
: > "$scratch/.flea-test-sandbox"
cleanup() {
    [[ -n "$scratch" && "$scratch" == "$repo"/.superpowers/bench-raw-corpus-contract/test.* && "$scratch" == /* && -f "$scratch/.flea-test-sandbox" ]] || exit 1
    rm -rf -- "$scratch"
}
trap cleanup EXIT
python3 -B - "$repo" "$scratch" "${RAW_CORPUS_TOOL:-$repo/tools/flea-raw-corpus}" <<'PY'
import csv
import hashlib
from pathlib import Path
import runpy
import subprocess
import sys

repo, scratch, tool = map(Path, sys.argv[1:])
module = runpy.run_path(str(tool))
fields, filename = module['FIELDS'], module['filename']
manifest = repo / 'tools/flea-raw-corpus.tsv'
rows = module['load_manifest'](manifest)
twelve = [r for r in rows if r['set'] == 'twelve']
hundred = [r for r in rows if r['set'] == 'hundred']
assert (len(twelve), len(hundred)) == (12, 100), 'manifest pins twelve and hundred'
assert len({r['suffix'] for r in twelve}) == 12, 'twelve distinct formats'
assert len({r['sha256'] for r in rows}) == 112, 'sets share no file'
assert sum(int(r['bytes']) for r in twelve) == 348173466, 'twelve byte total'
assert sum(int(r['bytes']) for r in hundred) == 2120883456, 'hundred byte total'
assert all(r['url'].startswith('https://raw.pixls.us/') for r in rows), 'camera source only'
print('PASS bench-raw-corpus-contract: manifest pins 12 formats and 100 distinct camera originals')


def owned(path):
    assert path.is_absolute() and path.is_relative_to(scratch)
    assert (scratch / '.flea-test-sandbox').is_file()
    return path


def write_manifest(path, data):
    with path.open('w', newline='') as stream:
        writer = csv.DictWriter(stream, fieldnames=fields, delimiter='\t', lineterminator='\n')
        writer.writeheader()
        writer.writerows(data)


def cli(command, *args, ok=True, using=manifest):
    wrapper = ('import runpy,socket,sys; '
               'socket.socket.connect=lambda *a,**k:sys.exit("NETWORK FORBIDDEN"); '
               'sys.argv=sys.argv[1:];runpy.run_path(sys.argv[0],run_name="__main__")')
    result = subprocess.run([sys.executable, '-B', '-c', wrapper, str(tool), command,
                             *map(str, args), '--manifest', str(using)],
                            capture_output=True, text=True, timeout=60)
    assert (result.returncode == 0) == ok, (command, args, result.stdout, result.stderr)
    assert 'NETWORK FORBIDDEN' not in result.stderr, result.stderr
    return result


bodies = {'twelve': [b'twelve camera a\n', b'twelve camera b\n'], 'hundred': [b'hundred camera c\n']}
mini_rows = []
for name, payloads in bodies.items():
    for index, body in enumerate(payloads):
        mini_rows.append(dict(set=name, suffix='nef', make='Camera', model='Model ' + str(index),
                              url='https://raw.pixls.us/getfile.php/{}/{}.nef'.format(name, index),
                              sha256=hashlib.sha256(body).hexdigest(), bytes=str(len(body)),
                              licence=module['LICENCE'], mime='image/x-nikon-nef'))
mini = scratch / 'mini.tsv'
write_manifest(mini, mini_rows)
corpus = scratch / 'corpus'


def build():
    for row, body in zip(mini_rows, [b for name in bodies for b in bodies[name]]):
        folder = corpus / row['set']
        folder.mkdir(parents=True, exist_ok=True)
        path = owned(folder / filename(row))
        path.write_bytes(body)


build()
cli('verify', corpus, '--set', 'twelve', using=mini)
cli('verify', corpus, '--set', 'hundred', using=mini)
assert '3 files' in cli('verify', corpus, '--set', 'all', using=mini).stdout
print('PASS bench-raw-corpus-contract: twelve and hundred verify as sets below one corpus parent')

moved = mini_rows[1]
source = owned(corpus / 'twelve' / filename(moved))
wrong = owned(corpus / 'hundred' / filename(moved))
source.rename(wrong)
refusal = cli('verify', corpus, '--set', 'twelve', using=mini, ok=False).stderr
assert str(source) in refusal and 'mismatch' in refusal, refusal
refusal = cli('verify', corpus, '--set', 'hundred', using=mini, ok=False).stderr
assert 'extra file' in refusal and str(wrong) in refusal, refusal
wrong.rename(source)
flat = owned(scratch / 'flat')
flat.mkdir()
for path in sorted((corpus / 'twelve').iterdir()):
    owned(flat / path.name).write_bytes(path.read_bytes())
refusal = cli('verify', flat, '--set', 'twelve', using=mini, ok=False).stderr
assert 'extra file' in refusal, refusal
nested = owned(scratch / 'nested')
(nested / 'corpus').mkdir(parents=True)
(corpus / 'twelve').rename(nested / 'corpus' / 'twelve')
refusal = cli('verify', nested, '--set', 'twelve', using=mini, ok=False).stderr
assert 'extra file' in refusal, refusal
(nested / 'corpus' / 'twelve').rename(corpus / 'twelve')
real_twelve = owned(scratch / 'real-twelve')
(corpus / 'twelve').rename(real_twelve)
(corpus / 'twelve').symlink_to(real_twelve)
refusal = cli('verify', corpus, '--set', 'twelve', using=mini, ok=False).stderr
assert 'twelve' in refusal, refusal
owned(corpus / 'twelve').unlink()
real_twelve.rename(corpus / 'twelve')
cli('verify', corpus, '--set', 'all', using=mini)
print('PASS bench-raw-corpus-contract: a file under the wrong set, a flat, nested or linked set directory refuses')

missing = owned(corpus / 'hundred' / filename(mini_rows[2]))
missing.unlink()
refusal = cli('verify', corpus, '--set', 'hundred', using=mini, ok=False).stderr
assert str(missing) in refusal, refusal
cli('verify', corpus, '--set', 'twelve', using=mini)
refusal = cli('verify', corpus, '--set', 'all', using=mini, ok=False).stderr
assert str(missing) in refusal, refusal
missing.write_bytes(bodies['hundred'][0])
print('PASS bench-raw-corpus-contract: one missing file refuses its set and the whole corpus')

altered = owned(corpus / 'twelve' / filename(mini_rows[0]))
original = altered.read_bytes()
altered.write_bytes(bytes([original[0] ^ 1]) + original[1:])
assert len(altered.read_bytes()) == len(original)
refusal = cli('verify', corpus, '--set', 'twelve', using=mini, ok=False).stderr
assert str(altered) in refusal and 'sha256 mismatch' in refusal, refusal
altered.write_bytes(original + b'x')
refusal = cli('verify', corpus, '--set', 'twelve', using=mini, ok=False).stderr
assert str(altered) in refusal, refusal
altered.unlink()
payload = owned(scratch / 'matching-payload.nef')
payload.write_bytes(original)
altered.symlink_to(payload)
refusal = cli('verify', corpus, '--set', 'twelve', using=mini, ok=False).stderr
assert str(altered) in refusal, refusal
assert altered.is_symlink() and payload.read_bytes() == original
altered.unlink()
altered.write_bytes(original)
cli('verify', corpus, '--set', 'all', using=mini)
print('PASS bench-raw-corpus-contract: one altered byte, one extra byte or a linked matching file refuses')

generated_root = owned(scratch / 'generated')
generated_root.mkdir()
(generated_root / '.flea-test-sandbox').touch()
# Sample input: 751567ed42146f61462141bf4a26a028f98929de2079e5ab93c314189deca28f  L1000921.DNG
line = subprocess.run([sys.executable, '-B', str(repo / 'tests/fixtures/raw/generate.py'), str(generated_root)],
                      capture_output=True, text=True, check=True, timeout=120).stdout.split()
generated = generated_root / line[1]
generated_hash = line[0]
# Sample input: 751567ed42146f61462141bf4a26a028f98929de2079e5ab93c314189deca28f  L1000921.DNG
pinned = (repo / 'tests/fixtures/raw/SHA256SUMS').read_text().split()[0]
assert generated_hash == pinned, 'generated CI fixture matches its pin'
assert generated_hash not in {r['sha256'] for r in rows}, 'generated DNG is never a camera row'
fake = owned(scratch / 'fake')
for row in rows:
    folder = fake / row['set']
    folder.mkdir(parents=True, exist_ok=True)
    (folder / filename(row)).hardlink_to(generated)
assert all((fake / row['set'] / filename(row)).is_file() for row in rows), 'every generated impostor is present'
refusal = cli('verify', fake, '--set', 'twelve', ok=False).stderr
assert 'mismatch' in refusal, refusal
refusal = cli('verify', fake, '--set', 'hundred', ok=False).stderr
assert 'mismatch' in refusal, refusal
ricoh = next(r for r in twelve if r['suffix'] == 'dng')
pinned_generated = dict(ricoh, sha256=generated_hash, bytes=str(generated.stat().st_size))
local_url = scratch / 'local-url.tsv'
write_manifest(local_url, [dict(pinned_generated, url='file://' + str(generated))])
refusal = cli('list', using=local_url, ok=False).stderr
assert 'raw.pixls.us' in refusal, refusal
print('PASS bench-raw-corpus-contract: a generated-only corpus refuses against the camera manifest')
print('PASS bench-raw-corpus-contract: all offline checks passed')
PY
