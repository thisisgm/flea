"""Office fixtures as real OOXML, ODF and encrypted compound files with fixed timestamps and exact byte sizes."""
import hashlib
import io
import struct
from xml.sax.saxutils import escape
import zipfile

from . import aes

STAMP = (2025, 6, 15, 10, 0, 0)
W_NS = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
PAD_FIELD, PAD_HEADER = 0x6C66, 4  # a private extra field id, and its id and length bytes
PASSWORD = "fixture"  # lease.docx opens with this password; the battery only needs it to stay locked


def zip_exact(entries, target):
    """entries: [(name, bytes, deflate)]; extra fields on the last entries pad to exactly target bytes when one is given."""
    def write(extras, comment):
        buffer = io.BytesIO()
        with zipfile.ZipFile(buffer, "w") as archive:
            for index, (name, data, deflate) in enumerate(entries):
                info = zipfile.ZipInfo(name, STAMP)
                info.compress_type = zipfile.ZIP_DEFLATED if deflate else zipfile.ZIP_STORED
                info.create_system = 3
                info.external_attr = 0o100644 << 16
                info.extra = extras.get(index, b"")
                archive.writestr(info, data, compresslevel=9 if deflate else None)
            archive.comment = comment
        return buffer.getvalue()
    data = write({}, b"")
    if target is None:
        return data
    pad = target - len(data)
    if pad < 0:
        raise ValueError(f"archive base is {len(data)} bytes, over its {target} byte target")
    # A private extra field repeats in both zip headers, padding two bytes per byte; LibreOffice refuses long comments, so one holds the 0 to 7 left.
    extras, index, left = {}, len(entries) - 1, pad // 2
    while left >= PAD_HEADER and index > 0:
        n = min(left, 65535) - PAD_HEADER
        extras[index] = struct.pack("<HH", PAD_FIELD, n) + b"\0" * n
        left -= n + PAD_HEADER
        index -= 1
    comment = b" " * (pad - 2 * sum(len(e) for e in extras.values()))
    if len(comment) > 64:
        raise ValueError(f"archive cannot be padded to exactly {target} bytes")
    result = write(extras, comment)
    if len(result) != target:
        raise ValueError("archive padding arithmetic failed")
    return result


def core_xml(title, author, created="2025-06-02T09:00:00Z"):
    return (f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<cp:coreProperties '
            'xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" '
            'xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" '
            'xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">'
            f'<dc:title>{escape(title)}</dc:title><dc:creator>{escape(author)}</dc:creator>'
            f'<dcterms:created xsi:type="dcterms:W3CDTF">{created}</dcterms:created>'
            f'<dcterms:modified xsi:type="dcterms:W3CDTF">2025-06-15T10:00:00Z</dcterms:modified></cp:coreProperties>\n').encode()


def app_xml(extra):
    return ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<Properties '
            'xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties">'
            f'<Application>flea fixture</Application>{extra}</Properties>\n').encode()


ROOT_RELS = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<Relationships '
             'xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
             '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="{main}"/>'
             '<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/>'
             '<Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties" Target="docProps/app.xml"/>'
             '</Relationships>\n')


def content_types(overrides):
    rows = "".join(f'<Override PartName="{part}" ContentType="{kind}"/>' for part, kind in overrides)
    return ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
            '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
            '<Default Extension="xml" ContentType="application/xml"/>'
            f'{rows}<Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/>'
            '<Override PartName="/docProps/app.xml" ContentType="application/vnd.openxmlformats-officedocument.extended-properties+xml"/></Types>\n').encode()


DOCX_STYLES = (f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<w:styles xmlns:w="{W_NS}">'
               '<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Liberation Serif" w:hAnsi="Liberation Serif"/>'
               '<w:sz w:val="24"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:after="160"/></w:pPr></w:pPrDefault></w:docDefaults>'
               '<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>'
               '<w:style w:type="paragraph" w:styleId="Title"><w:name w:val="Title"/><w:basedOn w:val="Normal"/>'
               '<w:rPr><w:rFonts w:ascii="Liberation Sans" w:hAnsi="Liberation Sans"/><w:b/><w:sz w:val="56"/></w:rPr></w:style>'
               '<w:style w:type="paragraph" w:styleId="Subtitle"><w:name w:val="Subtitle"/><w:basedOn w:val="Normal"/>'
               '<w:rPr><w:i/><w:sz w:val="28"/></w:rPr></w:style>'
               '<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/>'
               '<w:pPr><w:spacing w:before="240"/><w:outlineLvl w:val="0"/></w:pPr>'
               '<w:rPr><w:rFonts w:ascii="Liberation Sans" w:hAnsi="Liberation Sans"/><w:b/><w:sz w:val="32"/></w:rPr></w:style></w:styles>\n').encode()


