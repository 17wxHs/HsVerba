"""生成 P3 测试素材：docx / pptx / xlsx / srt / txt / pdf（全部手工构造，可校验）。"""
import os
import zipfile

BASE = os.path.join(os.path.dirname(os.path.abspath(__file__)), "p3_fixtures")
os.makedirs(BASE, exist_ok=True)

CT = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
{extra}
</Types>"""


def write_docx(path):
    doc = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
<w:p><w:r><w:t>Quarterly revenue grew by</w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t> 18% year over year</w:t></w:r><w:r><w:t>.</w:t></w:r></w:p>
<w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:t>2024</w:t></w:r></w:p>
<w:p><w:r><w:t>The board approved the acquisition on March 3.</w:t></w:r></w:p>
<w:p><w:r><w:t>自动检测：这是中文段落，应该被翻译。</w:t></w:r></w:p>
<w:p/>
</w:body></w:document>"""
    header = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:hdr xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:p><w:r><w:t>Confidential — Internal Use Only</w:t></w:r></w:p></w:hdr>"""
    rels = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"""
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr(
            "[Content_Types].xml",
            CT.format(
                extra='<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>\n'
                '<Override PartName="/word/header1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/>'
            ),
        )
        z.writestr("_rels/.rels", rels)
        z.writestr("word/document.xml", doc)
        z.writestr("word/header1.xml", header)


def write_pptx(path):
    slide = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><p:cSld><p:spTree>
<p:sp><p:txBody><a:bodyPr/><a:p><a:r><a:rPr lang="en-US"/><a:t>Our strategy for next year</a:t></a:r></a:p>
<a:p><a:r><a:t>Grow in Europe and cut costs.</a:t></a:r></a:p></p:txBody></p:sp>
<p:sp><p:txBody><a:p><a:fld id="{1}" type="slidenum"><a:t>1</a:t></a:fld></a:p></p:txBody></p:sp>
</p:spTree></p:cSld></p:sld>"""
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr(
            "[Content_Types].xml",
            CT.format(
                extra='<Override PartName="/ppt/slides/slide1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/>'
            ),
        )
        z.writestr("ppt/slides/slide1.xml", slide)


def write_xlsx(path):
    sst = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="3" uniqueCount="3">
<si><t>Product name</t></si>
<si><t>Unit price</t></si>
<si><r><t>Total </t></r><r><t>revenue</t></r></si>
</sst>"""
    sheet = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>
<row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1" t="s"><v>1</v></c><c r="C1" t="s"><v>2</v></c></row>
<row r="2"><c r="A2" t="inlineStr"><is><t>Widget A</t></is></c><c r="B2"><v>19.9</v></c></row>
</sheetData></worksheet>"""
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr(
            "[Content_Types].xml",
            CT.format(
                extra='<Override PartName="/xl/sharedStrings.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"/>\n'
                '<Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>'
            ),
        )
        z.writestr("xl/sharedStrings.xml", sst)
        z.writestr("xl/worksheets/sheet1.xml", sheet)


def write_srt(path):
    data = """1
00:00:01,000 --> 00:00:03,000
Hello and welcome to the show.

2
00:00:03,200 --> 00:00:06,000
Today we talk about shipping
software faster.
"""
    open(path, "w", encoding="utf-8").write(data)


def write_txt(path):
    open(path, "w", encoding="utf-8").write(
        "First line of the document.\n\nSecond paragraph here.\n2024\n"
    )


def write_pdf(path):
    text = "Verba document translation smoke test."
    stream = f"BT /F1 14 Tf 72 720 Td ({text}) Tj ET"
    objs = []
    objs.append("<< /Type /Catalog /Pages 2 0 R >>")
    objs.append("<< /Type /Pages /Kids [3 0 R] /Count 1 >>")
    objs.append(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>"
    )
    objs.append(f"<< /Length {len(stream)} >>\nstream\n{stream}\nendstream")
    objs.append("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>")

    out = bytearray(b"%PDF-1.4\n")
    offsets = []
    for i, o in enumerate(objs, start=1):
        offsets.append(len(out))
        out += f"{i} 0 obj\n{o}\nendobj\n".encode("latin-1")
    xref = len(out)
    out += f"xref\n0 {len(objs)+1}\n0000000000 65535 f \n".encode()
    for off in offsets:
        out += f"{off:010d} 00000 n \n".encode()
    out += f"trailer\n<< /Size {len(objs)+1} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n".encode()
    open(path, "wb").write(bytes(out))


write_docx(os.path.join(BASE, "sample.docx"))
write_pptx(os.path.join(BASE, "sample.pptx"))
write_xlsx(os.path.join(BASE, "sample.xlsx"))
write_srt(os.path.join(BASE, "sample.srt"))
write_txt(os.path.join(BASE, "sample.txt"))
write_pdf(os.path.join(BASE, "sample.pdf"))
print("fixtures ->", BASE)
