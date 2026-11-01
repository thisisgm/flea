"""Exercise native sidecar boundaries independently of a running compositor."""
import copy
import json
from pathlib import Path
import re
import shutil
import struct
import tempfile
import unittest
from unittest import mock
import zlib

import scenes


class Sidecars(unittest.TestCase):
    def setUp(self):
        parent = Path(__file__).resolve().parents[2] / '.superpowers/g010v'
        parent.mkdir(parents=True, exist_ok=True)
        self.root = Path(tempfile.mkdtemp(prefix='sidecar.', dir=parent))
        (self.root / '.flea-test-sandbox').touch()
        self.addCleanup(self.remove_root)
        self.image = self.root / 'frame.png'
        chunk = lambda kind, data: struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data))
        self.image.write_bytes(b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', 1100, 700, 8, 2, 0, 0, 0)) + chunk(b'IDAT', zlib.compress((b'\0' + bytes([240, 240, 240]) * 1100) * 700)) + chunk(b'IEND', b''))
        self.receipt = dict(scene='g1-01-list-r0', window=42, frame=3, scale=1, width=1100, height=700)
        def node(key, parent, role, rect, text=''):
            return dict(id=key, parent=parent, role=role, rect=rect, pixel_rect=rect, text=text, scale=1)
        self.data = dict(receipt=self.receipt, roles=[
            node('list', '', 'surface', [192, 54, 908, 619]),
            node('row-0', 'list', 'row', [192, 54, 908, 31]),
            node('glyph-0', 'row-0', 'glyph', [206, 54, 23, 31]),
            node('ink-0', 'glyph-0', 'ink', [209, 61, 17, 17]),
            node('label-0', 'row-0', 'label', [238, 61, 70, 17], 'alpha.txt'),
            node('value-0', 'row-0', 'value', [900, 61, 30, 17], '4 B'),
        ])

    def remove_root(self):
        root = self.root
        if root.is_absolute() and root.name.startswith('sidecar.') and (root / '.flea-test-sandbox').is_file():
            shutil.rmtree(root)

    def test_wrapper_uses_native_positions_and_keeps_original_png(self):
        before = self.image.read_bytes()
        out = self.root / 'native.html'
        digest = scenes.wrapper(self.data, self.receipt, self.image, out)
        self.assertEqual(before, self.image.read_bytes())
        text = out.read_text()
        self.assertIn('left:192px;top:54px;width:908px;height:619px', text)
        self.assertIn('data-flea="glyph"', text)
        self.assertEqual(json.loads(out.with_suffix('.json').read_text())['png_sha256'], digest)

    def test_empty_stale_wrong_scale_and_outside_viewport_sidecars_fail(self):
        for mutate, error in [
            (lambda data: data.update(roles=[]), 'empty sidecar'),
            (lambda data: data['receipt'].update(frame=2), 'sidecar receipt mismatch'),
            (lambda data: data['roles'][1].update(scale=2), 'invalid role text or scale'),
            (lambda data: data['roles'][1].update(pixel_rect=[-1, 0, 31, 31]), 'native pixel rect does not clip its layout rect'),
            (lambda data: data['roles'].pop(4), 'missing native roles'),
        ]:
            data = copy.deepcopy(self.data)
            mutate(data)
            with self.assertRaisesRegex(ValueError, error):
                scenes.validate(data, self.receipt, self.image)

    def test_grid_captions_have_no_trailing_value_on_the_board(self):
        data = copy.deepcopy(self.data)
        receipt = dict(self.receipt, scene='g1-01-grid-r0', view='grid')
        data['receipt'] = receipt
        data['roles'] = [row for row in data['roles'] if row['role'] != 'value']
        scenes.validate(data, receipt, self.image)
        for role in ('glyph', 'ink', 'label'):
            broken = copy.deepcopy(data)
            broken['roles'] = [row for row in broken['roles'] if row['role'] != role]
            with self.assertRaisesRegex(ValueError, 'missing native roles'):
                scenes.validate(broken, receipt, self.image)
        data['receipt'] = self.receipt
        with self.assertRaisesRegex(ValueError, 'missing native roles'):
            scenes.validate(data, self.receipt, self.image)

    def test_a_list_receipt_cannot_claim_grid_to_omit_its_values(self):
        data = copy.deepcopy(self.data)
        receipt = dict(self.receipt, view='grid')
        data['receipt'] = receipt
        data['roles'] = [row for row in data['roles'] if row['role'] != 'value']
        with self.assertRaisesRegex(ValueError, 'native view mismatch'):
            scenes.validate(data, receipt, self.image)

    def test_columns_require_sizes_on_the_active_track_only(self):
        data = copy.deepcopy(self.data)
        receipt = dict(self.receipt, scene='g1-01-columns-r0', view='columns')
        data['receipt'] = receipt
        data['roles'][0]['id'] = 'columns-0-3'
        data['roles'][1]['parent'] = 'columns-0-3'
        peek = dict(data['roles'][0], id='columns-0-2')
        data['roles'].append(peek)
        for row in copy.deepcopy(data['roles'][1:6]):
            row['id'] += '-peek'
            row['parent'] = 'columns-0-2' if row['role'] == 'row' else row['parent'] + '-peek'
            if row['role'] != 'value':
                data['roles'].append(row)
        scenes.validate(data, receipt, self.image)
        second = copy.deepcopy(data['roles'][1:6])
        for row in second:
            row['id'] += '-next'
            if row['role'] != 'row':
                row['parent'] += '-next'
            if row['role'] != 'value':
                data['roles'].append(row)
        with self.assertRaisesRegex(ValueError, 'incomplete row roles'):
            scenes.validate(data, receipt, self.image)
        data['roles'] = [row for row in data['roles'] if not row['id'].endswith('-next') and row['role'] != 'value']
        with self.assertRaisesRegex(ValueError, 'missing native roles'):
            scenes.validate(data, receipt, self.image)

    def test_the_active_track_id_follows_the_painter_slot_order(self):
        geometry = (Path(__file__).resolve().parents[2] / 'crates/flea-gui/src/columns/geometry.rs').read_text()
        active = re.search(r'Self::Active => (\d+),', geometry)
        self.assertIsNotNone(active, 'Slot::index() no longer names Active')
        for side in ('0', '1'):
            self.assertTrue(scenes.COLUMNS_ACTIVE_TRACK.fullmatch(f'columns-{side}-{active.group(1)}'))
        for slot in {'0', '1', '2', '3', '4'} - {active.group(1)}:
            self.assertIsNone(scenes.COLUMNS_ACTIVE_TRACK.fullmatch(f'columns-0-{slot}'))

    def test_registered_manifest_and_its_rejection_controls(self):
        source = Path(__file__).with_name('scenes') / 'g1-01.json'
        self.assertEqual(len(scenes.manifest(source)), 8)
        target = self.root / 'manifest.json'
        target.write_bytes(source.read_bytes())
        scenes.controls(target)

    def test_listing_scene_without_reference_pair_fails(self):
        source = Path(__file__).with_name('scenes') / 'g1-01.json'
        data = json.loads(source.read_text())
        data['scenes'][3].pop('reference')
        data['scenes'][3].pop('reference_tag')
        target = self.root / 'unpaired.json'
        target.write_text(json.dumps(data))
        with self.assertRaisesRegex(ValueError, 'card scene must name its own board'):
            scenes.manifest(target)

    def test_non_object_scenes_fail_with_validation_error(self):
        target = self.root / 'non-object.json'
        for scene in (None, 'x'):
            with self.subTest(scene=scene):
                target.write_text(json.dumps(dict(version=1, scenes=[scene])))
                with self.assertRaisesRegex(ValueError, 'missing or unknown scene field'):
                    scenes.manifest(target)

    def test_listing_id_cannot_borrow_a_card_board_reference_exemption(self):
        source = Path(__file__).with_name('scenes') / 'g1-01.json'
        listing = next(scene for scene in scenes.manifest(source) if scene['id'] == 'g1-01-grid-r8')
        target = self.root / 'wrong-card.json'
        for board in ('copyto', 'copyto-empty'):
            with self.subTest(board=board):
                scene = dict(listing, board=board)
                scene.pop('reference')
                scene.pop('reference_tag')
                target.write_text(json.dumps(dict(version=1, scenes=[scene])))
                with self.assertRaisesRegex(ValueError, 'card scene must name its own board'):
                    scenes.manifest(target)

    def test_another_unit_registers_its_own_scene_values(self):
        source = Path(__file__).with_name('scenes') / 'g1-01.json'
        schema = json.loads(Path(scenes.__file__).with_name('schema.json').read_text())
        schema['scene_ids'].append('g2-xx-selection')
        schema['boards'].append('cols-preview-image')
        schema['references'].append('cols-preview-image-1100x700')
        (self.root / 'schema.json').write_text(json.dumps(schema))
        data = json.loads(source.read_text())
        data['scenes'] = [dict(data['scenes'][2], id='g2-xx-selection', unit='G2.07', board='cols-preview-image',
                               reference='cols-preview-image-1100x700', reference_tag='0123abcd', view='columns',
                               selection=[2, 4], cursor=4, text_size=16, width=2560, height=1400, fixture='images',
                               density='comfortable', covers=['board:cols-preview-image'])]
        target = self.root / 'unit.json'
        target.write_text(json.dumps(data))
        root, scenes.ROOT = scenes.ROOT, self.root
        try:
            self.assertEqual(scenes.manifest(target)[0]['selection'], [2, 4])
            for key, value in [('selection', [-1]), ('cursor', '4'), ('view', 'tree'), ('reference_tag', '../x')]:
                bad = copy.deepcopy(data)
                bad['scenes'][0][key] = value
                target.write_text(json.dumps(bad))
                with self.assertRaises(ValueError):
                    scenes.manifest(target)
        finally:
            scenes.ROOT = root

    def test_card_scenes_validate_only_on_their_own_board(self):
        source = Path(__file__).with_name('scenes') / 'g2-op1.json'
        cards = scenes.manifest(source)
        self.assertEqual(len(cards), 4)
        target = self.root / 'card.json'
        for card in cards:
            target.write_text(json.dumps(dict(version=1, scenes=[card])))
            self.assertEqual(scenes.manifest(target)[0]['id'], card['id'])
            for board in ('list', 'grid', 'copyto', 'copyto-empty'):
                if board == card['board']:
                    continue
                with self.subTest(scene=card['id'], board=board):
                    target.write_text(json.dumps(dict(version=1, scenes=[dict(card, board=board)])))
                    with self.assertRaisesRegex(ValueError, 'card scene must name its own board'):
                        scenes.manifest(target)

    def test_a_card_scene_still_fails_the_field_set_check(self):
        card = scenes.manifest(Path(__file__).with_name('scenes') / 'g2-op1.json')[0]
        target = self.root / 'extra.json'
        target.write_text(json.dumps(dict(version=1, scenes=[dict(card, extra=1)])))
        with self.assertRaisesRegex(ValueError, 'missing or unknown scene field'):
            scenes.manifest(target)

    def test_unhashable_card_id_fails_with_validation_error(self):
        card = scenes.manifest(Path(__file__).with_name('scenes') / 'g2-op1.json')[0]
        target = self.root / 'unhashable.json'
        for bad in (['g2.op1.copyto.r8'], {'id': 'g2.op1.copyto.r8'}):
            with self.subTest(id=bad):
                scene = dict(card, id=bad)
                scene.pop('reference', None)
                scene.pop('reference_tag', None)
                target.write_text(json.dumps(dict(version=1, scenes=[scene])))
                with self.assertRaisesRegex(ValueError, 'card scene must name its own board'):
                    scenes.manifest(target)

    def language_scene(self, **changes):
        source = Path(__file__).with_name('scenes') / 'g1-01.json'
        listing = next(scene for scene in scenes.manifest(source) if scene['id'] == 'g1-01-list-r0')
        scene = dict(listing, id='i50.catalog.de.list.rest.gm.r0', unit='I50-27', board='lang-ja-list', theme='gm',
                     language='de', surface='list', state='rest', covers=['board:lang-ja-list'])
        scene.update(changes)
        return {key: value for key, value in scene.items() if value is not None}

    def test_a_language_scene_names_a_roster_language_surface_and_state(self):
        target = self.root / 'i50.json'
        target.write_text(json.dumps(dict(version=1, scenes=[self.language_scene()])))
        self.assertEqual(scenes.manifest(target)[0]['language'], 'de')
        for changes, error in [
            (dict(language='xx', id='i50.catalog.xx.list.rest.gm.r0'), 'unknown language'),
            (dict(language=['de']), 'unknown language'),
            (dict(surface=['list']), 'unknown surface or state'),
            (dict(language=None), 'missing or unknown scene field'),
            (dict(surface='tray', id='i50.catalog.de.tray.rest.gm.r0'), 'unknown surface or state'),
            (dict(state='blink', id='i50.catalog.de.list.blink.gm.r0'), 'unknown surface or state'),
            (dict(state=None), 'missing or unknown scene field'),
            (dict(id='i50.catalog.de.list.rest.gm.r0/../x'), 'unknown scene id'),
            (dict(id='i50.'), 'unknown scene id'),
            (dict(id='i50.paint.de.list.rest.gm.r0'), 'unknown scene id'),
        ]:
            with self.subTest(changes=changes):
                target.write_text(json.dumps(dict(version=1, scenes=[self.language_scene(**changes)])))
                with self.assertRaisesRegex(ValueError, error):
                    scenes.manifest(target)

    def test_a_language_scene_id_spells_its_fields(self):
        target = self.root / 'i50.json'
        for changes in [dict(language='ja'), dict(surface='menu'), dict(state='focus'), dict(theme='low-chroma'),
                        dict(corner_radius=8), dict(id='i50.catalog.de.list.rest.gm.r0.extra')]:
            with self.subTest(changes=changes):
                target.write_text(json.dumps(dict(version=1, scenes=[self.language_scene(**changes)])))
                with self.assertRaisesRegex(ValueError, 'language scene id does not spell its fields'):
                    scenes.manifest(target)
        spelled = self.language_scene(id='i50.fit.ja.menu.focus.low-chroma.r8', language='ja', surface='menu', state='focus',
                                      theme='low-chroma', corner_radius=8)
        target.write_text(json.dumps(dict(version=1, scenes=[spelled])))
        self.assertEqual(scenes.manifest(target)[0]['id'], 'i50.fit.ja.menu.focus.low-chroma.r8')

    def test_language_fields_belong_only_to_language_scenes(self):
        source = Path(__file__).with_name('scenes') / 'g1-01.json'
        target = self.root / 'g1.json'
        for key, value in (('language', 'de'), ('surface', 'list'), ('state', 'rest')):
            with self.subTest(key=key):
                data = json.loads(source.read_text())
                data['scenes'][0][key] = value
                target.write_text(json.dumps(data))
                with self.assertRaisesRegex(ValueError, 'missing or unknown scene field'):
                    scenes.manifest(target)
        bare = self.language_scene()
        for key in ('language', 'surface', 'state'):
            bare.pop(key)
        target.write_text(json.dumps(dict(version=1, scenes=[bare])))
        with self.assertRaisesRegex(ValueError, 'missing or unknown scene field'):
            scenes.manifest(target)

    def test_the_language_registry_holds_only_drawable_spelled_scenes(self):
        registered = scenes.manifest(Path(__file__).with_name('scenes') / 'i50.json')
        # The I50 fit languages; only the listing at rest is drawable until each surface owner adds a drawn-state readback.
        languages = ['de', 'ar', 'ja', 'zh-CN', 'th', 'hi']
        self.assertEqual(sorted(scene['id'] for scene in registered),
                         sorted(f'i50.fit.{code}.list.rest.{theme}.r{radius}' for code in languages
                                for theme in ('gm', 'low-chroma') for radius in (0, 8)))
        self.assertEqual({(scene['surface'], scene['state'], scene['view']) for scene in registered}, {('list', 'rest', 'list')})
        # Arabic draws mirrored on its own board; every other fit language uses the left-to-right list board.
        self.assertEqual({(scene['language'], scene['board']) for scene in registered},
                         {(code, 'lang-ar-list' if code == 'ar' else 'lang-ja-list') for code in languages})


class Quiet(unittest.TestCase):
    def wait(self, frames, pending):
        replies = {'presentedFrame': iter(frames), 'pendingRequests': iter(pending)}
        with mock.patch.object(scenes.time, 'sleep') as sleep:
            scenes.wait_quiet(lambda function: next(replies[function]))
        return sleep.call_count

    def test_a_still_frame_with_no_pending_request_is_quiet(self):
        self.assertEqual(self.wait(['4', '5', '5'], ['0', '0']), 2)

    def test_a_still_frame_blames_the_pending_request(self):
        frames = ['5'] * (scenes.QUIET_LIMIT + 1)
        with self.assertRaisesRegex(ValueError, r'presentedFrame held at 5, pendingRequests 1$'):
            self.wait(frames, ['1'] * scenes.QUIET_LIMIT)

    def test_a_moving_frame_is_named(self):
        frames = [str(frame) for frame in range(scenes.QUIET_LIMIT + 1)]
        with self.assertRaisesRegex(ValueError, rf'presentedFrame moved to {scenes.QUIET_LIMIT}, pendingRequests 0$'):
            self.wait(frames, ['0'] * scenes.QUIET_LIMIT)


if __name__ == '__main__':
    unittest.main()
