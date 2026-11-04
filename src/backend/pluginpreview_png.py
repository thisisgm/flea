# The PNG stream is checked with Python's packaged zlib, inside the preview jail.
import struct
import sys
import zlib

SIGNATURE = b"\x89PNG\r\n\x1a\n"
MAX_FILE = 32 << 20
MAX_SIDE = 8192
CHUNK_OVERHEAD = 12
IHDR_BYTES = 13
MAX_PALETTE = 256
BYTE_BITS = 8
BYTE_MASK = (1 << BYTE_BITS) - 1
FILTER_NONE, FILTER_SUB, FILTER_UP, FILTER_AVERAGE, FILTER_PAETH = range(5)
# x start, y start, x step, y step for the seven Adam7 passes.
ADAM7 = ((0, 0, 8, 8), (4, 0, 8, 8), (0, 4, 4, 8), (2, 0, 4, 4),
         (0, 2, 2, 4), (1, 0, 2, 2), (0, 1, 1, 2))



# Sample row: filter Sub plus bytes 1, 1 reconstructs palette indices 1, 2.
def palette_row(row, previous, columns, depth, palette):
    pixels = bytearray(row[1:])
    filtering = row[0]
    if filtering != FILTER_NONE:
        for at, value in enumerate(pixels):
            left = pixels[at - 1] if at else 0
            up = previous[at]
            corner = previous[at - 1] if at else 0
            if filtering == FILTER_SUB:
                prediction = left
            elif filtering == FILTER_UP:
                prediction = up
            elif filtering == FILTER_AVERAGE:
                prediction = (left + up) // 2
            else:
                estimate = left + up - corner
                a, b, c = abs(estimate - left), abs(estimate - up), abs(estimate - corner)
                prediction = left if a <= b and a <= c else up if b <= c else corner
            pixels[at] = (value + prediction) & BYTE_MASK
    mask = (1 << depth) - 1
    for column in range(columns):
        bit = column * depth
        sample = (pixels[bit // BYTE_BITS] >> (BYTE_BITS - depth - bit % BYTE_BITS)) & mask
        if sample >= palette:
            raise ValueError("PNG sample references an absent palette entry")
    return pixels

# Sample input: PNG signature, IHDR(1, 1, 8, grayscale), IDAT(zlib(two zero bytes)), IEND.
def check(path):
    with open(path, "rb") as stream:
        data = stream.read(MAX_FILE + 1)
    if len(data) > MAX_FILE or not data.startswith(SIGNATURE):
        raise ValueError("invalid PNG signature or file size")
    at = len(SIGNATURE)
    header = None
    palette = None
    compressed = bytearray()
    ended = False
    image_ended = False
    image_started = False
    while at < len(data):
        if len(data) - at < CHUNK_OVERHEAD:
            raise ValueError("truncated PNG chunk")
        size = struct.unpack_from(">I", data, at)[0]
        end = at + size + CHUNK_OVERHEAD
        if end > len(data):
            raise ValueError("truncated PNG payload")
        kind = data[at + 4:at + 8]
        body = data[at + 8:end - 4]
        crc = struct.unpack_from(">I", data, end - 4)[0]
        if not kind.isalpha() or not kind[2:3].isupper() or zlib.crc32(kind + body) != crc:
            raise ValueError("invalid PNG chunk or checksum")
        if header is None:
            if kind != b"IHDR" or size != IHDR_BYTES:
                raise ValueError("missing PNG header")
            header = struct.unpack(">IIBBBBB", body)
        elif kind == b"IHDR":
            raise ValueError("duplicate PNG header")
        elif kind == b"PLTE":
            if palette is not None or image_started or not size or size % 3 or size > MAX_PALETTE * 3:
                raise ValueError("invalid PNG palette")
            palette = size // 3
        elif kind == b"IDAT":
            if image_ended:
                raise ValueError("nonconsecutive PNG image chunks")
            image_started = True
            compressed.extend(body)
        elif kind == b"IEND":
            if size or end != len(data):
                raise ValueError("invalid PNG end")
            ended = True
            break
        elif kind[:1].isupper():
            raise ValueError("unknown critical PNG chunk")
        if kind != b"IDAT" and image_started:
            image_ended = True
        at = end
    if not ended or not compressed or header is None:
        raise ValueError("incomplete PNG")
    width, height, depth, color, method, filtering, interlace = header
    channels = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}
    depths = {0: (1, 2, 4, 8, 16), 2: (8, 16), 3: (1, 2, 4, 8), 4: (8, 16), 6: (8, 16)}
    if not (1 <= width <= MAX_SIDE and 1 <= height <= MAX_SIDE) or depth not in depths.get(color, ()) or method or filtering or interlace not in (0, 1):
        raise ValueError("invalid PNG image format")
    if (color == 3 and (palette is None or palette > 1 << depth)) or (color in (0, 4) and palette is not None):
        raise ValueError("invalid palette for PNG image format")
    passes = ADAM7 if interlace else ((0, 0, 1, 1),)
    # Request one bounded row at a time, even for the largest permitted 16-bit image.
    decoder = zlib.decompressobj()
    pending = compressed
    for x, y, dx, dy in passes:
        columns = max(0, (width - x + dx - 1) // dx)
        rows = max(0, (height - y + dy - 1) // dy)
        if not columns or not rows:
            continue
        row_bytes = (columns * channels[color] * depth + 7) // 8 + 1
        previous = bytearray(row_bytes - 1)
        for _ in range(rows):
            row = decoder.decompress(pending, row_bytes)
            pending = decoder.unconsumed_tail
            if len(row) != row_bytes or row[0] > FILTER_PAETH:
                raise ValueError("invalid PNG scanline")
            if color == 3:
                previous = palette_row(row, previous, columns, depth, palette)
    if decoder.decompress(pending, 1) or not decoder.eof or decoder.unused_data:
        raise ValueError("invalid PNG compressed stream length")


try:
    check(sys.argv[1])
except (OSError, ValueError, zlib.error, struct.error):
    sys.exit(1)
