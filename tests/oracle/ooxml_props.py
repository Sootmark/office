"""Every property of an Office Open XML package, one TSV line each.

An independent reader (Python standard library only: zipfile and
xml.etree), not this crate's code:

    python3 -I tests/oracle/ooxml_props.py tests/fixtures/plaso/Document.docx

The property parts are found through _rels/.rels. Columns: part (core, app,
custom), element local name (a custom property's name), value. Times are
written as microseconds since 1970 (UTC); heading pairs as "name=count"
joined by " | ", titles of parts joined by " | ", a custom property's value
as its first child's text.
"""

import datetime
import re
import sys
import xml.etree.ElementTree as ElementTree
import zipfile

KINDS = {
    "/metadata/core-properties": "core",
    "/extended-properties": "app",
    "/custom-properties": "custom",
}
TIMES = {"created", "modified", "lastPrinted"}
EPOCH = datetime.datetime(1970, 1, 1, tzinfo=datetime.timezone.utc)


def local(tag):
    return tag.rpartition("}")[2]


def micros(text):
    """A W3CDTF time with a zone, as microseconds since 1970."""
    match = re.fullmatch(r"([0-9T:-]+)(?:\.([0-9]+))?(Z|[+-][0-9]{2}:[0-9]{2})", text)
    stamp, fraction, zone = match.groups()
    zone = "+00:00" if zone == "Z" else zone
    moment = datetime.datetime.fromisoformat(stamp + zone)
    delta = moment - EPOCH
    whole = (delta.days * 86400 + delta.seconds) * 1_000_000
    return str(whole + int((fraction or "0")[:6].ljust(6, "0")))


def vector(element):
    found = next((c for c in element if local(c.tag) == "vector"), None)
    return list(found) if found is not None else []


def app_value(element):
    name = local(element.tag)
    if name == "HeadingPairs":
        values = [variant[0].text or "" for variant in vector(element)]
        return " | ".join(f"{a}={b}" for a, b in zip(values[::2], values[1::2]))
    if name == "TitlesOfParts":
        return " | ".join(item.text or "" for item in vector(element))
    return element.text or ""


def lines(package):
    relationships = ElementTree.fromstring(package.read("_rels/.rels"))
    for relationship in relationships:
        kind = next(
            (k for suffix, k in KINDS.items() if relationship.get("Type").endswith(suffix)),
            None,
        )
        if kind is None:
            continue
        root = ElementTree.fromstring(package.read(relationship.get("Target").lstrip("/")))
        for element in root:
            name = local(element.tag)
            if kind == "custom":
                yield f"custom\t{element.get('name')}\t{element[0].text or ''}"
            elif kind == "core":
                value = element.text or ""
                yield f"core\t{name}\t{micros(value) if name in TIMES else value}"
            else:
                yield f"app\t{name}\t{app_value(element)}"


def main():
    with zipfile.ZipFile(sys.argv[1]) as package:
        for row in sorted(lines(package)):
            print(row)


if __name__ == "__main__":
    main()
