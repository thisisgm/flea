"""Exercise Linux discovery with real containers under isolated roots; generate WOFF without dependencies."""
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import zlib

root = Path(sys.argv[1])
fixtures = root / 'formats'
fixtures.mkdir()
env = {**os.environ, 'HOME': str(root / 'formats-home'), 'XDG_DATA_HOME': str(root / 'formats-data'), 'XDG_CACHE_HOME': str(root / 'formats-cache')}
binary = str(Path('target/debug/flea').resolve())
fonts = subprocess.check_output(['fc-list', '-f', '%{file}\n'], text=True).splitlines()
ttf = next(Path(p) for p in fonts if p.endswith('.ttf'))
data = ttf.read_bytes()
count = struct.unpack_from('>H', data, 4)[0]
records, blocks, offset = [], [], 44 + count * 20
sfnt_size = 12 + count * 16
for i in range(count):
    tag, checksum, start, length = struct.unpack_from('>4sIII', data, 12 + 16 * i)
    raw = data[start:start + length]
    compressed = zlib.compress(raw)
    block = compressed if len(compressed) < length else raw
    records.append(struct.pack('>4sIIII', tag, offset, len(block), length, checksum))
    block += b'\0' * (-len(block) % 4)
    blocks.append(block)
    offset += len(block)
    sfnt_size += (length + 3) & ~3
header = struct.pack('>4s4sIHHIHHIIIII', b'wOFF', data[:4], offset, count, 0, sfnt_size, 1, 0, 0, 0, 0, 0, 0)
woff = fixtures / 'generated.woff'
woff.write_bytes(header + b''.join(records) + b''.join(blocks))
bdf = fixtures / 'generated.bdf'
bdf_header = """STARTFONT 2.1
FONT -flea-FleaBitmapTest-medium-r-normal--8-80-75-75-c-80-iso10646-1
SIZE 8 75 75
FONTBOUNDINGBOX 8 8 0 0
STARTPROPERTIES 7
FAMILY_NAME "Flea Bitmap Test"
FONT_ASCENT 8
FONT_DESCENT 0
DEFAULT_CHAR 65
PIXEL_SIZE 8
CHARSET_REGISTRY "ISO10646"
CHARSET_ENCODING "1"
ENDPROPERTIES
CHARS 95
"""
bdf_glyphs = []
for code in range(32, 127):
    bdf_glyphs.append(f"STARTCHAR char{code}\nENCODING {code}\nSWIDTH 500 0\nDWIDTH 8 0\nBBX 8 8 0 0\nBITMAP\n18\n24\n42\n7E\n42\n42\n42\n00\nENDCHAR\n")
bdf.write_text(bdf_header + ''.join(bdf_glyphs) + 'ENDFONT\n')
candidates = [woff, bdf]
for suffix in ('.ttf', '.otf', '.ttc', '.pcf.gz', '.pfa', '.pfb', '.bdf', '.fon', '.fnt'):
    match = next((Path(p) for p in fonts if p.endswith(suffix)), None)
    if match:
        copied = fixtures / ('fixture' + suffix)
        shutil.copyfile(match, copied)
        candidates.append(copied)
for match in list(Path('/usr/share/fonts').rglob('*.woff2'))[:1]:
    copied = fixtures / 'fixture.woff2'
    shutil.copyfile(match, copied)
    candidates.append(copied)
variables = subprocess.check_output(['fc-list', ':variable=true', '-f', '%{file}\n'], text=True).splitlines()
if variables:
    variable = fixtures / 'variable.ttf'
    shutil.copyfile(variables[0], variable)
    candidates.append(variable)
else:
    print('font-formats: no variable fixture on host')
# Invalid, metrics-only and Office containers must never be installed by a renamed extension.
for name in ['fake.eot', 'metrics.afm', 'office.odf']:
    bad = fixtures / name
    bad.write_text('not a glyph-bearing font')
    result = subprocess.run([binary, '--font', 'install', str(bad)], env=env, capture_output=True, text=True)
    assert result.returncode == 2 and 'error' in json.loads(result.stdout)
cases = []
for source in candidates:
    before = source.read_bytes()
    info_run = subprocess.run([binary, '--font', 'info', str(source)], env=env, capture_output=True, text=True)
    info = json.loads(info_run.stdout)
    assert info_run.returncode == 0, (source, info)
    assert info['faces'], (source, info)
    if source.name == 'variable.ttf':
        assert any(info['axes'].values()), info
    installed = subprocess.run([binary, '--font', 'install', str(source)], env=env, capture_output=True, text=True, timeout=45)
    result = json.loads(installed.stdout)
    if installed.returncode:
        assert 'rolled back' in result['error'], (source, result)
        assert not Path(info['location']).exists(), (source, result)
        print('font-formats:', source.name, 'desktop configuration rejected; rollback verified')
    else:
        location = result['location']
        listed = subprocess.check_output(['fc-list', '-f', '%{file}\n'], env=env, text=True).splitlines()
        assert location in listed, (source, result)
        assert Path(location).read_bytes() == before, 'collections and variation data must survive unchanged'
        removed = subprocess.run([binary, '--font', 'uninstall', str(source)], env=env, capture_output=True, text=True, timeout=45)
        assert removed.returncode == 0 and not Path(location).exists(), removed.stdout
        print('font-formats:', source.name, info['format'], len(info['faces']), 'face(s), desktop install/uninstall verified')
    assert source.read_bytes() == before
    cases.append(str(source))
(root / 'format-cases.json').write_text(json.dumps(cases))

# A host may reject bitmap, legacy or all user fonts: never claim success from the copy alone.
reject = root / 'reject-fonts.conf'
reject.write_text('<fontconfig><include>/etc/fonts/fonts.conf</include><selectfont><rejectfont><glob>*/fonts/*</glob></rejectfont></selectfont></fontconfig>')
blocked_env = {**env, 'FONTCONFIG_FILE': str(reject)}
result = subprocess.run([binary, '--font', 'install', str(woff)], env=blocked_env, capture_output=True, text=True)
assert result.returncode == 2 and 'rolled back' in json.loads(result.stdout)['error'], result.stdout
info = json.loads(subprocess.check_output([binary, '--font', 'info', str(woff)], env=env, text=True))
assert not Path(info['location']).exists(), 'a rejected install must leave no font behind'
existing = Path(env['XDG_DATA_HOME']) / 'fonts' / 'existing.ttf'
existing.parent.mkdir(parents=True, exist_ok=True)
existing.write_bytes(ttf.read_bytes())
result = subprocess.run([binary, '--font', 'install', str(existing)], env=blocked_env, capture_output=True, text=True)
assert result.returncode == 2 and existing.read_bytes() == ttf.read_bytes(), result.stdout
print('font-formats: rejected desktop install rolled back; existing user font preserved')
