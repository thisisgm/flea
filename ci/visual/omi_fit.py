"""Native pixel gate for Omi placement across the first two caption changes."""
import json
from pathlib import Path
import time

from blackgate import read_image


def ink_box(path, slot, color):
    """Measure accent pixels, including a guard around the reported layout slot."""
    image_width, image_height, rgb = read_image(Path(path))
    x, y, width, height = slot
    target = tuple(int(color[i:i + 2], 16) for i in (1, 3, 5))
    points = []
    for row in range(max(0, int(y) - 4), min(image_height, int(y + height) + 4)):
        for column in range(max(0, int(x) - 4), min(image_width, int(x + width) + 4)):
            offset = (row * image_width + column) * 3
            pixel = rgb[offset:offset + 3]
            if max(abs(a - b) for a, b in zip(pixel, target)) <= 3:
                points.append((column, row))
    if not points:
        raise ValueError(f'Omi has no accent pixels in {path}')
    xs, ys = zip(*points)
    return [min(xs), min(ys), max(xs) + 1 - min(xs), max(ys) + 1 - min(ys)]


def check_records(records):
    if len(records) != 3:
        raise ValueError('Omi gate did not observe two caption changes')
    first = records[0]
    for record in records:
        if record['slot'][2:] != [48., 48.]:
            raise ValueError(f'Omi hero lost its old 48px slot: {record}')
        if record['slot'] != first['slot'] or record['pixels'] != first['pixels']:
            raise ValueError(f'Omi moved between captions: {records}')


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
            grab(window, path)
            after = json.loads(call(window, 'emptyHeroState'))
            if after.get('caption') != caption or after.get('captionOpacity') != 1:
                continue
            slot = [float(value) for value in state['omi']['slot'].split()]
            box = ink_box(path, slot, state['markColor'])
            records.append(dict(caption=caption, slot=slot, pixels=box, png=str(path)))
            seen.add(caption)
        time.sleep(.05)
    (output / 'omi-caption-geometry.json').write_text(json.dumps(records, indent=2))
    check_records(records)
    print('GPUI_OMI_FIT PASS captions=3 changes=2 stable_pixels=true slot=48x48', flush=True)
