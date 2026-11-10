"""Native pixel gate for Omi placement across the first two caption changes."""
import json
import math
from pathlib import Path
import time

from blackgate import read_image

SLOT_PX = 66.
GUARD_PX = 4
CHANNEL_TOLERANCE = 3
# The caption sits in this many rows under the slot, centred on it and narrower than twice the half span.
CAPTION_BAND_PX = 60
CAPTION_HALF_SPAN_PX = 200


def ink_box(path, rect, color, what, guard=GUARD_PX):
    """Box of the pixels in color inside rect, widened by guard pixels on every side."""
    image_width, image_height, rgb = read_image(Path(path))
    x, y, width, height = rect
    # emptyHeroState colours are '#c6b395' when opaque and '#80c6b395' when not.
    if len(color) != 7:
        raise ValueError(f'{what} colour {color} is not opaque in {path}')
    target = tuple(int(color[i:i + 2], 16) for i in (1, 3, 5))
    points = []
    for row in range(max(0, int(y) - guard), min(image_height, int(y + height) + guard)):
        for column in range(max(0, int(x) - guard), min(image_width, int(x + width) + guard)):
            offset = (row * image_width + column) * 3
            pixel = rgb[offset:offset + 3]
            if max(abs(a - b) for a, b in zip(pixel, target)) <= CHANNEL_TOLERANCE:
                points.append((column, row))
    if not points:
        raise ValueError(f'{what} has no ink in {path}')
    xs, ys = zip(*points)
    return [min(xs), min(ys), max(xs) + 1 - min(xs), max(ys) + 1 - min(ys)]


def caption_line(path, slot, color):
    """Top and height of the caption's ink in a band under the slot; its width follows the phrase."""
    x, y, width, height = slot
    # GPUI lays out unrounded, so a slot can end mid-row; the band starts at the first whole row under it.
    band = [x + width / 2 - CAPTION_HALF_SPAN_PX, math.ceil(y + height), 2 * CAPTION_HALF_SPAN_PX, CAPTION_BAND_PX]
    box = ink_box(path, band, color, 'caption', guard=0)
    return [box[1], box[3]]


def check_records(records):
    if len(records) != 3:
        raise ValueError('Omi gate did not observe two caption changes')
    first = records[0]
    for record in records:
        if record['slot'][2:] != [SLOT_PX, SLOT_PX]:
            raise ValueError(f'Omi hero lost its {SLOT_PX:g}px slot: {record}')
        if record['slot'] != first['slot'] or record['pixels'] != first['pixels']:
            raise ValueError(f'Omi moved between captions: {records}')
        if record['line'] != first['line']:
            raise ValueError(f'caption line moved between captions: {records}')
        top, height = record['line']
        ink_top, ink_height = record['ink']
        if (not all(math.isfinite(value) for value in (top, height, ink_top, ink_height))
                or height <= 0 or ink_height <= 0 or ink_top < math.floor(top)
                or ink_top + ink_height > math.ceil(top + height)):
            raise ValueError(f'caption ink escapes its painted line: {record}')


def caption_case(window, call, grab, output):
    """Capture actual frames and reject any mark translation or slot-size regression."""
    records = []
    seen = set()
    deadline = time.monotonic() + 18
    while len(records) < 3 and time.monotonic() < deadline:
        state = json.loads(call(window, 'emptyHeroState'))
        caption = state.get('caption')
        if (caption and caption not in seen and state.get('settled')
                and state.get('captionOpacity') == 1):
            path = output / f'omi-caption-{len(records)}.png'
            after = {}
            # Bracket the screenshot before pixel validation can span the next caption fade.
            grab(window, path, lambda: after.update(json.loads(call(window, 'emptyHeroState'))))
            if after.get('caption') != caption or after.get('captionOpacity') != 1:
                continue
            slot = [float(value) for value in state['omi']['slot'].split()]
            box = ink_box(path, slot, state['markColor'], 'Omi')
            rect = [float(value) for value in state['captionRect'].split()]
            if len(rect) != 4 or after.get('captionRect') != state['captionRect']:
                raise ValueError('caption layout changed during capture')
            line = [rect[1], rect[3]]
            ink = caption_line(path, slot, state['captionColor'])
            records.append(dict(caption=caption, slot=slot, pixels=box, line=line, ink=ink, png=str(path)))
            seen.add(caption)
        time.sleep(.05)
    (output / 'omi-caption-geometry.json').write_text(json.dumps(records, indent=2))
    check_records(records)
    print(f'GPUI_OMI_FIT PASS captions=3 changes=2 stable_pixels=true stable_caption=true slot={SLOT_PX:g}x{SLOT_PX:g}', flush=True)
