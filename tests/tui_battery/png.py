"""Small bounded PNG codec for raw captures and comparison artifacts."""
import struct
import zlib

SIGNATURE = b"\x89PNG\r\n\x1a\n"


def read(path):
    try:
        return decode(path)
    except (IndexError, KeyError, TypeError, struct.error, zlib.error) as error:
        raise ValueError("malformed PNG") from error


def decode(path):
    data = path.read_bytes()
    if data[:8] != SIGNATURE or len(data) > 128 * 1024 * 1024:
        raise ValueError("invalid or oversized PNG")
    offset, compressed, palette, alpha = 8, bytearray(), None, b""
    width = height = kind = channels = 0
    while offset < len(data):
        size = struct.unpack_from(">I", data, offset)[0]
        tag = data[offset + 4:offset + 8]
        body = data[offset + 8:offset + 8 + size]
        if offset + 12 + size > len(data):
            raise ValueError("truncated PNG chunk")
        crc = struct.unpack_from(">I", data, offset + 8 + size)[0]
        if zlib.crc32(tag + body) & 0xffffffff != crc:
            raise ValueError("bad PNG checksum")
        if tag == b"IHDR":
            width, height, depth, kind, method, filter_method, interlace = struct.unpack(">IIBBBBB", body)
            if depth != 8 or interlace or method or filter_method or kind not in (0, 2, 3, 4, 6):
                raise ValueError("PNG requires non-interlaced 8-bit samples")
            if not 0 < width * height <= 16_000_000:
                raise ValueError("PNG pixel budget exceeded")
            channels = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}[kind]
        elif tag == b"PLTE":
            palette = [tuple(body[i:i + 3]) for i in range(0, len(body), 3)]
        elif tag == b"tRNS":
            alpha = body
        elif tag == b"IDAT":
            compressed.extend(body)
        offset += size + 12
        if tag == b"IEND":
            break
    stride = width * channels
    expected = (stride + 1) * height
    decoder = zlib.decompressobj()
    raw = decoder.decompress(bytes(compressed), expected + 1)
    if not channels or len(raw) != expected or not decoder.eof or decoder.unconsumed_tail:
        raise ValueError("PNG decompressed size mismatch")
    previous, result, pos = bytearray(stride), bytearray(), 0
    for _ in range(height):
        mode = raw[pos]
        row = bytearray(raw[pos + 1:pos + 1 + stride])
        pos += stride + 1
        for x in range(stride):
            a = row[x - channels] if x >= channels else 0
            b = previous[x]
            c = previous[x - channels] if x >= channels else 0
            if mode == 1:
                delta = a
            elif mode == 2:
                delta = b
            elif mode == 3:
                delta = (a + b) // 2
            elif mode == 4:
                p = a + b - c
                delta = min((a, b, c), key=lambda v: abs(p - v))
            elif mode == 0:
                delta = 0
            else:
                raise ValueError("unknown PNG filter")
            row[x] = (row[x] + delta) & 255
        for x in range(0, stride, channels):
            values = row[x:x + channels]
            if kind in (0, 4):
                rgb, opacity = values[:1] * 3, values[1] if kind == 4 else 255
            elif kind == 3:
                rgb = palette[values[0]]
                opacity = alpha[values[0]] if values[0] < len(alpha) else 255
            else:
                rgb, opacity = values[:3], values[3] if kind == 6 else 255
            result.extend((*rgb, opacity))
        previous = row
    return width, height, result


def write(path, width, height, pixels):
    if len(pixels) != width * height * 4:
        raise ValueError("RGBA size mismatch")
    def chunk(tag, body):
        return struct.pack(">I", len(body)) + tag + body + struct.pack(">I", zlib.crc32(tag + body) & 0xffffffff)
    stride = width * 4
    raw = b"".join(b"\0" + pixels[y:y + stride] for y in range(0, len(pixels), stride))
    with path.open("xb") as output:
        output.write(SIGNATURE + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
                     + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))


def crop(image, rect):
    width, height, pixels = image
    x, y, w, h = rect
    if min(x, y) < 0 or min(w, h) <= 0 or x + w > width or y + h > height:
        raise ValueError("crop outside capture")
    return w, h, bytearray().join(pixels[((y + row) * width + x) * 4:((y + row) * width + x + w) * 4] for row in range(h))


def artifacts(candidate, reference, directory):
    w, h, a = candidate
    rw, rh, b = reference
    if (w, h) != (rw, rh):
        raise ValueError(f"native grid {w}x{h} differs from reference {rw}x{rh}; no resampling permitted")
    overlay, diff, changed = bytearray(), bytearray(), 0
    for i in range(0, len(a), 4):
        delta = [abs(a[i + c] - b[i + c]) for c in range(3)]
        changed += bool(max(delta))
        overlay.extend((*[(a[i + c] + b[i + c]) // 2 for c in range(3)], 255))
        diff.extend((*delta, 255))
    write(directory / "overlay.png", w, h, overlay)
    write(directory / "diff.png", w, h, diff)
    return {"different_pixels": changed, "total_pixels": w * h, "resampled": False}
