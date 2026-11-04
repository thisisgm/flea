"""Validate repository scene IDs and pair native geometry with an unchanged PNG."""
import argparse
import copy
import hashlib
import html
import json
import math
from pathlib import Path
import re
import struct
import sys
import time

ROOT = Path(__file__).resolve().parent
# A language scene names a code of the pinned Omarchy roster the core reads, so the two lists cannot drift.
# Sample: {"en": {"name": "English", ...}, "de": {"name": "Deutsch", ...}}, keyed by code.
ROSTER = ROOT.parents[1] / 'src/i18n/fixtures/locales.json'
# The fields only a language scene carries, checked against schema.json's language_scenes.
LANGUAGE_FIELDS = {'language', 'surface', 'state'}
# Board columns.png sizes only the active track; the id's last field is Slot::Active.index() in crates/flea-gui/src/columns/geometry.rs.
COLUMNS_ACTIVE_TRACK = re.compile(r'columns-[01]-3')
# The card scenes that carry no reference pair, each bound to the board ci/visual/g2/op1.json names for it.
CARD_BOARDS = {'g2.op1.copyto.r8': 'copyto', 'g2.op1.copyto.r0': 'copyto',
               'g2.op1.copyto-empty.r8': 'copyto-empty', 'g2.op1.copyto-empty.r0': 'copyto-empty'}
# Startup presents frames no backend request counts, and one after the receipt voids it; this readiness wait accepts nothing.
QUIET_SECONDS = 1
QUIET_LIMIT = 20


def require(condition, message):
    if not condition:
        raise ValueError(message)


def wait_quiet(read):
    previous = read('presentedFrame')
    for attempt in range(QUIET_LIMIT):
        time.sleep(QUIET_SECONDS)
        current, pending = read('presentedFrame'), read('pendingRequests')
        if current == previous and pending == '0':
            return
        moved, previous = current != previous, current
    state = 'moved to' if moved else 'held at'
    raise ValueError(f'native startup not quiet after {QUIET_LIMIT * QUIET_SECONDS} s: presentedFrame {state} {current}, pendingRequests {pending}')


def manifest(path):
    path = Path(path)
    if path.is_dir():
        rows = [row for source in sorted(path.glob('*.json')) for row in manifest(source)]
        require(rows and len({row['id'] for row in rows}) == len(rows), 'empty or duplicate scene registry')
        return rows
    schema = json.loads((ROOT / 'schema.json').read_text())
    data = json.loads(Path(path).read_text())
    require(data.get('version') == 1 and data.get('scenes'), 'missing scene manifest')
    seen = set()
    family = schema['language_scenes']
    codes = json.loads(ROSTER.read_text())
    # Types, safe IDs and registry membership only; each scene's own values belong to its manifest.
    for scene in data['scenes']:
        require(isinstance(scene, dict), 'missing or unknown scene field')
        # Only a card scene names its board alone; every listing scene keeps the v0.3.12 reference pair.
        paired = 'reference' in scene
        require(paired == ('reference_tag' in scene), 'reference needs its tag')
        want = set(schema['required'])
        # A language scene is one of a generated family: a pattern rather than a listed id, plus its language fields.
        language = isinstance(scene.get('id'), str) and scene['id'].startswith(family['prefix'])
        if language:
            kind = re.fullmatch(family['pattern'], scene['id'])
            require(kind, 'unknown scene id')
            want |= LANGUAGE_FIELDS
        if not paired:
            require(isinstance(scene.get('id'), str) and CARD_BOARDS.get(scene.get('id')) == scene.get('board'), 'card scene must name its own board')
            want -= {'reference', 'reference_tag'}
        require(set(scene) == want, 'missing or unknown scene field')
        for key in ('id', 'unit', 'board', 'fixture', 'theme'):
            require(isinstance(scene[key], str) and re.fullmatch(r'[A-Za-z0-9_.-]+', scene[key]), f'unsafe {key}')
        require(language or scene['id'] in schema['scene_ids'], 'unknown scene id')
        if language:
            require(isinstance(scene['language'], str) and scene['language'] in codes, 'unknown language')
            require(scene['surface'] in family['surfaces'] and scene['state'] in family['states'], 'unknown surface or state')
        require(scene['id'] not in seen, 'duplicate scene id')
        seen.add(scene['id'])
        require(scene['board'] in schema['boards'], 'unknown board id')
        if paired:
            require(scene['reference'] in schema['references'], 'unknown reference id')
            require(isinstance(scene['reference_tag'], str) and re.fullmatch(r'[0-9a-f]{8,40}', scene['reference_tag']), 'unsafe reference tag')
        require(scene['view'] in schema['views'] and scene['density'] in schema['densities'], 'unknown view or density')
        for key, low, high in (('width', 1, 2560), ('height', 1, 1440), ('text_size', 8, 32), ('corner_radius', 0, 64), ('cursor', 0, 1 << 31)):
            require(type(scene[key]) is int and low <= scene[key] <= high, f'invalid {key}')
        require(isinstance(scene['selection'], list) and all(type(n) is int and n >= 0 for n in scene['selection'])
                and len(set(scene['selection'])) == len(scene['selection']), 'invalid selection')
        require(type(scene['reduced_motion']) is bool and type(scene['caret_blink']) is bool, 'invalid motion settings')
        for key in ('decisions', 'changes'):
            require(isinstance(scene[key], list) and all(value in schema[key] for value in scene[key]), f'unknown {key[:-1]} id')
        require(scene['changes'], 'missing changes')
        require(isinstance(scene['covers'], list) and scene['covers']
                and all(isinstance(v, str) and re.fullmatch(r'[a-z]+:[A-Za-z0-9_.|-]+', v) for v in scene['covers']), 'missing covers')
        if language:
            # The id spells its kind, language, surface, state, theme and rounding, so no two of those captures share an id.
            spelled = f"i50.{kind[1]}.{scene['language']}.{scene['surface']}.{scene['state']}.{scene['theme']}.r{scene['corner_radius']}"
            require(scene['id'] == spelled, 'language scene id does not spell its fields')
    return data['scenes']


