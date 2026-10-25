#!/usr/bin/env bash
# Pins real CC0 corpus diversity and exercises fetch and verify without network access.
set -euo pipefail
repo=$(cd "$(dirname "$0")/.." && pwd -P)
mkdir -p "$repo/.superpowers/rawcorpus"
scratch=$(mktemp -d "$repo/.superpowers/rawcorpus/test.XXXXXXXX")
[[ -n "$scratch" && "$scratch" == /* ]] || exit 1
: > "$scratch/.flea-test-sandbox"
cleanup() {
    [[ -n "$scratch" && "$scratch" == "$repo"/.superpowers/rawcorpus/test.* && "$scratch" == /* && -f "$scratch/.flea-test-sandbox" ]] || exit 1
    rm -rf -- "$scratch"
}
trap cleanup EXIT
python3 -B - "$repo" "$scratch" "${RAW_CORPUS_MANIFEST:-$repo/tools/flea-raw-corpus.tsv}" <<'PY'
import contextlib
import csv
import hashlib
import io
import os
from pathlib import Path
import re
import runpy
import subprocess
import sys
from unittest.mock import patch
from urllib.request import Request

repo, scratch, manifest = map(Path, sys.argv[1:])
tool = repo / 'tools/flea-raw-corpus'
module = runpy.run_path(str(tool))
fields = module['FIELDS']
with manifest.open(newline='') as stream:
    reader = csv.DictReader(stream, delimiter='\t')
    assert reader.fieldnames == fields, 'manifest header'
    rows = list(reader)
twelve = [r for r in rows if r['set'] == 'twelve']
hundred = [r for r in rows if r['set'] == 'hundred']
assert len(rows) == 112 and len(twelve) == 12 and len(hundred) == 100, 'set counts'
assert len({r['suffix'] for r in twelve}) == 12, 'twelve distinct formats'
assert len({(r['make'].casefold(), r['model'].casefold()) for r in twelve}) == 12, 'twelve distinct cameras'
assert len({r['sha256'] for r in hundred}) == 100, 'hundred distinct hashes'
assert len({r['sha256'] for r in rows}) == 112, 'sets must not share files'
assert {r['suffix'] for r in hundred} == {r['suffix'] for r in twelve}, 'same twelve formats'
assert {'cr2', 'cr3', 'nef', 'arw', 'raf', 'orf', 'rw2', 'pef', 'dng', 'srw'} <= {r['suffix'] for r in twelve}, 'required formats'
assert all(r['url'].startswith('https://raw.pixls.us/') for r in rows), 'HTTPS source only'
assert all(r['licence'] == 'Creative Commons 0 - Public Domain' for r in rows), 'CC0 licence title'
assert all(re.fullmatch(r'[0-9a-f]{64}', r['sha256']) for r in rows), '64 hex sha256'
assert sum(int(r['bytes']) for r in rows) < 4_000_000_000, 'corpus size cap'
assert module['load_manifest'](manifest) == rows, 'schema, HTTPS, CC0 and hash syntax'
print('PASS raw-corpus: pinned 12 formats and 100 distinct CC0 originals')


def write_manifest(path, data):
    with path.open('w', newline='') as stream:
        writer = csv.DictWriter(stream, fieldnames=fields, delimiter='\t', lineterminator='\n')
        writer.writeheader()
        writer.writerows(data)


def owned(path):
    assert path.is_absolute() and path.is_relative_to(scratch)
    assert (scratch / '.flea-test-sandbox').is_file()
    return path


WRAPPER = ('import runpy,socket,sys; '
           'socket.socket.connect=lambda *a,**k:sys.exit("NETWORK FORBIDDEN"); '
           'socket.getaddrinfo=lambda *a,**k:sys.exit("NETWORK FORBIDDEN"); '
           'sys.argv=sys.argv[1:];runpy.run_path(sys.argv[0],run_name="__main__")')
# The guard must stop a name lookup too, which urllib makes before any connect.
lookup = owned(scratch / 'lookup.py')
lookup.write_text('import socket\nsocket.getaddrinfo("raw.pixls.us", 443)\nprint("DNS REACHED")\n')
probe = subprocess.run([sys.executable, '-B', '-c', WRAPPER, str(lookup)], capture_output=True, text=True, timeout=10)
assert 'NETWORK FORBIDDEN' in probe.stderr and 'DNS REACHED' not in probe.stdout, (probe.stdout, probe.stderr)


def cli(command, *args, ok=True, using=None):
    result = subprocess.run([sys.executable, '-B', '-c', WRAPPER, str(tool), command,
                             *map(str, args), '--manifest', str(using or mini)],
                            capture_output=True, text=True, timeout=10)
    assert (result.returncode == 0) == ok, (command, result.stdout, result.stderr)
    assert 'NETWORK FORBIDDEN' not in result.stderr, result.stderr
    return result


body = b'tiny offline corpus\n'
row = dict(set='twelve', suffix='dng', make='Test / Camera', model='../Model',
           url='https://raw.pixls.us/test.dng', sha256=hashlib.sha256(body).hexdigest(),
           bytes=str(len(body)), licence=module['LICENCE'], mime='image/x-adobe-dng')
mini = scratch / 'tiny.tsv'
write_manifest(mini, [row])
corpus = scratch / 'corpus'
folder = corpus / 'twelve'
folder.mkdir(parents=True)
target = owned(folder / module['filename'](row))
assert target.name.startswith('Test-Camera-Model-') and target.suffix == '.dng'
target.write_bytes(body)
cli('verify', corpus, '--set', 'twelve')
assert '1 files' in cli('verify', corpus, '--set', 'twelve').stdout
assert 'missing manifest rows' in cli('verify', corpus, '--set', 'all', ok=False).stderr
assert len(list(csv.DictReader(io.StringIO(cli('list', '--set', 'twelve').stdout), delimiter='\t'))) == 1
print('PASS raw-corpus: offline verify and list accept tiny manifest')

target.write_bytes(b'x' * len(body))
assert 'sha256 mismatch' in cli('verify', corpus, '--set', 'twelve', ok=False).stderr
owned(target).unlink()
cli('verify', corpus, '--set', 'twelve', ok=False)
target.write_bytes(body)
extra = owned(folder / 'extra.dng')
extra.write_bytes(body)
assert 'extra file' in cli('verify', corpus, '--set', 'twelve', ok=False).stderr
owned(extra).unlink()
(folder / '.ignored').write_bytes(b'allowed')
cli('verify', corpus, '--set', 'twelve')
extra = owned(corpus / 'extra.dng')
extra.touch()
cli('verify', corpus, '--set', 'twelve', ok=False)
owned(extra).unlink()
extra = owned(corpus / 'hundred')
extra.touch()
cli('verify', corpus, '--set', 'twelve', ok=False)
owned(extra).unlink()
owned(target).unlink()
payload = owned(scratch / 'matching-payload.dng')
payload.write_bytes(body)
target.symlink_to(payload)
symlink_failures = []
for command in ('verify', 'fetch'):
    try:
        refusal = cli(command, corpus, '--set', 'twelve', ok=False)
        assert str(target) in refusal.stderr, refusal.stderr
    except AssertionError as error:
        symlink_failures.append((command, str(error)))
assert not symlink_failures, symlink_failures
assert target.is_symlink() and payload.read_bytes() == body
owned(target).unlink()
os.mkfifo(owned(target))
assert 'not a regular file' in cli('fetch', corpus, '--set', 'twelve', ok=False).stderr
owned(target).unlink()
print('PASS raw-corpus: wrong hash, missing file, extra file and symlink fail')

bad_manifest = scratch / 'bad.tsv'
write_manifest(bad_manifest, [dict(row, url='http://raw.pixls.us/test.dng')])
refusal = cli('fetch', corpus, '--set', 'twelve', using=bad_manifest, ok=False)
assert 'URL must use HTTPS' in refusal.stderr
redirect = module['SafeRedirect']()
request = Request(row['url'])
for url in ('http://raw.pixls.us/test.dng', 'https://example.org/test.dng', 'file:///etc/passwd'):
    try:
        redirect.redirect_request(request, None, 302, 'Found', {}, url)
    except ValueError:
        pass
    else:
        raise AssertionError('unsafe redirect accepted: ' + url)
assert redirect.redirect_request(request, None, 302, 'Found', {}, row['url']).full_url == row['url']
print('PASS raw-corpus: non-HTTPS fetch and unsafe redirects refused offline')


class Response(io.BytesIO):
    status = 200

    def geturl(self):
        return row['url']


class Opener:
    def __init__(self, data):
        self.data, self.calls = data, 0

    def open(self, request, timeout):
        assert timeout == 30 and request.full_url == row['url']
        assert not target.exists() or target.read_bytes() == b'old'
        self.calls += 1
        return Response(self.data)


fetch = module['fetch']
namespace = fetch.__globals__
opener = Opener(body)
with patch.dict(namespace, build_opener=lambda *a: opener), contextlib.redirect_stdout(io.StringIO()):
    fetch(corpus, [row])
    assert target.read_bytes() == body and opener.calls == 1
    fetch(corpus, [row])
    assert opener.calls == 1, 'valid existing file must skip network'
for bad in (b'x' * len(body), body[:-1], body + b'overflow'):
    target.write_bytes(b'old')
    with patch.dict(namespace, build_opener=lambda *a: Opener(bad)):
        try:
            fetch(corpus, [row])
        except ValueError:
            pass
        else:
            raise AssertionError('bad download accepted')
    assert target.read_bytes() == b'old', 'failed fetch overwrote destination'
    assert not list(corpus.glob('.raw-*.tmp')), 'failed fetch leaked temporary file'
print('PASS raw-corpus: fetch checks bytes and hash before rename, cleans failures and skips valid files')
print('PASS raw-corpus: all offline checks passed')
PY
