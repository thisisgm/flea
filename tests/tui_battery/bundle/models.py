"""3D print fixtures: bracket.stl, knob.obj, benchy.3mf, benchy.gcode and shelf-kit.3mf, built from fixed geometry."""
import base64
import math
import struct

from . import raster
from .office import zip_exact

HOLE_R = 3.2  # an M6 clearance hole
HOLE_SEGMENTS = 173  # with the gusset's split sides this makes the bracket exactly 4,212 triangles
BRACKET_TRIANGLES = 4212
KNOB_SEGMENTS = 360
ORANGE = ((255, 106, 19), (138, 50, 4))
WHITE = ((238, 238, 236), (119, 118, 123))
FOREGROUND = ((198, 179, 149), (106, 89, 62))  # the board theme's #c6b395 and its OKLCH shade: a part with no material
P1S_PLATE = (256, 256)
MK4_PLATE = (250, 210)
SHELF_KIT = [(70, 80), (70, 136), (165, 128)]  # plate 1: two brackets and the knob, plate millimetres
PLATE2_KNOB = (128, 128)
PLATE_PITCH = 307.2  # the slicer lays plate 2 out one and a fifth plate widths to the right
BENCHY_AT = (95, 105)


def quad(a, b, c, d):
    return [(a, b, c), (a, c, d)]


def fan(a, b, c, d):
    """A quad as four triangles about its centre, for the faces that need an exact triangle count."""
    m = tuple(sum(p[k] for p in (a, b, c, d)) / 4 for k in range(3))
    return [(a, b, m), (b, c, m), (c, d, m), (d, a, m)]


def holed_half(u0, u1, v0, v1, cu, cv, n, lift, flip):
    """A rectangle with one round hole as 2n + 4 triangles in (u, v), lifted to 3D by lift(u, v)."""
    ring, rim = [], []
    for k in range(n):
        a = 2 * math.pi * k / n
        du, dv = math.cos(a), math.sin(a)
        ring.append((cu + HOLE_R * du, cv + HOLE_R * dv))
        t = min(((u1 - cu) / du) if du > 1e-12 else ((u0 - cu) / du) if du < -1e-12 else math.inf,
                ((v1 - cv) / dv) if dv > 1e-12 else ((v0 - cv) / dv) if dv < -1e-12 else math.inf)
        rim.append((cu + t * du, cv + t * dv))
    out = []
    for k in range(n):
        j = (k + 1) % n
        out += [(ring[k], ring[j], rim[j]), (ring[k], rim[j], rim[k])]
    corners = [(u1, v1), (u0, v1), (u0, v0), (u1, v0)]
    angles = [math.atan2(v - cv, u - cu) % (2 * math.pi) for u, v in corners]
    for corner, angle in zip(corners, angles):
        k = int(angle / (2 * math.pi / n)) % n
        out.append((rim[k], corner, rim[(k + 1) % n]))
    faces = [tuple(lift(*p) for p in t) for t in out]
    return [(t[0], t[2], t[1]) for t in faces] if flip else faces


