"""Exercise the Omi caption gate on synthetic frames, without a compositor."""
from pathlib import Path
import json
import shutil
import struct
import tempfile
import unittest
from unittest import mock
import zlib

import omi_fit

WIDTH, HEIGHT = 1100, 700
BACKGROUND = (28, 18, 13)
CAPTION = (198, 179, 149)
SLOT = [613., 318., 66., 66.]
RAIL = [15, 390, 62, 50]


class CaptionGate(unittest.TestCase):
    def setUp(self):
        parent = Path(__file__).resolve().parents[2] / '.superpowers/omi-fit'
        parent.mkdir(parents=True, exist_ok=True)
        self.root = Path(tempfile.mkdtemp(prefix='omi-fit.', dir=parent))
        (self.root / '.flea-test-sandbox').touch()
        self.addCleanup(self.remove_root)

    def remove_root(self):
        root = self.root
        if root.is_absolute() and root.name.startswith('omi-fit.') and (root / '.flea-test-sandbox').is_file():
            shutil.rmtree(root)

    def frame(self, name, boxes):
        """A PNG of the background with each [x, y, width, height] box filled in the caption colour."""
        pixels = bytearray(bytes(BACKGROUND) * WIDTH * HEIGHT)
        for x, y, width, height in boxes:
            for row in range(y, y + height):
                start = (row * WIDTH + x) * 3
                pixels[start:start + width * 3] = bytes(CAPTION) * width
        raw = b''.join(b'\0' + bytes(pixels[row * WIDTH * 3:(row + 1) * WIDTH * 3]) for row in range(HEIGHT))
        chunk = lambda kind, data: struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data))
        path = self.root / name
        path.write_bytes(b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', WIDTH, HEIGHT, 8, 2, 0, 0, 0)) + chunk(b'IDAT', zlib.compress(raw)) + chunk(b'IEND', b''))
        return path

    def test_rail_text_beside_the_hero_is_not_the_caption(self):
        path = self.frame('rail.png', [[580, 396, 130, 9], RAIL])
        self.assertEqual(omi_fit.caption_line(path, SLOT, '#c6b395'), [396, 9])

    def test_omi_ink_at_the_slot_bottom_is_not_the_caption(self):
        path = self.frame('slot.png', [[580, 396, 130, 9], [613, 380, 66, 4]])
        self.assertEqual(omi_fit.caption_line(path, SLOT, '#c6b395'), [396, 9])

    def test_a_slot_ending_mid_row_keeps_its_last_row_out_of_the_band(self):
        path = self.frame('half.png', [[580, 396, 130, 9], [613, 380, 66, 4]])
        self.assertEqual(omi_fit.caption_line(path, [613., 317.5, 66., 66.], '#c6b395'), [396, 9])

    def test_a_caption_with_no_ink_is_named(self):
        path = self.frame('empty.png', [RAIL])
        with self.assertRaisesRegex(ValueError, r'^caption has no ink in '):
            omi_fit.caption_line(path, SLOT, '#c6b395')

    def test_a_translucent_colour_is_refused(self):
        path = self.frame('translucent.png', [[580, 396, 130, 9]])
        with self.assertRaisesRegex(ValueError, r'^caption colour #80c6b395 is not opaque in '):
            omi_fit.caption_line(path, SLOT, '#80c6b395')

    def test_a_moved_caption_line_is_rejected(self):
        record = dict(slot=SLOT, pixels=[624, 329, 45, 45], line=[394, 14], ink=[396, 9])
        omi_fit.check_records([record, record, record])
        with self.assertRaisesRegex(ValueError, r'^caption line moved between captions: '):
            omi_fit.check_records([record, dict(record, line=[398, 14]), record])

    def test_a_changed_line_height_is_rejected(self):
        record = dict(slot=SLOT, pixels=[624, 329, 45, 45], line=[394, 14], ink=[396, 9])
        with self.assertRaisesRegex(ValueError, r'^caption line moved between captions: '):
            omi_fit.check_records([record, dict(record, line=[394, 16]), record])

    def test_ink_outside_the_painted_line_is_rejected(self):
        record = dict(slot=SLOT, pixels=[624, 329, 45, 45], line=[394, 14], ink=[396, 9])
        for ink in ([393, 9], [396, 13]):
            with self.subTest(ink=ink), self.assertRaisesRegex(ValueError, r'^caption ink escapes'):
                omi_fit.check_records([dict(record, ink=ink), record, record])

    def test_caption_descenders_do_not_change_the_painted_line(self):
        states, images = [], []
        for index, (caption, height) in enumerate((('CAPS', 9), ('QUIET', 11), ('J', 11))):
            image = self.frame(f'phrase-{index}.png', [[624, 329, 45, 45], [580, 396, 130, height]])
            images.append(image)
            state = dict(caption=caption, settled=True, captionOpacity=1,
                         captionRect='580 394 130 14', markColor='#c6b395', captionColor='#c6b395',
                         omi=dict(slot='613 318 66 66'))
            states.extend([state, state])
        answers, frames = iter(states), iter(images)
        def grab(_, path, captured=None):
            shutil.copyfile(next(frames), path)
            if captured is not None:
                captured()
        with mock.patch.object(omi_fit.time, 'sleep'):
            omi_fit.caption_case(object(), lambda *_: json.dumps(next(answers)),
                                 grab, self.root)

    def test_caption_check_precedes_pixel_validation(self):
        frames = [self.frame(f'capture-{index}.png', [[624, 329, 45, 45], [580, 396, 130, 9]])
                  for index in range(3)]
        current = 0
        def call(*_):
            return json.dumps(dict(caption=f'PHRASE {current}', settled=True, captionOpacity=1,
                                   captionRect='580 394 130 14', markColor='#c6b395',
                                   captionColor='#c6b395', omi=dict(slot='613 318 66 66')))
        def grab(_, path, captured=None):
            nonlocal current
            shutil.copyfile(frames[current % 3], path)
            if captured is not None:
                captured()
            # Pixel validation finishes after the animation has reached its next phrase.
            current += 1
        with mock.patch.object(omi_fit.time, 'sleep'), mock.patch.object(
                omi_fit.time, 'monotonic', side_effect=range(100)):
            omi_fit.caption_case(object(), call, grab, self.root)


if __name__ == '__main__':
    unittest.main()
