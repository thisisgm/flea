"""PNG encoding and a flat-shaded isometric rasterizer for fixture thumbnails and reference renders."""
import math
import struct
import zlib

ISO_EL = math.degrees(math.atan(1 / math.sqrt(2)))  # 35.26, the isometric pitch
ISO_AZ = 45
LIGHT = (-0.55, 0.65, 0.5)


def png(width, height, pixels, channels=3, level=9):
    """Encodes 8-bit RGB or RGBA rows; pixels is a bytes-like of width * height * channels."""
    stride = width * channels
    raw = bytearray()
    for y in range(height):
        raw.append(0)
        raw += pixels[y * stride:(y + 1) * stride]

    def chunk(tag, body):
        return struct.pack(">I", len(body)) + tag + body + struct.pack(">I", zlib.crc32(tag + body) & 0xFFFFFFFF)

    kind = {3: 2, 4: 6}[channels]
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, kind, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(bytes(raw), level)) + chunk(b"IEND", b""))


def sub(a, b):
    return (a[0] - b[0], a[1] - b[1], a[2] - b[2])


def cross(a, b):
    return (a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0])


def unit(a):
    n = math.sqrt(a[0] ** 2 + a[1] ** 2 + a[2] ** 2) or 1.0
    return (a[0] / n, a[1] / n, a[2] / n)


class Camera:
    """Orbits about target: az turns about z, el pitches the view down; screen y grows downward."""

    def __init__(self, az, el, points, width, height, fill, target=None):
        self.az, self.el = math.radians(az), math.radians(el)
        xs, ys, zs = zip(*points)
        self.target = target or ((min(xs) + max(xs)) / 2, (min(ys) + max(ys)) / 2, (min(zs) + max(zs)) / 2)
        self.scale, self.cx, self.cy = 1.0, 0.0, 0.0
        projected = [self.view(p) for p in points]
        us, vs = [p[0] for p in projected], [p[1] for p in projected]
        span_u, span_v = (max(us) - min(us)) or 1.0, (max(vs) - min(vs)) or 1.0
        self.scale = fill * min(width / span_u, height / span_v)
        self.cx = width / 2 - self.scale * (min(us) + max(us)) / 2
        self.cy = height / 2 - self.scale * (min(vs) + max(vs)) / 2

    def view(self, p):
        x, y, z = sub(p, self.target)
        ca, sa, ce, se = math.cos(self.az), math.sin(self.az), math.cos(self.el), math.sin(self.el)
        u = x * ca - y * sa
        depth = x * sa + y * ca
        v = -(z * ce - depth * se)
        return u, v, depth * ce + z * se

    def screen(self, p):
        u, v, d = self.view(p)
        return self.cx + self.scale * u, self.cy + self.scale * v, d


