#!/usr/bin/env python3
"""Reject black Flea captures and palette errors using saved capture geometry."""
import argparse
import json
import math
import re
import struct
import subprocess
import tomllib
from pathlib import Path


BLACK_BOUND = 4
# All 3347 approved PNGs peak at 0.639481%; ae9419c8de78's broken window is 85.289081%.
BLACK_THRESHOLD = 0.20
BLACK_RUN = re.compile(rb'[\x00-\x04]+')


def read_image(path):
    with path.open('rb') as stream:
        header = stream.read(24)
    if header[:8] != b'\x89PNG\r\n\x1a\n' or len(header) != 24:
        raise ValueError('not a PNG capture')
    width, height = struct.unpack('>II', header[16:24])
    if not 0 < width * height <= 100_000_000:
        raise ValueError('invalid capture dimensions')
    decoded = subprocess.run(
        ['ffmpeg', '-nostdin', '-v', 'error', '-threads', '1', '-i', str(path),
         '-frames:v', '1', '-threads', '1', '-f', 'rawvideo', '-pix_fmt', 'rgb24', 'pipe:1'],
        check=True, capture_output=True, timeout=30)
    if len(decoded.stdout) != width * height * 3:
        raise ValueError('capture dimensions differ from decoded frame')
    return width, height, decoded.stdout


def black_share(image, rect):
    width, height, rgb = image
    x, y, w, h = rect
    if not (0 <= x < x + w <= width and 0 <= y < y + h <= height):
        raise ValueError('window rectangle is outside capture')
    crop = b''.join(rgb[(row * width + x) * 3:(row * width + x + w) * 3]
                    for row in range(y, y + h))
    count = sum(max(0, match.end() // 3 - (match.start() + 2) // 3)
                for match in BLACK_RUN.finditer(crop))
    return count / (w * h)


def window_rectangles(geometry, clients, kind, image):
    match = re.fullmatch(r'(-?\d+),(-?\d+) (\d+)x(\d+)', geometry)
    if not match:
        raise ValueError('invalid capture geometry')
    gx, gy, gw, gh = map(int, match.groups())
    if gw <= 0 or gh <= 0:
        raise ValueError('empty capture geometry')
    scale_x, scale_y = image[0] / gw, image[1] / gh
    if not math.isclose(scale_x, scale_y, rel_tol=0.001):
        raise ValueError('capture aspect ratio differs from geometry')
    rectangles = []
    for window in clients:
        if window.get('class') != kind:
            continue
        x, y = window['at']
        w, h = window['size']
        if not all(isinstance(n, int) for n in (x, y, w, h)) or w <= 0 or h <= 0:
            raise ValueError('invalid window geometry')
        left, top = max(x, gx), max(y, gy)
        right, bottom = min(x + w, gx + gw), min(y + h, gy + gh)
        if left >= right or top >= bottom:
            continue
        px, py = round((left - gx) * scale_x), round((top - gy) * scale_y)
        ex, ey = round((right - gx) * scale_x), round((bottom - gy) * scale_y)
        rectangles.append((window.get('address', 'unknown'), (px, py, ex - px, ey - py)))
    if not rectangles:
        raise ValueError('capture contains no matching Flea window')
    return rectangles


def theme_background(path):
    with path.open('rb') as stream:
        background = tomllib.load(stream).get('background', '')
    if not isinstance(background, str) or not re.fullmatch(r'#[0-9a-fA-F]{6}', background):
        raise ValueError('theme background must be #RRGGBB')
    return background, all(int(background[i:i + 2], 16) <= BLACK_BOUND for i in (1, 3, 5))


def palette_errors(logs):
    errors = []
    for path in logs:
        if not path.is_file():
            continue
        for line in path.read_text(errors='replace').splitlines():
            if ('Theme.qml' in line and 'is not a function' in line) or 'Unable to assign [undefined] to QColor' in line:
                errors.append(str(path))
                break
    return errors


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--shot', required=True, type=Path)
    parser.add_argument('--geometry', required=True)
    parser.add_argument('--clients', required=True)
    parser.add_argument('--class', dest='kind', default='com.thisisgm.flea')
    parser.add_argument('--case', required=True)
    parser.add_argument('--theme', required=True, type=Path)
    parser.add_argument('--logs', nargs='*', type=Path, default=[])
    args = parser.parse_args()
    prefix = f'VISUAL {args.case} blackgate'
    try:
        image = read_image(args.shot)
        clients = json.loads(args.clients)
        rectangles = window_rectangles(args.geometry, clients, args.kind, image)
        ground, near_black = theme_background(args.theme)
        errors = palette_errors(args.logs)
        measurements = [{'window': address, 'rect': rect, 'black_share': black_share(image, rect)}
                        for address, rect in rectangles]
        metadata = {'capture_geometry': args.geometry, 'clients': clients, 'background': ground,
                    'black_bound': BLACK_BOUND, 'threshold': BLACK_THRESHOLD,
                    'windows': measurements, 'palette_error_logs': errors}
        args.shot.with_suffix('.blackgate.json').write_text(json.dumps(metadata, indent=2) + '\n')
        failed = False
        for row in measurements:
            bad = bool(errors) or (row['black_share'] > BLACK_THRESHOLD and not near_black)
            failed |= bad
            reason = 'palette-log' if errors else 'theme-near-black' if near_black else 'pixels'
            print(f"{prefix} {'FAIL' if bad else 'PASS'} window={row['window']} "
                  f"black={row['black_share']:.6%} threshold={BLACK_THRESHOLD:.6%} "
                  f"background={ground} reason={reason}")
        return int(failed)
    except (OSError, ValueError, TypeError, KeyError, subprocess.SubprocessError) as error:
        print(f'{prefix} FAIL black=unknown threshold={BLACK_THRESHOLD:.6%} reason={error}')
        return 1


if __name__ == '__main__':
    raise SystemExit(main())
