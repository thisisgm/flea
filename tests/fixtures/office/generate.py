#!/usr/bin/env python3
"""Generate Office and PDF specimens only inside the caller's marked fixture root."""
from io import BytesIO
from pathlib import Path
import sys
from zipfile import ZipFile, ZIP_STORED


def pdf(destination, pages):
    objects = ['<< /Type /Catalog /Pages 2 0 R >>',
               '<< /Type /Pages /Kids [%s] /Count %d >>' %
               (' '.join('%d 0 R' % (3 + 2 * i) for i in range(pages)), pages)]
    for i in range(pages):
        stream = '0.1 0.1 0.1 rg 30 732 %d 24 re f' % (40 + 40 * i)
        objects += ['<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents %d 0 R >>' % (4 + 2 * i),
                    '<< /Length %d >>\nstream\n%s\nendstream' % (len(stream), stream)]
    body = b'%PDF-1.4\n'
    offsets = []
    for number, obj in enumerate(objects, 1):
        offsets.append(len(body))
        body += ('%d 0 obj\n%s\nendobj\n' % (number, obj)).encode()
    xref = len(body)
    body += ('xref\n0 %d\n0000000000 65535 f \n' % (len(objects) + 1)).encode()
    body += ''.join('%010d 00000 n \n' % offset for offset in offsets).encode()
    body += ('trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n' % (len(objects) + 1, xref)).encode()
    destination.write_bytes(body)


def package(destination, content_type, parts, root_target, size):
    types = '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="txt" ContentType="text/plain"/>'
    types += ''.join('<Override PartName="/%s" ContentType="%s"/>' % pair for pair in content_type)
    relationship = '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="%s"/></Relationships>' % root_target
    def write_members(archive, padding):
        archive.writestr('[Content_Types].xml', types + '</Types>')
        archive.writestr('_rels/.rels', relationship)
        for name, body in parts.items():
            archive.writestr(name, body)
        archive.writestr('flea-padding.txt', b' ' * padding)
    with BytesIO() as template:
        with ZipFile(template, 'w', compression=ZIP_STORED) as archive:
            write_members(archive, 0)
        padding = size - template.tell()
    if padding < 0:
        raise ValueError('specimen content exceeds its board size')
    with ZipFile(destination, 'x', compression=ZIP_STORED) as archive:
        write_members(archive, padding)


def generate(root):
    word_ns = 'http://schemas.openxmlformats.org/wordprocessingml/2006/main'
    paragraphs = []
    for page in range(12):
        if page:
            paragraphs.append('<w:p><w:r><w:br w:type="page"/></w:r></w:p>')
        paragraphs.append('<w:p><w:r><w:t>Office preview report, page %d</w:t></w:r></w:p>' % (page + 1))
    document = '<w:document xmlns:w="%s"><w:body>%s<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:bottom="1440" w:left="1440" w:right="1440"/></w:sectPr></w:body></w:document>' % (word_ns, ''.join(paragraphs))
    package(root / 'report.docx', [('word/document.xml', 'application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml')],
            {'word/document.xml': document}, 'word/document.xml', 88000)
    sheet_ns = 'http://schemas.openxmlformats.org/spreadsheetml/2006/main'
    relationship_ns = 'http://schemas.openxmlformats.org/officeDocument/2006/relationships'
    workbook = '<workbook xmlns="%s" xmlns:r="%s"><sheets>%s</sheets></workbook>' % (sheet_ns, relationship_ns,
        ''.join('<sheet name="Budget %d" sheetId="%d" r:id="rId%d"/>' % (i, i, i) for i in range(1, 4)))
    parts = {'xl/workbook.xml': workbook}
    types = [('xl/workbook.xml', 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml')]
    relations = []
    for i in range(1, 4):
        name = 'xl/worksheets/sheet%d.xml' % i
        parts[name] = '<worksheet xmlns="%s"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Budget page %d</t></is></c><c r="B1"><v>%d</v></c></row></sheetData><printOptions gridLines="1"/><pageSetup paperSize="1" orientation="portrait"/></worksheet>' % (sheet_ns, i, i * 1000)
        types.append((name, 'application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml'))
        relations.append('<Relationship Id="rId%d" Type="%s/worksheet" Target="worksheets/sheet%d.xml"/>' % (i, relationship_ns, i))
    parts['xl/_rels/workbook.xml.rels'] = '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">%s</Relationships>' % ''.join(relations)
    package(root / 'budget.xlsx', types, parts, 'xl/workbook.xml', 41000)
    pdf(root / 'report.pdf', 12)
    pdf(root / 'budget.pdf', 3)


if __name__ == '__main__':
    fixture_root = Path(sys.argv[1])
    if not fixture_root.is_absolute() or not (fixture_root / '.flea-test-sandbox').is_file():
        raise SystemExit('Office fixture requires an absolute marked root')
    generate(fixture_root)