def bracket(ox=0.0, oy=0.0):
    """A 60 x 40 x 40 mm corner bracket: 5 mm base and upright, a 4 mm gusset, two M6 holes in each leg."""
    n, tris = HOLE_SEGMENTS, []
    x0, x1, y0, y1 = ox, ox + 60, oy, oy + 40
    for z, flip in ((5, False), (0, True)):
        for v0, v1, cv in ((y0, oy + 20, oy + 11), (oy + 20, y1, oy + 29)):
            tris += holed_half(x0, x1, v0, v1, ox + 42, cv, n, lambda u, v, z=z: (u, v, z), flip)
    for cv in (oy + 11, oy + 29):
        tris += cylinder_wall((ox + 42, cv), 0, 5, HOLE_R, n, inward=True, axis="z")
    tris += quad((x0, y0, 0), (x1, y0, 0), (x1, y0, 5), (x0, y0, 5)) + quad((x1, y1, 0), (x0, y1, 0), (x0, y1, 5), (x1, y1, 5))
    tris += quad((x1, y0, 0), (x1, y1, 0), (x1, y1, 5), (x1, y0, 5)) + quad((x0, y1, 0), (x0, y0, 0), (x0, y0, 5), (x0, y1, 5))
    for x, flip in ((ox + 5, False), (ox, True)):
        for v0, v1, cv in ((y0, oy + 20, oy + 11), (oy + 20, y1, oy + 29)):
            tris += holed_half(v0, v1, 5, 40, cv, 27, n, lambda u, v, x=x: (x, u, v), flip)
    for cv in (oy + 11, oy + 29):
        tris += cylinder_wall((cv, 27), ox, ox + 5, HOLE_R, n, inward=True, axis="x")
    u0, u1 = ox, ox + 5
    tris += quad((u0, y0, 40), (u1, y0, 40), (u1, y1, 40), (u0, y1, 40)) + quad((u0, y1, 5), (u1, y1, 5), (u1, y0, 5), (u0, y0, 5))
    tris += quad((u0, y0, 5), (u1, y0, 5), (u1, y0, 40), (u0, y0, 40)) + quad((u1, y1, 5), (u0, y1, 5), (u0, y1, 40), (u1, y1, 40))
    g = [(ox + 5, oy + 18, 5), (ox + 25, oy + 18, 5), (ox + 5, oy + 18, 25)]
    h = [(x, y + 4, z) for x, y, z in g]
    tris += [(g[0], g[2], g[1]), (h[0], h[1], h[2])]
    tris += fan(g[0], g[1], h[1], h[0]) + fan(g[2], g[0], h[0], h[2]) + quad(g[1], g[2], h[2], h[1])
    return tris


def cylinder_wall(centre, a, b, r, n, inward=False, axis="z"):
    out = []
    for k in range(n):
        p, q = 2 * math.pi * k / n, 2 * math.pi * (k + 1) / n
        c0, c1 = (centre[0] + r * math.cos(p), centre[1] + r * math.sin(p)), (centre[0] + r * math.cos(q), centre[1] + r * math.sin(q))
        if axis == "z":
            face = quad((*c0, a), (*c1, a), (*c1, b), (*c0, b))
        else:
            face = quad((a, *c0), (a, *c1), (b, *c1), (b, *c0))
        out += [(t[0], t[2], t[1]) for t in face] if inward else face
    return out


def cylinder(cx, cy, z0, z1, r, n):
    ring0 = [(cx + r * math.cos(2 * math.pi * k / n), cy + r * math.sin(2 * math.pi * k / n), z0) for k in range(n)]
    ring1 = [(x, y, z1) for x, y, _ in ring0]
    tris = cylinder_wall((cx, cy), z0, z1, r, n)
    tris += [((cx, cy, z1), ring1[k], ring1[(k + 1) % n]) for k in range(n)]
    tris += [((cx, cy, z0), ring0[(k + 1) % n], ring0[k]) for k in range(n)]
    return tris


def knob(cx=0.0, cy=0.0, n=KNOB_SEGMENTS):
    """A 30 mm knob 12 mm tall under an 18 mm cap 4 mm tall."""
    return cylinder(cx, cy, 0, 12, 15, n) + cylinder(cx, cy, 12, 16, 9, n)


HULL = [(0, 9.0, 5.0, 50.0), (2, 11.5, 3.0, 53.0), (5, 13.5, 1.5, 56.0), (9, 14.8, 0.5, 58.5), (13, 15.3, 0.0, 59.6), (16, 15.5, 0.0, 60.0)]
HULL_T = [0, 0.03, 0.07, 0.12, 0.2, 0.3, 0.4, 0.5, 0.58, 0.66, 0.74, 0.82, 0.9, 0.96]
CABIN = (14, 38, 10.5)
DECK, ROOF, ROOF_TOP, CHIMNEY_TOP = 16, 34, 35.5, 48