class Canvas:
    def __init__(self, width, height, ground=(0, 0, 0, 0)):
        self.width, self.height = width, height
        self.pixels = bytearray(bytes(ground) * (width * height))
        self.depth = [-math.inf] * (width * height)

    def put(self, x, y, color):
        if 0 <= x < self.width and 0 <= y < self.height:
            i = (y * self.width + x) * 4
            self.pixels[i:i + 4] = bytes(color)

    def line(self, a, b, color):
        steps = int(max(abs(b[0] - a[0]), abs(b[1] - a[1]))) + 1
        for k in range(steps + 1):
            t = k / steps
            self.put(int(a[0] + (b[0] - a[0]) * t), int(a[1] + (b[1] - a[1]) * t), color)

    def triangle(self, a, b, c, color):
        """Scanline fill at pixel centres with a depth test; a, b and c are (x, y, depth), larger depth nearer."""
        area = (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1])
        if abs(area) < 1e-9:
            return
        # Depth as a plane over the screen: d = gx * x + gy * y + g0.
        gx = ((b[2] - a[2]) * (c[1] - a[1]) - (c[2] - a[2]) * (b[1] - a[1])) / area
        gy = ((c[2] - a[2]) * (b[0] - a[0]) - (b[2] - a[2]) * (c[0] - a[0])) / area
        g0 = a[2] - gx * a[0] - gy * a[1]
        top, bottom = max(0, math.ceil(min(a[1], b[1], c[1]) - 0.5)), min(self.height - 1, math.floor(max(a[1], b[1], c[1]) - 0.5))
        edges = ((a, b), (b, c), (c, a))
        pixel, depth = bytes(color), self.depth
        for y in range(top, bottom + 1):
            sy = y + 0.5
            xs = [p[0] + (sy - p[1]) * (q[0] - p[0]) / (q[1] - p[1]) for p, q in edges if (p[1] <= sy < q[1]) or (q[1] <= sy < p[1])]
            if len(xs) < 2:
                continue
            left, right = max(0, math.ceil(min(xs) - 0.5)), min(self.width - 1, math.floor(max(xs) - 0.5))
            row = y * self.width
            d = gx * (left + 0.5) + gy * sy + g0
            for x in range(left, right + 1):
                if d > depth[row + x]:
                    depth[row + x] = d
                    self.pixels[(row + x) * 4:(row + x) * 4 + 4] = pixel
                d += gx

    def encode(self):
        return png(self.width, self.height, self.pixels, channels=4)


def shade(normal, ink, dark):
    """Mixes the lit and dark inks by the Lambert term against LIGHT, as the boards' two-tone flat shading."""
    lam = max(0.0, sum(n * l for n, l in zip(normal, unit(LIGHT))))
    return tuple(round(d + (i - d) * lam) for i, d in zip(ink, dark)) + (255,)


def render(meshes, width, height, az=ISO_AZ, el=ISO_EL, fill=0.92, plate=None, grid=(110, 110, 110, 255),
           frame=(150, 150, 150, 255), ground=(0, 0, 0, 0)):
    """meshes: [(triangles, ink, dark)]; plate: (width, depth) in mm drawn as 10 mm cells under the models."""
    points = [v for tris, _, _ in meshes for t in tris for v in t]
    if plate:
        points += [(0, 0, 0), (plate[0], 0, 0), (plate[0], plate[1], 0), (0, plate[1], 0)]
    cam = Camera(az, el, points, width, height, fill)
    canvas = Canvas(width, height, ground)
    if plate:
        pw, ph = plate
        for x in range(0, int(pw) + 1, 10):
            canvas.line(cam.screen((x, 0, 0)), cam.screen((x, ph, 0)), grid)
        for y in range(0, int(ph) + 1, 10):
            canvas.line(cam.screen((0, y, 0)), cam.screen((pw, y, 0)), grid)
        corners = [cam.screen(p) for p in ((0, 0, 0), (pw, 0, 0), (pw, ph, 0), (0, ph, 0))]
        for i in range(4):
            canvas.line(corners[i], corners[(i + 1) % 4], frame)
    for tris, ink, dark in meshes:
        for t in tris:
            s = [cam.screen(v) for v in t]
            normal = unit(cross(sub(t[1], t[0]), sub(t[2], t[0])))
            canvas.triangle(s[0], s[1], s[2], shade(normal, ink, dark))
    return canvas.encode()


def gradient_png(width, height, stops, level=9):
    """A vertical blend through RGB stops with a small fixed ripple, deterministic and decodable everywhere."""
    pixels = bytearray()
    for y in range(height):
        t = y / max(1, height - 1) * (len(stops) - 1)
        i = min(int(t), len(stops) - 2)
        f = t - i
        base = [stops[i][k] + (stops[i + 1][k] - stops[i][k]) * f for k in range(3)]
        for x in range(width):
            ripple = ((x * 7 + y * 13) % 17) - 8
            pixels += bytes(max(0, min(255, round(c + ripple))) for c in base)
    return png(width, height, pixels, channels=3, level=level)
