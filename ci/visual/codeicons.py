"""Language-mark pixel checks for native GPUI rows, measured against the same frame with no stored reference images."""
from collections import Counter
import math


def require(value, message):
    if not value:
        raise ValueError(message)


def crop(image, box):
    width, height, rgb = image
    x, y, w, h = box
    require(all(type(v) is int for v in box) and w > 0 and h > 0
            and 0 <= x < x + w <= width and 0 <= y < y + h <= height, 'invalid mark pixel rectangle')
    return w, h, b''.join(rgb[(row * width + x) * 3:(row * width + x + w) * 3]
                        for row in range(y, y + h))


def background(image):
    _, _, rgb = image
    return Counter(rgb[i:i + 3] for i in range(0, len(rgb), 3)).most_common(1)[0][0]


def ink(image, ground=None):
    """Ground colour, mean colour of the fully inked pixels, peak distance and per-pixel distance."""
    _, _, rgb = image
    ground = tuple(ground if ground is not None else background(image))
    pixels = [tuple(rgb[i:i + 3]) for i in range(0, len(rgb), 3)]
    distance = [math.dist(p, ground) for p in pixels]
    peak = max(distance, default=0)
    require(peak >= 8, 'missing mark pixels')
    solid = [p for p, d in zip(pixels, distance) if d >= peak * .98]
    return ground, tuple(sum(p[c] for p in solid) / len(solid) for c in range(3)), peak, distance


def logo_box(image):
    """The lower-left square of the cut page that LangMarks.js fills with the logo."""
    width, height = image[:2]
    mark = min(width, 18.85) if width <= 32 else width
    offset = (width - mark) / 2
    left, top = math.floor(offset + mark / 24), math.ceil(offset + mark * 9 / 24)
    right, bottom = math.ceil(offset + mark * 15 / 24), math.floor(offset + mark * 23 / 24)
    return [left, top, right - left, bottom - top]


def coverage(image, box, ground, peak):
    _, _, rgb = crop(image, box)
    return [min(1., math.dist(rgb[i:i + 3], ground) / peak) for i in range(0, len(rgb), 3)]


def overlap(a, b):
    union = sum(max(x, y) for x, y in zip(a, b))
    return sum(min(x, y) for x, y in zip(a, b)) / max(union, 1e-9)


def close(actual, wanted, what):
    error = max(abs(a - b) for a, b in zip(actual, wanted))
    require(error <= 8, f'{what} ink mismatch: actual={actual} expected={wanted} max_channel_error={error:.1f}')
    return error


def check(mark, generic, expected_rgb):
    """One language mark beside a generic page in the same row state; expected_rgb is the state's ink."""
    require(mark[:2] == generic[:2], 'mark slot size mismatch')
    ground, page, peak, _ = ink(mark)
    close(ground, ink(generic)[0], 'row ground')
    page_error = close(page, expected_rgb, 'language page')
    box = logo_box(mark)
    logo_error = None
    if mark[0] > 32:
        # The logo shares the page's ink: a faded or recoloured logo under a correct page fails here. Thin logos
        # never reach full ink at list size, so the same shared renderer is held to this at grid size only.
        _, logo, logo_peak, _ = ink(crop(mark, box), ground)
        require(logo_peak >= peak * .9, f'language logo ink mismatch: peak {logo_peak:.1f} of page {peak:.1f}')
        logo_error = close(logo, expected_rgb, 'language logo')
    shape = coverage(mark, box, ground, peak)
    plain = coverage(generic, box, ground, ink(generic)[2])
    # The thinnest logo (fish at grid size) inks 0.23 of the text page's square; an erased square inks none.
    require(sum(shape) >= .12 * sum(plain), 'language contour mismatch: logo square is empty')
    # A generic or text page in the logo square is the regression this gate exists for.
    require(overlap(shape, plain) < .6, f'language contour mismatch: logo square matches the generic page ({overlap(shape, plain):.2f})')
    width, height = mark[:2]
    fold = [width * 2 // 3, 0, width - width * 2 // 3, height * 3 // 8]
    page_shape = overlap(coverage(mark, fold, ground, peak), coverage(generic, fold, ground, ink(generic)[2]))
    require(page_shape >= .8, f'language contour mismatch: cut page fold differs from the generic page ({page_shape:.2f})')
    return dict(rgb=list(page), max_channel_error=page_error, logo_max_channel_error=logo_error,
                fold_overlap=page_shape, shape=shape)


def distinct(receipts):
    """Every language draws its own logo: no two logo squares of one view and state coincide."""
    for i, a in enumerate(receipts):
        for b in receipts[i + 1:]:
            if a['view'] == b['view'] and a['state'] == b['state'] and len(a['shape']) == len(b['shape']):
                score = overlap(a['shape'], b['shape'])
                require(score < .9, f'language contour mismatch: {a["language"]} and {b["language"]} draw the same logo ({score:.2f})')


def state_ink(generic_ground, foreground, alpha):
    return tuple(generic_ground[c] + alpha * (foreground[c] - generic_ground[c]) for c in range(3))


def controls(mark, generic, expected_rgb):
    """Damage a live mark five ways; every damaged copy must be refused, or the gate proves nothing."""
    width, height, rgb = mark
    ground, page, peak, distance = ink(mark)
    left, top, w, h = logo_box(mark)
    inside = {y * width + x for y in range(top, top + h) for x in range(left, left + w)}

    def painted(target, where):
        out = bytearray(rgb)
        for i in where:
            alpha = min(1., distance[i] / peak)
            out[i * 3:i * 3 + 3] = bytes(round(ground[c] + (target[c] - ground[c]) * alpha) for c in range(3))
        return width, height, bytes(out)

    hue = tuple((round(c) + 80) % 256 for c in page)
    damaged = {
        'blank': (width, height, bytes(ground) * (width * height)),
        'generic-page': generic,
        'erased-logo': painted(ground, inside),
        'logo-hue': painted(hue, inside),
        'logo-opacity': painted(state_ink(ground, page, .55), inside),
        'page-hue': painted(hue, range(width * height)),
    }
    if width <= 32:
        damaged = {k: v for k, v in damaged.items() if not k.startswith('logo-')}
    for name, image in damaged.items():
        try:
            check(image, generic, expected_rgb)
        except ValueError:
            continue
        raise ValueError(f'language pixel control accepted {name}')
    return sorted(damaged)