def hull_width(t):
    if t < 0.12:
        return 0.8 + 0.2 * math.sqrt(1 - ((0.12 - t) / 0.12) ** 2)
    if t <= 0.5:
        return 1.0
    return 1 - ((t - 0.5) / 0.5) ** 1.8


def hull_loop(hb, xs, xb, ox, oy):
    side = [(xs + t * (xb - xs), hull_width(t)) for t in HULL_T]
    return [(ox + x, oy - hb * w) for x, w in side] + [(ox + xb, oy)] + [(ox + x, oy + hb * w) for x, w in reversed(side)]


def box(x0, y0, z0, x1, y1, z1):
    c = [(x0, y0, z0), (x1, y0, z0), (x1, y1, z0), (x0, y1, z0), (x0, y0, z1), (x1, y0, z1), (x1, y1, z1), (x0, y1, z1)]
    return sum((quad(*[c[i] for i in f]) for f in ((0, 3, 2, 1), (4, 5, 6, 7), (0, 1, 5, 4), (1, 2, 6, 5), (2, 3, 7, 6), (3, 0, 4, 7))), [])


def benchy(ox=0.0, oy=0.0):
    """The fixture boat: a lofted hull, cabin, roof and chimney, with its stern at (ox, oy)."""
    sections = [(z, hull_loop(hb, xs, xb, ox, oy)) for z, hb, xs, xb in HULL]
    tris = []
    for (za, la), (zb, lb) in zip(sections, sections[1:]):
        m = len(la)
        for i in range(m):
            j = (i + 1) % m
            tris += quad((*la[i], za), (*la[j], za), (*lb[j], zb), (*lb[i], zb))
    for z, loop, up in ((sections[0][0], sections[0][1], False), (sections[-1][0], sections[-1][1], True)):
        mid = (sum(p[0] for p in loop) / len(loop), sum(p[1] for p in loop) / len(loop), z)
        for i in range(len(loop)):
            a, b = (*loop[i], z), (*loop[(i + 1) % len(loop)], z)
            tris.append((mid, a, b) if up else (mid, b, a))
    xs, xb, hw = CABIN
    tris += box(ox + xs, oy - hw, DECK, ox + xb, oy + hw, ROOF)
    tris += box(ox + xs - 1, oy - hw - 1, ROOF, ox + xb + 1, oy + hw + 1, ROOF_TOP)
    tris += cylinder(ox + 22, oy, ROOF_TOP, CHIMNEY_TOP, 3.5, 32)
    return tris


def subdivide(tris, extra):
    """Splits the first `extra` triangles into four at their edge midpoints, leaving the shape unchanged."""
    out = []
    for i, (a, b, c) in enumerate(tris):
        if i < extra:
            ab, bc, ca = (tuple((p + q) / 2 for p, q in zip(a, b)), tuple((p + q) / 2 for p, q in zip(b, c)),
                          tuple((p + q) / 2 for p, q in zip(c, a)))
            out += [(a, ab, ca), (ab, b, bc), (ca, bc, c), (ab, bc, ca)]
        else:
            out.append((a, b, c))
    return out


def refine(tris, levels):
    for _ in range(levels):
        tris = subdivide(tris, len(tris))
    return tris


def stl(tris, name):
    header = ("flea fixture " + name).encode().ljust(80, b" ")
    body = bytearray(header + struct.pack("<I", len(tris)))
    for a, b, c in tris:
        n = raster.unit(raster.cross(raster.sub(b, a), raster.sub(c, a)))
        body += struct.pack("<12fH", *n, *a, *b, *c, 0)
    return bytes(body)


def indexed(tris):
    index, vertices, faces = {}, [], []
    for t in tris:
        face = []
        for v in t:
            key = tuple(round(x, 6) for x in v)
            if key not in index:
                index[key] = len(vertices)
                vertices.append(key)
            face.append(index[key])
        faces.append(face)
    return vertices, faces


