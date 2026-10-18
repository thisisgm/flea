"""Text and image PDFs with a fixed-date Info dictionary and an exact byte size, padded by a comment block after the header."""
import zlib

A4 = (595, 842)


def literal(text):
    return "(" + text.replace("\\", "\\\\").replace("(", "\\(").replace(")", "\\)") + ")"


def utf16(text):
    return "<" + text.encode("utf-16-be").hex().upper() + ">"


def info_string(text):
    return utf16("﻿" + text) if any(ord(c) > 126 for c in text) else literal(text)


def document(pages, title, author, target, cjk=False, size=A4):
    """pages: [[(font size, x, y, text), ...]]; Helvetica for Latin, a CID Japanese font when cjk is set."""
    objects = []

    def add(body):
        objects.append(body)
        return len(objects)

    font = add("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>")
    bold = add("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>")
    fonts = f"/F1 {font} 0 R /F2 {bold} 0 R"
    if cjk:
        descriptor = add("<< /Type /FontDescriptor /FontName /HeiseiKakuGo-W5 /Flags 4 /FontBBox [-92 -250 1010 922] "
                         "/ItalicAngle 0 /Ascent 752 /Descent -271 /CapHeight 737 /StemV 114 >>")
        cid = add(f"<< /Type /Font /Subtype /CIDFontType0 /BaseFont /HeiseiKakuGo-W5 /CIDSystemInfo << /Registry (Adobe) "
                  f"/Ordering (Japan1) /Supplement 5 >> /FontDescriptor {descriptor} 0 R /DW 1000 >>")
        japanese = add(f"<< /Type /Font /Subtype /Type0 /BaseFont /HeiseiKakuGo-W5-UniJIS-UCS2-H /Encoding /UniJIS-UCS2-H "
                       f"/DescendantFonts [{cid} 0 R] >>")
        fonts += f" /F3 {japanese} 0 R"
    pages_id = len(objects) + 1
    objects.append(None)
    kids = []
    for page in pages:
        ops = []
        for points, x, y, text in page:
            if cjk and any(ord(c) > 255 for c in text):
                ops.append(f"BT /F3 {points} Tf {x} {y} Td {utf16(text)} Tj ET")
            else:
                face = "/F2" if points >= 16 else "/F1"
                ops.append(f"BT {face} {points} Tf {x} {y} Td {literal(text)} Tj ET")
        stream = "\n".join(ops)
        content = add(f"<< /Length {len(stream.encode('latin-1'))} >>\nstream\n{stream}\nendstream")
        kids.append(add(f"<< /Type /Page /Parent {pages_id} 0 R /MediaBox [0 0 {size[0]} {size[1]}] "
                        f"/Resources << /Font << {fonts} >> >> /Contents {content} 0 R >>"))
    objects[pages_id - 1] = f"<< /Type /Pages /Count {len(kids)} /Kids [{' '.join(f'{k} 0 R' for k in kids)}] >>"
    return pack(objects, pages_id, title, author, target)


def image(width, height, rgb, target):
    """One page showing an 8-bit RGB raster at one point per pixel, as an ImageMagick PDF shows its canvas."""
    stream = zlib.compress(bytes(rgb), 9).decode("latin-1")
    objects = [f"<< /Type /XObject /Subtype /Image /Width {width} /Height {height} /ColorSpace /DeviceRGB /BitsPerComponent 8 "
               f"/Filter /FlateDecode /Length {len(stream)} >>\nstream\n{stream}\nendstream"]
    draw = f"q {width} 0 0 {height} 0 0 cm /Im1 Do Q"
    objects.append(f"<< /Length {len(draw)} >>\nstream\n{draw}\nendstream")
    objects.append(f"<< /Type /Page /Parent 4 0 R /MediaBox [0 0 {width} {height}] /Resources << /XObject << /Im1 1 0 R >> >> "
                   "/Contents 2 0 R >>")
    objects.append("<< /Type /Pages /Count 1 /Kids [3 0 R] >>")
    return pack(objects, 4, "", "", target)


def pack(objects, pages_id, title, author, target):
    """Appends the catalog and the fixed-date Info, then pads the file to exactly target bytes."""
    objects = list(objects)
    objects.append(f"<< /Type /Catalog /Pages {pages_id} 0 R >>")
    catalog = len(objects)
    objects.append(f"<< /Title {info_string(title)} /Author {info_string(author)} /Producer (flea fixture) "
                   "/CreationDate (D:20250615100000Z) /ModDate (D:20250615100000Z) >>")
    info = len(objects)

    def serialize(padding):
        out = bytearray(b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n" + padding)
        offsets = []
        for number, body in enumerate(objects, 1):
            offsets.append(len(out))
            out += f"{number} 0 obj\n{body}\nendobj\n".encode("latin-1")
        xref = len(out)
        out += f"xref\n0 {len(objects) + 1}\n0000000000 65535 f \n".encode()
        out += b"".join(f"{o:010d} 00000 n \n".encode() for o in offsets)
        out += f"trailer\n<< /Size {len(objects) + 1} /Root {catalog} 0 R /Info {info} 0 R >>\nstartxref\n{xref}\n%%EOF\n".encode()
        return bytes(out)

    def padding(count):
        # Comment lines of at most 80 bytes, each a percent sign, zeros and a newline; a lone newline is also allowed.
        lines, left = [], count
        while left:
            n = min(80, left)
            lines.append(b"%" + b"0" * (n - 2) + b"\n" if n >= 2 else b"\n")
            left -= n
        return b"".join(lines)

    pad = target - len(serialize(b""))
    for _ in range(3):
        if pad < 0:
            raise ValueError(f"PDF base is over its {target} byte target")
        data = serialize(padding(pad))
        if len(data) == target:
            return data
        # The startxref offset can gain a digit once the padding moves it.
        pad -= len(data) - target
    raise ValueError("PDF padding did not converge")
