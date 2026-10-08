"""Writes Presentation.pptx, written for these tests (not a plaso file).

A minimal Office Open XML package whose property parts use what plaso's
Document.docx doesn't: every core property, a time with a fraction and one
with a zone offset, slide counts, manager and company, heading pairs and
titles of parts, a hyperlink base, entities, CDATA, and custom properties
(reached through a relationship target with a leading slash). Run with
`python3 -I tests/fixtures/written/make_pptx.py tests/fixtures/written/Presentation.pptx`.
"""

import sys
import zipfile

FIXED_TIME = (1980, 1, 1, 0, 0, 0)

CONTENT_TYPES = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">\
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>\
<Default Extension="xml" ContentType="application/xml"/>\
<Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/>\
<Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/>\
<Override PartName="/docProps/app.xml" ContentType="application/vnd.openxmlformats-officedocument.extended-properties+xml"/>\
<Override PartName="/docProps/custom.xml" ContentType="application/vnd.openxmlformats-officedocument.custom-properties+xml"/>\
</Types>"""

RELATIONSHIPS = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">\
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="ppt/presentation.xml"/>\
<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/>\
<Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties" Target="docProps/app.xml"/>\
<Relationship Id="rId4" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/custom-properties" Target="/docProps/custom.xml"/>\
</Relationships>"""

CORE = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" \
xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" \
xmlns:dcmitype="http://purl.org/dc/dcmitype/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">\
<dc:title>R&amp;D &lt;draft&gt; caf&#233; &#x2014; plan</dc:title>\
<dc:subject>Quarterly review</dc:subject>\
<dc:creator>Alice Example</dc:creator>\
<cp:keywords>budget; review</cp:keywords>\
<dc:description><![CDATA[Notes <for> the board]]></dc:description>\
<cp:lastModifiedBy>Bob Example</cp:lastModifiedBy>\
<cp:revision>12</cp:revision>\
<cp:lastPrinted>2021-03-05T09:00:00+02:00</cp:lastPrinted>\
<dcterms:created xsi:type="dcterms:W3CDTF">2021-03-04T05:06:07Z</dcterms:created>\
<dcterms:modified xsi:type="dcterms:W3CDTF">2021-03-05T10:20:30.1234567Z</dcterms:modified>\
<cp:category>Finance</cp:category>\
<cp:contentStatus>Draft</cp:contentStatus>\
<dc:language>fr-FR</dc:language>\
<dc:identifier>DOC-0042</dc:identifier>\
<cp:version>1.3</cp:version>\
</cp:coreProperties>"""

APP = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties" \
xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes">\
<Template>Office Theme</Template>\
<TotalTime>95</TotalTime>\
<Words>120</Words>\
<Application>Microsoft Office PowerPoint</Application>\
<PresentationFormat>Widescreen</PresentationFormat>\
<Paragraphs>30</Paragraphs>\
<Slides>4</Slides>\
<Notes>2</Notes>\
<HiddenSlides>1</HiddenSlides>\
<MMClips>3</MMClips>\
<ScaleCrop>true</ScaleCrop>\
<HeadingPairs><vt:vector size="4" baseType="variant">\
<vt:variant><vt:lpstr>Theme</vt:lpstr></vt:variant><vt:variant><vt:i4>1</vt:i4></vt:variant>\
<vt:variant><vt:lpstr>Slide Titles</vt:lpstr></vt:variant><vt:variant><vt:i4>2</vt:i4></vt:variant>\
</vt:vector></HeadingPairs>\
<TitlesOfParts><vt:vector size="3" baseType="lpstr">\
<vt:lpstr>Office Theme</vt:lpstr><vt:lpstr>Results</vt:lpstr><vt:lpstr>Next steps</vt:lpstr>\
</vt:vector></TitlesOfParts>\
<Manager>Carol Example</Manager>\
<Company>Société Exemple</Company>\
<LinksUpToDate>true</LinksUpToDate>\
<SharedDoc>true</SharedDoc>\
<HyperlinkBase>https://example.org/base/</HyperlinkBase>\
<HyperlinksChanged>true</HyperlinksChanged>\
<DocSecurity>2</DocSecurity>\
<AppVersion>16.0000</AppVersion>\
</Properties>"""

CUSTOM = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/custom-properties" \
xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes">\
<property fmtid="{D5CDD505-2E9C-101B-9397-08002B2CF9AE}" pid="2" name="Client"><vt:lpwstr>Acme &amp; Co</vt:lpwstr></property>\
<property fmtid="{D5CDD505-2E9C-101B-9397-08002B2CF9AE}" pid="3" name="Reviewed"><vt:bool>true</vt:bool></property>\
</Properties>"""

PRESENTATION = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"/>"""

PARTS = [
    ("[Content_Types].xml", CONTENT_TYPES),
    ("_rels/.rels", RELATIONSHIPS),
    ("ppt/presentation.xml", PRESENTATION),
    ("docProps/core.xml", CORE),
    ("docProps/app.xml", APP),
    ("docProps/custom.xml", CUSTOM),
]


def main():
    with zipfile.ZipFile(sys.argv[1], "w", zipfile.ZIP_DEFLATED) as package:
        for name, text in PARTS:
            info = zipfile.ZipInfo(name, FIXED_TIME)
            info.compress_type = zipfile.ZIP_DEFLATED
            package.writestr(info, text.encode("utf-8"))


if __name__ == "__main__":
    main()