def validate(sidecar, receipt, png):
    require(sidecar.get('receipt') == receipt, 'sidecar receipt mismatch')
    require(type(receipt.get('frame')) is int and receipt['frame'] > 0 and type(receipt.get('window')) is int and receipt['window'] > 0 and receipt.get('scale') == 1, 'missing presented frame or app scale')
    with Path(png).open('rb') as image:
        header = image.read(24)
    require(len(header) == 24 and header[:8] == b'\x89PNG\r\n\x1a\n' and struct.unpack('>II', header[16:24]) == (receipt['width'], receipt['height']), 'PNG rect mismatch')
    nodes = sidecar.get('roles', [])
    require(nodes, 'empty sidecar')
    ids = {node['id']: node for node in nodes}
    require(len(ids) == len(nodes), 'duplicate native role id')
    registered = {scene['id']: scene['view'] for scene in manifest(ROOT / 'scenes')}
    view = registered.get(receipt.get('scene'))
    require(view is not None and receipt.get('view', view) == view, 'native view mismatch')
    required = {'surface', 'row', 'glyph', 'ink', 'label'}
    # Board grid.png captions carry no size, so a grid row has no value role.
    if view != 'grid':
        required.add('value')
    require(required <= {node['role'] for node in nodes}, 'missing native roles')
    for node in nodes:
        require(node['role'] in ('surface', 'row', 'glyph', 'ink', 'label', 'value', 'check', 'hint'), 'unknown native role')
        require(node['scale'] == 1 and isinstance(node['text'], str), 'invalid role text or scale')
        for key in ('rect', 'pixel_rect'):
            require(len(node[key]) == 4 and all(type(n) in (int, float) and math.isfinite(n) for n in node[key]), 'invalid native rect')
        lx, ly, lw, lh = node['rect']
        require(lw > 0 and lh > 0, 'empty native layout rect')
        x, y, w, h = node['pixel_rect']
        expected = (max(0, lx), max(0, ly), min(receipt['width'], lx + lw) - max(0, lx), min(receipt['height'], ly + lh) - max(0, ly))
        require(all(abs(a - b) < .001 for a, b in zip((x, y, w, h), expected)), 'native pixel rect does not clip its layout rect')
        require(w > 0 and h > 0 and x >= 0 and y >= 0 and x + w <= receipt['width'] and y + h <= receipt['height'], 'native rect outside viewport')
        parent = node['parent']
        require((node['role'] == 'surface' and not parent) or parent in ids, 'missing native parent')
        if node['role'] == 'row':
            require(ids[parent]['role'] == 'surface', 'row without surface')
            roles = {n['role'] for n in nodes if n['parent'] == node['id']}
            row_roles = {'glyph', 'label'}
            if view in ('list', 'dual') or (view == 'columns' and COLUMNS_ACTIVE_TRACK.fullmatch(parent)):
                row_roles.add('value')
            require(row_roles <= roles, 'incomplete row roles')
    return ids


