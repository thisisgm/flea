"""A single empty, missing or cached third column must fail the retained native interval contract."""
import copy
import unittest

from flea_colhold_frames import interval


def frame(path, last):
    return dict(path=path, hasThird=True, columns=[
        dict(rows=2, firstName="a"), dict(rows=1, firstName="child"), dict(rows=1, firstName=last)])


class IntervalControls(unittest.TestCase):
    def setUp(self):
        self.frames = [frame("/a", "a.txt"), frame("/b", "a.txt"), frame("/b", "b.txt")]

    def test_complete_navigation_keeps_every_intermediate_frame(self):
        self.assertEqual(interval(self.frames, "/a", "/b"), self.frames)

    def test_one_empty_frame_is_not_hidden_by_completed_frames(self):
        self.frames[1]["columns"][1]["rows"] = 0
        with self.assertRaisesRegex(AssertionError, "empty non-empty column"):
            interval(self.frames, "/a", "/b")

    def test_one_missing_track_is_not_hidden_by_a_cached_child(self):
        self.frames[1]["columns"].pop()
        self.frames[1]["hasThird"] = False
        with self.assertRaises(AssertionError):
            interval(self.frames, "/a", "/b")

    def test_missing_third_fact_fails_even_with_three_row_records(self):
        self.frames[1]["hasThird"] = False
        with self.assertRaisesRegex(AssertionError, "no third column"):
            interval(self.frames, "/a", "/b")

    def test_both_settled_endpoints_are_required(self):
        for frames in (self.frames[1:], self.frames[:-1]):
            with self.subTest(frames=copy.deepcopy(frames)), self.assertRaises(AssertionError):
                interval(frames, "/a", "/b")


if __name__ == "__main__":
    unittest.main()