def obj(tris, name, target):
    vertices, faces = indexed(tris)
    lines = [f"# {name}, a fixture model in millimetres", f"o {name.split('.')[0]}"]
    lines += [f"v {x:.6f} {y:.6f} {z:.6f}" for x, y, z in vertices]
    lines += [f"f {a + 1} {b + 1} {c + 1}" for a, b, c in faces]
    text = ("\n".join(lines) + "\n").encode()
    pad = target - len(text)
    if pad < 3:
        raise ValueError(f"{name} is {len(text)} bytes, over its {target} byte target")
    return text + b"#" + b" " * (pad - 2) + b"\n"


def model_xml(objects, items, unit="millimeter", materials=None, bed=None):
    """objects: [(id, name, tris)]; items: [(object id, x, y)] as translations; materials: {object id: pindex}, others carry none."""
    out = ['<?xml version="1.0" encoding="UTF-8"?>',
           f'<model unit="{unit}" xml:lang="en-US" xmlns="http://schemas.microsoft.com/3dmanufacturing/core/2015/02">',
           '<metadata name="Application">flea fixture</metadata>']
    if bed:
        width, depth = bed
        out += ['<metadata name="printer_model">P1S</metadata>',
                f'<metadata name="printable_area">0x0,{width}x0,{width}x{depth},0x{depth}</metadata>']
    out.append("<resources>")
    if materials:
        out.append('<basematerials id="1024"><base name="orange" displaycolor="#FF6A13"/></basematerials>')
    for oid, name, tris in objects:
        vertices, faces = indexed(tris)
        material = f' pid="1024" pindex="{materials[oid]}"' if materials and oid in materials else ""
        out.append(f'<object id="{oid}" name="{name}" type="model"{material}><mesh><vertices>')
        out += [f'<vertex x="{x:.4f}" y="{y:.4f}" z="{z:.4f}"/>' for x, y, z in vertices]
        out.append("</vertices><triangles>")
        out += [f'<triangle v1="{a}" v2="{b}" v3="{c}"/>' for a, b, c in faces]
        out.append("</triangles></mesh></object>")
    out.append("</resources><build>")
    out += [f'<item objectid="{oid}" transform="1 0 0 0 1 0 0 0 1 {x:.4f} {y:.4f} 0"/>' for oid, x, y in items]
    out.append("</build></model>")
    return ("\n".join(out) + "\n").encode()


CONTENT_TYPES_3MF = (b'<?xml version="1.0" encoding="UTF-8"?>\n<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
                     b'<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
                     b'<Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/>'
                     b'<Default Extension="png" ContentType="image/png"/>'
                     b'<Default Extension="config" ContentType="text/xml"/></Types>\n')
RELS_3MF = (b'<?xml version="1.0" encoding="UTF-8"?>\n<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
            b'<Relationship Target="/3D/3dmodel.model" Id="rel0" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel"/>'
            b'<Relationship Target="/Metadata/thumbnail.png" Id="rel1" '
            b'Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/thumbnail"/></Relationships>\n')


def thumbnail(meshes, size=256, fill=0.92):
    return raster.render(meshes, size, size, fill=fill)


SLACK_3MF = 1000  # headroom under target that zip_exact pads, well above its 4-byte field header and 64-byte comment cap


def threemf(parts, base, target):
    """Stores the model uncompressed so size grows with detail: the finest whole level under target, then k split triangles."""
    def size(tris, k):
        return len(zip_exact(parts(subdivide(tris, k)), None))
    level = base
    while size(refine(level, 1), 0) <= target - SLACK_3MF:
        level = refine(level, 1)
    if size(level, 0) > target:
        raise ValueError(f"3MF base model exceeds its {target} byte target")
    lo, hi = 0, len(level)
    while hi - lo > 1:
        mid = (lo + hi) // 2
        if size(level, mid) <= target - SLACK_3MF:
            lo = mid
        else:
            hi = mid
    return zip_exact(parts(subdivide(level, lo)), target)


