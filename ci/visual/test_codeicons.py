"""The language-mark pixel gate accepts a logo in the row's ink and refuses every damage control."""
import unittest

import codeicons

GROUND, INK = (17, 28, 24), (86, 107, 94)


def page(size, logo):
    pixels = []
    for y in range(size):
        for x in range(size):
            u, v = x / size, y / size
            outline = (u < .04 and v > .05) or (v > .96) or (v < .1 and u < .6) or (u > .85 and v > .3) or (.6 <= u <= .85 and .05 <= v - (u - .6) <= .15)
            pixels.append(INK if outline or logo(u, v) else GROUND)
    return size, size, b''.join(bytes(p) for p in pixels)


def text(u, v):
    return .15 < u < .55 and any(abs(v - line) < .04 for line in (.5, .65, .8))


def disc(u, v):
    return (u - .32) ** 2 + (v - .66) ** 2 < .04


def wedge(u, v):
    return .1 < u < .55 and .4 < v < .9 and abs(u - .1 - (v - .4) * .9) < .07


class LanguageMarkGate(unittest.TestCase):
    def test_logo_in_row_ink_passes_and_controls_refuse(self):
        for size in (96, 23):
            generic, mark = page(size, text), page(size, disc)
            codeicons.check(mark, generic, INK)
            self.assertIn('generic-page', codeicons.controls(mark, generic, INK))

    def test_wrong_state_ink_is_refused(self):
        with self.assertRaisesRegex(ValueError, 'ink mismatch'):
            codeicons.check(page(96, disc), page(96, text), codeicons.state_ink(GROUND, INK, .55))

    def test_two_languages_drawing_one_logo_are_refused(self):
        generic = page(96, text)
        receipts = [dict(language=name, view='grid', state='rest', **codeicons.check(page(96, logo), generic, INK))
                    for name, logo in (('a', disc), ('b', wedge))]
        codeicons.distinct(receipts)
        with self.assertRaisesRegex(ValueError, 'draw the same logo'):
            codeicons.distinct(receipts + [dict(receipts[0], language='c')])


if __name__ == '__main__':
    unittest.main()