def docx(pages, title, author, target):
    """pages: [[(style, text), ...], ...], each page ended by a page break except the last."""
    body = []
    for number, page in enumerate(pages):
        for style, text in page:
            props = f'<w:pPr><w:pStyle w:val="{style}"/></w:pPr>' if style else ""
            body.append(f'<w:p>{props}<w:r><w:t xml:space="preserve">{escape(text)}</w:t></w:r></w:p>')
        if number < len(pages) - 1:
            body.append('<w:p><w:r><w:br w:type="page"/></w:r></w:p>')
    document = (f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<w:document xmlns:w="{W_NS}"><w:body>'
                + "".join(body) + '<w:sectPr><w:pgSz w:w="11906" w:h="16838"/>'
                '<w:pgMar w:top="1417" w:right="1417" w:bottom="1417" w:left="1417" w:header="709" w:footer="709" w:gutter="0"/>'
                '</w:sectPr></w:body></w:document>\n').encode()
    rels = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
            '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>'
            '</Relationships>\n').encode()
    words = sum(len(text.split()) for page in pages for _, text in page)
    entries = [("[Content_Types].xml", content_types([
        ("/word/document.xml", "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"),
        ("/word/styles.xml", "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml")]), True),
        ("_rels/.rels", ROOT_RELS.format(main="word/document.xml").encode(), True),
        ("word/document.xml", document, True), ("word/_rels/document.xml.rels", rels, True), ("word/styles.xml", DOCX_STYLES, True),
        ("docProps/core.xml", core_xml(title, author), True), ("docProps/app.xml", app_xml(f"<Pages>{len(pages)}</Pages><Words>{words}</Words>"), True)]
    return zip_exact(entries, target)


def column(n):
    return chr(ord("A") + n)


