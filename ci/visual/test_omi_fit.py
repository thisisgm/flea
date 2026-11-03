"""Exercise the Omi caption gate on synthetic frames, without a compositor."""
from pathlib import Path
import shutil
import struct
import tempfile
import unittest
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
        record = dict(slot=SLOT, pixels=[624, 329, 45, 45], line=[396, 9])
        omi_fit.check_records([record, record, record])
        with self.assertRaisesRegex(ValueError, r'^caption line moved between captions: '):
            omi_fit.check_records([dict(record, line=[398, 9]), record, record])


if __name__ == '__main__':
    unittest.main()