def benchy_3mf(target):
    base = benchy()
    thumb = thumbnail([(benchy(), *ORANGE)])

    def parts(tris):
        return [("[Content_Types].xml", CONTENT_TYPES_3MF, False), ("_rels/.rels", RELS_3MF, False),
                ("3D/3dmodel.model", model_xml([(1, "benchy", tris)], [(1, BENCHY_AT[0], BENCHY_AT[1])],
                                            materials={1: 0}, bed=P1S_PLATE), False),
                ("Metadata/thumbnail.png", thumb, False), ("Metadata/plate_1.png", thumb, False),
                ("Metadata/slice_info.config", slice_info(1, "4320", "13.42"), False)]
    return threemf(parts, base, target)


def slice_info(plates, seconds, grams):
    rows = "".join(f'<plate><metadata key="index" value="{i}"/><metadata key="prediction" value="{seconds}"/>'
                   f'<metadata key="weight" value="{grams}"/></plate>' for i in range(1, plates + 1))
    return (f'<?xml version="1.0" encoding="UTF-8"?>\n<config><header><header_item key="X-BBL-Client-Type" value="slicer"/>'
            f'</header>{rows}</config>\n').encode()


def model_settings(plates):
    """plates: [[object id, ...], ...] as the slicer's per-plate object lists."""
    out = ['<?xml version="1.0" encoding="UTF-8"?>', "<config>"]
    for number, ids in enumerate(plates, 1):
        out.append(f'<plate><metadata key="plater_id" value="{number}"/><metadata key="plater_name" value=""/>'
                   f'<metadata key="thumbnail_file" value="Metadata/plate_{number}.png"/>')
        out += [f'<model_instance><metadata key="object_id" value="{oid}"/><metadata key="instance_id" value="0"/></model_instance>'
                for oid in ids]
        out.append("</plate>")
    out.append("</config>")
    return ("\n".join(out) + "\n").encode()


def kit_meshes(plate, bracket_ink=WHITE):
    if plate == 1:
        (ax, ay), (bx, by), (kx, ky) = SHELF_KIT
        return [(bracket(ax, ay), *bracket_ink), (bracket(bx, by), *bracket_ink), (knob(kx, ky), *ORANGE)]
    return [(knob(*PLATE2_KNOB), *ORANGE)]


def shelf_kit_3mf(target):
    (ax, ay), (bx, by), (kx, ky) = SHELF_KIT
    plate1 = thumbnail(kit_meshes(1), fill=0.98)
    plate2 = thumbnail(kit_meshes(2), fill=0.8)
    shape = bracket()

    def parts(knob_tris):
        objects = [(1, "bracket", shape), (2, "bracket", shape), (3, "knob", knob_tris), (4, "knob", knob_tris)]
        items = [(1, ax, ay), (2, bx, by), (3, kx, ky), (4, PLATE_PITCH + PLATE2_KNOB[0], PLATE2_KNOB[1])]
        return [("[Content_Types].xml", CONTENT_TYPES_3MF, False), ("_rels/.rels", RELS_3MF, False),
                ("3D/3dmodel.model", model_xml(objects, items, materials={3: 0, 4: 0}, bed=P1S_PLATE), False),
                ("Metadata/model_settings.config", model_settings([[1, 2, 3], [4]]), False),
                ("Metadata/slice_info.config", slice_info(2, "2580", "31.80"), False),
                ("Metadata/thumbnail.png", plate1, False), ("Metadata/plate_1.png", plate1, False),
                ("Metadata/plate_2.png", plate2, False)]
    return threemf(parts, knob(), target)


def kit_reference(width, height, az):
    """Plate 1 of shelf-kit.3mf on the whole P1S plate, the independent render a Quick Look reference uses."""
    return raster.render(kit_meshes(1, FOREGROUND), width, height, az=az, fill=0.88, plate=P1S_PLATE)