def wrapper(sidecar, receipt, png, output):
    ids = validate(sidecar, receipt, png)
    children = {}
    for node in ids.values():
        children.setdefault(node['parent'], []).append(node)

    def element(node, origin=(0, 0)):
        x, y, w, h = node['rect']
        role = node['role']
        tag = 'img' if role == 'ink' else 'div'
        attr = '' if role == 'ink' else f' data-flea="{role}"'
        style = f'position:absolute;left:{x-origin[0]}px;top:{y-origin[1]}px;width:{w}px;height:{h}px;'
        content = ''.join(element(child, (x, y)) for child in children.get(node['id'], []))
        if tag == 'img':
            return f'<img alt="" src="data:image/svg+xml,%3Csvg xmlns=\'http://www.w3.org/2000/svg\'/%3E" style="{style}">'
        return f'<div{attr} style="{style}" title="{html.escape(node["text"], quote=True)}">{content}</div>'

    digest = hashlib.sha256(Path(png).read_bytes()).hexdigest()
    paired = dict(sidecar, png_sha256=digest)
    Path(output).with_suffix('.json').write_text(json.dumps(paired, indent=2) + '\n')
    encoded = __import__('base64').b64encode(Path(png).read_bytes()).decode()
    body = ''.join(element(node) for node in children.get('', []))
    Path(output).write_text(f'<!doctype html><meta charset="utf-8"><style>body{{margin:0;width:{receipt["width"]}px;height:{receipt["height"]}px;overflow:hidden}}div{{box-sizing:border-box}}</style><img alt="captured frame" style="position:absolute" src="data:image/png;base64,{encoded}">{body}')
    return digest


def controls(path):
    require((Path(path).parent / ".flea-test-sandbox").is_file(), "unmarked control root")
    original = json.loads(Path(path).read_text())
    for name, mutate in [('absolute-path', lambda s: s.update(board='/outside')),
                         ('unknown-id', lambda s: s.update(id='unknown')),
                         ('missing-field', lambda s: s.pop('fixture'))]:
        data = copy.deepcopy(original)
        mutate(data['scenes'][0])
        control = Path(path).parent / f'.control-{name}.json'
        control.write_text(json.dumps(data))
        try:
            manifest(control)
        except ValueError:
            print(f'GPUI_SCENE_SCHEMA_CONTROL name={name} rejected=true')
        else:
            raise ValueError(f'accepted {name}')
        finally:
            require(control.is_absolute() and control.name.startswith('.control-'), 'unsafe control cleanup')
            control.unlink()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('command', choices=['check', 'list', 'wrapper', 'controls'])
    parser.add_argument('path', type=Path)
    parser.add_argument('--receipt', type=Path)
    parser.add_argument('--png', type=Path)
    parser.add_argument('--out', type=Path)
    args = parser.parse_args()
    if args.command == 'wrapper':
        print(wrapper(json.loads(args.path.read_text()), json.loads(args.receipt.read_text()), args.png, args.out))
    elif args.command == 'controls':
        controls(args.path)
    else:
        scenes = manifest(args.path)
        if args.command == 'list':
            for scene in scenes:
                print(scene['id'])
        else:
            print(f'GPUI_SCENE_SCHEMA PASS scenes={len(scenes)}')


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, KeyError, TypeError) as error:
        print(f'GPUI_SCENE_REJECT {error}', file=sys.stderr)
        raise SystemExit(1)