def sheet_xml(header, rows, formula=None):
    """rows of strings and numbers; formula(row number) gives the last column's formula when set."""
    out = [f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>']
    for r, values in enumerate([header] + rows, 1):
        cells = []
        for c, value in enumerate(values):
            ref = f"{column(c)}{r}"
            if isinstance(value, str):
                cells.append(f'<c r="{ref}" t="inlineStr"><is><t>{escape(value)}</t></is></c>')
            elif formula and r > 1 and c == len(values) - 1:
                cells.append(f'<c r="{ref}"><f>{formula(r)}</f><v>{value}</v></c>')
            else:
                cells.append(f'<c r="{ref}"><v>{value}</v></c>')
        out.append(f'<row r="{r}">' + "".join(cells) + "</row>")
    out.append("</sheetData></worksheet>\n")
    return "".join(out).encode()


MONTHLY = [("Seeds", 120, 104), ("Compost", 80, 95), ("Tools", 150, 138), ("Water", 60, 72), ("Fencing", 200, 185),
           ("Mulch", 45, 40), ("Insurance", 90, 90), ("Printing", 25, 31), ("Open evening", 70, 64), ("Repairs", 110, 126),
           ("Plants", 140, 133), ("Signage", 35, 30)]
SAVINGS = [("January", 50), ("February", 50), ("March", 75), ("April", 60), ("May", 60), ("June", 80)]


def xlsx(target):
    monthly = [[item, planned, actual, actual - planned] for item, planned, actual in MONTHLY]
    total_p, total_a = sum(r[1] for r in monthly), sum(r[2] for r in monthly)
    monthly.append(["Total", total_p, total_a, total_a - total_p])
    balance, savings = 0, []
    for month, deposit in SAVINGS:
        balance += deposit
        savings.append([month, deposit, balance])
    workbook = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" '
                'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets>'
                '<sheet name="Monthly" sheetId="1" r:id="rId1"/><sheet name="Savings" sheetId="2" r:id="rId2"/></sheets></workbook>\n').encode()
    rels = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
            '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>'
            '<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet2.xml"/>'
            '</Relationships>\n').encode()
    sheet = "application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"
    entries = [("[Content_Types].xml", content_types([
        ("/xl/workbook.xml", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"),
        ("/xl/worksheets/sheet1.xml", sheet), ("/xl/worksheets/sheet2.xml", sheet)]), True),
        ("_rels/.rels", ROOT_RELS.format(main="xl/workbook.xml").encode(), True),
        ("xl/workbook.xml", workbook, True), ("xl/_rels/workbook.xml.rels", rels, True),
        ("xl/worksheets/sheet1.xml", sheet_xml(["Item", "Planned", "Actual", "Difference"], monthly, lambda r: f"C{r}-B{r}"), True),
        ("xl/worksheets/sheet2.xml", sheet_xml(["Month", "Deposit", "Balance"], savings), True),
        ("docProps/core.xml", core_xml("Garden budget 2025", "Sam Ellis"), True), ("docProps/app.xml", app_xml(""), True)]
    return zip_exact(entries, target)


ODF = ('xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" '
       'xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" '
       'xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" '
       'xmlns:presentation="urn:oasis:names:tc:opendocument:xmlns:presentation:1.0" xmlns:dc="http://purl.org/dc/elements/1.1/" '
       'xmlns:meta="urn:oasis:names:tc:opendocument:xmlns:meta:1.0" office:version="1.3"')


def odp(slides, title, thumbnail, target):
    """slides: [(title, [line, ...])] on 16:9 pages; thumbnail is the embedded PNG of slide 1."""
    pages = []
    for number, (heading, lines) in enumerate(slides, 1):
        body = "".join(f"<text:p>{escape(line)}</text:p>" for line in lines)
        pages.append(f'<draw:page draw:name="Slide {number}" draw:master-page-name="Default" presentation:presentation-page-layout-name="AL1T0">'
                     f'<draw:frame presentation:class="title" svg:x="1.4cm" svg:y="1.6cm" svg:width="25.2cm" svg:height="3.2cm">'
                     f'<draw:text-box><text:p>{escape(heading)}</text:p></draw:text-box></draw:frame>'
                     f'<draw:frame presentation:class="outline" svg:x="1.4cm" svg:y="5.6cm" svg:width="25.2cm" svg:height="8.6cm">'
                     f'<draw:text-box>{body}</draw:text-box></draw:frame></draw:page>')
    content = (f'<?xml version="1.0" encoding="UTF-8"?>\n<office:document-content {ODF}><office:body><office:presentation>'
               + "".join(pages) + "</office:presentation></office:body></office:document-content>\n").encode()
    styles = (f'<?xml version="1.0" encoding="UTF-8"?>\n<office:document-styles {ODF}><office:automatic-styles>'
              '<style:page-layout style:name="PM1"><style:page-layout-properties fo:page-width="28cm" fo:page-height="15.75cm" '
              'style:print-orientation="landscape"/></style:page-layout></office:automatic-styles><office:master-styles>'
              '<style:master-page style:name="Default" style:page-layout-name="PM1"/></office:master-styles></office:document-styles>\n').encode()
    meta = (f'<?xml version="1.0" encoding="UTF-8"?>\n<office:document-meta {ODF}><office:meta><dc:title>{escape(title)}</dc:title>'
            '<meta:initial-creator>Sam Ellis</meta:initial-creator><dc:date>2025-06-15T10:00:00</dc:date>'
            f'<meta:document-statistic meta:object-count="{len(slides) * 2}"/></office:meta></office:document-meta>\n').encode()
    manifest = ('<?xml version="1.0" encoding="UTF-8"?>\n<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" manifest:version="1.3">'
                '<manifest:file-entry manifest:full-path="/" manifest:version="1.3" manifest:media-type="application/vnd.oasis.opendocument.presentation"/>'
                '<manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/>'
                '<manifest:file-entry manifest:full-path="styles.xml" manifest:media-type="text/xml"/>'
                '<manifest:file-entry manifest:full-path="meta.xml" manifest:media-type="text/xml"/>'
                '<manifest:file-entry manifest:full-path="Thumbnails/thumbnail.png" manifest:media-type="image/png"/>'
                '</manifest:manifest>\n').encode()
    entries = [("mimetype", b"application/vnd.oasis.opendocument.presentation", False), ("content.xml", content, True),
               ("styles.xml", styles, True), ("meta.xml", meta, True), ("Thumbnails/thumbnail.png", thumbnail, False),
               ("META-INF/manifest.xml", manifest, True)]
    return zip_exact(entries, target)


def standard_key(password, salt, spins=50000):
    """ECMA-376 Standard Encryption key derivation for AES-128 with SHA-1."""
    h = hashlib.sha1(salt + password.encode("utf-16-le")).digest()
    for i in range(spins):
        h = hashlib.sha1(struct.pack("<I", i) + h).digest()
    h = hashlib.sha1(h + struct.pack("<I", 0)).digest()
    x1 = hashlib.sha1(bytes(0x36 ^ (h[i] if i < len(h) else 0) for i in range(64))).digest()
    x2 = hashlib.sha1(bytes(0x5C ^ (h[i] if i < len(h) else 0) for i in range(64))).digest()
    return (x1 + x2)[:16]


def encryption_info(salt, key, verifier):
    csp = "Microsoft Enhanced RSA and AES Cryptographic Provider\0".encode("utf-16-le")
    header = struct.pack("<IIIIIIII", 0x24, 0, 0x660E, 0x8004, 128, 0x18, 0, 0) + csp
    hashed = hashlib.sha1(verifier).digest().ljust(32, b"\0")
    verifier_block = (struct.pack("<I", 16) + salt + aes.ecb(key, verifier) + struct.pack("<I", 20) + aes.ecb(key, hashed))
    return struct.pack("<HHI", 4, 2, 0x24) + struct.pack("<I", len(header)) + header + verifier_block


def encrypted(package, salt=None):
    """The package encrypted with PASSWORD into a 15-sector compound file, exactly 8,192 bytes."""
    salt = salt or hashlib.sha256(b"flea fixture lease salt").digest()[:16]
    verifier = hashlib.sha256(b"flea fixture lease verifier").digest()[:16]
    key = standard_key(PASSWORD, salt)
    info = encryption_info(salt, key, verifier)
    padded = package + b"\0" * (-len(package) % 16)
    stream = struct.pack("<Q", len(package)) + aes.ecb(key, padded)
    return compound([("EncryptionInfo", info), ("EncryptedPackage", stream)])


ENDOFCHAIN, FREESECT, FATSECT, NOSTREAM = 0xFFFFFFFE, 0xFFFFFFFF, 0xFFFFFFFD, 0xFFFFFFFF


def entry(name, kind, child, right, start, size):
    encoded = (name + "\0").encode("utf-16-le")
    return (encoded.ljust(64, b"\0") + struct.pack("<HBB", len(encoded), kind, 1) + struct.pack("<III", NOSTREAM, right, child)
            + b"\0" * 16 + struct.pack("<I", 0) + b"\0" * 16 + struct.pack("<IQ", start, size))


def compound(streams):
    """A version 3 compound file: sector 0 FAT, 1 directory, 2 MiniFAT, 3 the mini stream, then the large stream."""
    (small_name, small), (large_name, large) = streams
    if len(small) > 512 or not 4096 <= len(large):
        raise ValueError("compound layout expects one mini stream under 512 bytes and one regular stream")
    minis = -(-len(small) // 64)
    large_sectors = -(-len(large) // 512)
    total = 4 + large_sectors
    fat = [FATSECT, ENDOFCHAIN, ENDOFCHAIN, ENDOFCHAIN] + [4 + i + 1 for i in range(large_sectors - 1)] + [ENDOFCHAIN]
    fat += [FREESECT] * (128 - len(fat))
    minifat = [i + 1 for i in range(minis - 1)] + [ENDOFCHAIN] + [FREESECT] * (128 - minis)
    directory = (entry("Root Entry", 5, 1, NOSTREAM, 3, minis * 64) + entry(small_name, 2, NOSTREAM, 2, 0, len(small))
                 + entry(large_name, 2, NOSTREAM, NOSTREAM, 4, len(large)) + b"\0" * 64 + struct.pack("<HBB", 0, 0, 0)
                 + struct.pack("<III", NOSTREAM, NOSTREAM, NOSTREAM) + b"\0" * 48)
    header = (bytes.fromhex("D0CF11E0A1B11AE1") + b"\0" * 16 + struct.pack("<HHHHH", 0x3E, 3, 0xFFFE, 9, 6) + b"\0" * 6
              + struct.pack("<IIIIIIIII", 0, 1, 1, 0, 4096, 2, 1, ENDOFCHAIN, 0)
              + struct.pack("<I", 0) + struct.pack("<I", FREESECT) * 108)
    sectors = [struct.pack("<128I", *fat), directory, struct.pack("<128I", *minifat), small.ljust(512, b"\0"),
               large.ljust(large_sectors * 512, b"\0")]
    data = header + b"".join(sectors)
    if len(data) != 512 * (1 + total):
        raise ValueError("compound layout size mismatch")
    return data
