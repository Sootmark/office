# office

Office documents' metadata for forensics: who wrote a document and who saved it last, when it was created, last saved and last printed, how long it was edited, its page, word and slide counts, company, manager, template and the application that wrote it. Read from Office 97–2003 files (`.doc`, `.xls`, `.ppt` and other compound files: the `SummaryInformation` and `DocumentSummaryInformation` property sets, Microsoft's MS-OLEPS) and from Office Open XML (`.docx`, `.xlsx`, `.pptx`: `docProps/core.xml`, `app.xml` and `custom.xml`), with a small bounded XML reader of its own. Three dependencies, its siblings `sootmark-common` (times, text), `sootmark-shell` (compound files) and `sootmark-zip`.

```toml
[dependencies]
sootmark-office = "0.1"
```

```rust
let document = office::read(&std::fs::read("report.docx")?)?;
let p = &document.properties;
println!("{:?} by {:?}, last saved by {:?}", p.title, p.author, p.last_saved_by);
println!("created {:?}, saved {:?}, edited {:?}", p.created, p.modified, p.edit_time);
for (name, value) in &document.custom {
    println!("{name}: {value}");
}
```

## What you get

- `read(bytes)`: the format, and the same fields from either: title, subject, author, keywords, comments, template, last saved by, revision; created, last saved and last printed times; editing time (a duration); pages, words, characters (with and without spaces), bytes, lines, paragraphs, slides, notes, hidden slides, multimedia clips; application and its version, company, manager, category, presentation format, content type and status, language, identifier, version; security flags, scale or crop, links dirty or up to date, shared, hyperlinks changed, hyperlink base; heading pairs and titles of parts (sheet names, slide titles); the thumbnail's size and the code page. User-defined properties in `custom`, anything else in `other`; a compound file's storages and streams with their modification times in `items`.
- `ole::read_property_set(stream)`: any property set stream, every section and typed value (integers, floats, booleans, strings in Windows-1252, UTF-16, UTF-8 or Latin-1, FILETIMEs, dates, blobs, clipboard data by size, class ids, vectors and variants), the dictionary of user-defined names. Other code pages keep ASCII and escape the rest, with a problem.
- `detect(bytes)`: whether a file is a compound file with a property set stream, or an Office Open XML package.
- Damage goes to `problems`, never a panic. Counts, nesting, vectors and decompressed XML parts (4 MiB) are bounded.

## How it's checked

- plaso's `Document.doc` and `Document.docx` (Apache-2.0, `tests/fixtures/plaso/`) and `Presentation.pptx`, written for these tests (`tests/fixtures/written/make_pptx.py`): 12 of the 14 events plaso reads, every value compared (`tests/plaso.rs`). The two left are the creation times of two storages, which `sootmark-shell` 0.2.2 doesn't give. Where plaso is wrong, the crate follows the format: plaso shows two junk bytes for the heading pairs, leaves out the titles of parts and an editing time of zero, names unmapped properties with their decimal identifier behind `0x`, and calls `PIDDSI_LINKSDIRTY` "links up to date".
- Every property of `Document.doc`'s property sets against a reader written with Python's standard library from MS-CFB and MS-OLEPS, and every property of both packages against one using `zipfile` and `xml.etree` (`tests/oracle.rs`).
- Property tests: arbitrary bytes, each file cut anywhere, random bytes changed, and the property set streams damaged byte by byte, never panic.

## Not yet

- A compound file's property sets are found by name anywhere in its directory (the first one is read), not only at its root: an embedded object's could be read for the document's in a file Office didn't write.
- Code pages other than Windows-1252, UTF-16, UTF-8, Latin-1 and ASCII are not decoded.
- Hyperlink lists (`PIDDSI_HLINKS`, `HLinks`) and digital signatures are not parsed: a compound file's go to `other` by size, a package's are skipped.

## Licence

MIT or Apache-2.0, at your option. plaso's test files are under the Apache licence 2.0.
