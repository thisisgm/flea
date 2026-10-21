"""Exercise native sidecar boundaries independently of a running compositor."""
import copy
import json
from pathlib import Path
import shutil
import struct
import tempfile
import unittest
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


if __name__ == '__main__':
    unittest.main()