def layer_loops(z, ox, oy):
    """The boat's outline loops at height z, outermost first, for a three-perimeter toolpath."""
    if z <= DECK:
        for (za, hba, xsa, xba), (zb, hbb, xsb, xbb) in zip(HULL, HULL[1:]):
            if za <= z <= zb:
                f = (z - za) / (zb - za)
                hb, xs, xb = hba + (hbb - hba) * f, xsa + (xsb - xsa) * f, xba + (xbb - xba) * f
                break
        return [hull_loop(hb - inset, xs + inset, xb - inset, ox, oy) for inset in (0.0, 0.45, 0.9)]
    xs, xb, hw = CABIN
    if z <= ROOF:
        return [[(ox + xs + i, oy - hw + i), (ox + xb - i, oy - hw + i), (ox + xb - i, oy + hw - i), (ox + xs + i, oy + hw - i)]
                for i in (0.0, 0.45, 0.9)]
    if z <= ROOF_TOP:
        return [[(ox + xs - 1 + i, oy - hw - 1 + i), (ox + xb + 1 - i, oy - hw - 1 + i), (ox + xb + 1 - i, oy + hw + 1 - i),
                 (ox + xs - 1 + i, oy + hw + 1 - i)] for i in (0.0, 0.45, 0.9)]
    return [[(ox + 22 + (3.5 - i) * math.cos(2 * math.pi * k / 32), oy + (3.5 - i) * math.sin(2 * math.pi * k / 32)) for k in range(32)]
            for i in (0.0, 0.45)]


def resample(loop, points):
    """The closed loop walked at `points` evenly spaced stations, so line count follows points alone."""
    edges = [(loop[i], loop[(i + 1) % len(loop)]) for i in range(len(loop))]
    lengths = [math.dist(a, b) for a, b in edges]
    total, out, at, k = sum(lengths), [], 0.0, 0
    for s in range(points):
        d = total * s / points
        while k < len(edges) - 1 and at + lengths[k] < d:
            at += lengths[k]
            k += 1
        a, b = edges[k]
        f = (d - at) / lengths[k] if lengths[k] else 0
        out.append((a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f))
    return out


def gcode(points):
    ox, oy = BENCHY_AT
    thumb = raster.render([(benchy(), *ORANGE)], 220, 124, fill=0.92)
    encoded = base64.b64encode(thumb).decode()
    lines = ["; generated by flea fixture, a PrusaSlicer-format G-code toolpath", ";",
             f"; thumbnail begin 220x124 {len(encoded)}"]
    lines += ["; " + encoded[i:i + 78] for i in range(0, len(encoded), 78)]
    lines += ["; thumbnail end", ";", "M73 P0 R72", "G21", "G90", "M83", "M104 S215", "M140 S60", "G28", "G1 Z0.2 F720"]
    e_per_mm = 0.0332
    layers = int(round(CHIMNEY_TOP / 0.2))
    for n in range(1, layers + 1):
        z = round(n * 0.2, 2)
        lines += [";LAYER_CHANGE", f";Z:{z:.1f}", ";HEIGHT:0.2", f"G1 Z{z:.3f} F720"]
        for loop in layer_loops(z, ox, oy):
            stations = resample(loop, points)
            x, y = stations[0]
            lines.append(f"G1 X{x:.3f} Y{y:.3f} F9000")
            for p, q in zip(stations, stations[1:] + stations[:1]):
                lines.append(f"G1 X{q[0]:.3f} Y{q[1]:.3f} E{math.dist(p, q) * e_per_mm:.5f} F1800")
    lines += ["M104 S0", "M140 S0", "M84", "; filament used [mm] = 5212.40", "; filament used [g] = 15.54",
              "; estimated printing time (normal mode) = 1h 12m 0s", "; printer_model = MK4",
              "; bed_shape = 0x0,250x0,250x210,0x210", "; layer_height = 0.2", ""]
    return "\n".join(lines).encode()


def benchy_gcode(low, high):
    """The toolpath with the loop resolution chosen so the file lands in [low, high]."""
    small, large = gcode(40), gcode(80)
    points = 40 + round((((low + high) / 2) - len(small)) * 40 / (len(large) - len(small)))
    data = gcode(points)
    if not low <= len(data) <= high:
        raise ValueError(f"benchy.gcode is {len(data)} bytes, outside [{low}, {high}]")
    return data
