//! Every property, against independent readers written with Python's
//! standard library (`tests/oracle/*.py`, not this crate's code): the
//! property sets of plaso's `Document.doc` (`ole_props.py`), and the
//! property parts of plaso's `Document.docx` and of `Presentation.pptx`,
//! written for these tests (`ooxml_props.py`).

use std::collections::BTreeSet;
use std::fmt::Display;

use office::ole::{self, Value};
use office::Document;
use shell::compound::CompoundFile;

const STREAMS: [&str; 2] = ["\u{5}SummaryInformation", "\u{5}DocumentSummaryInformation"];
const SECONDS_PER_MINUTE: u64 = 60;
const TICKS_PER_MICROSECOND: i64 = 10;

fn fixture(path: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/tests/fixtures/{path}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn oracle(name: &str) -> BTreeSet<String> {
    let text = std::fs::read_to_string(format!(
        "{}/tests/oracle/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    text.lines().map(str::to_owned).collect()
}

/// A value as `ole_props.py` writes it.
fn oracle_text(value: &Value) -> String {
    match value {
        Value::Text(text) => text.clone(),
        Value::Int(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Filetime(raw) => raw.to_string(),
        Value::ClipboardData(size) => format!("cf:{size}"),
        Value::Blob(bytes) => format!("blob:{}", bytes.len()),
        Value::Vector(items) => items
            .iter()
            .map(oracle_text)
            .collect::<Vec<_>>()
            .join(" | "),
        other => format!("{other:?}"),
    }
}

#[test]
fn property_sets_as_python_reads_them() {
    let data = fixture("plaso/Document.doc");
    let file = CompoundFile::parse(&data).unwrap();
    let mut got = BTreeSet::new();
    for stream in STREAMS {
        let set = ole::read_property_set(&file.stream(stream).unwrap().unwrap()).unwrap();
        assert_eq!(set.problems, Vec::<String>::new());
        let label = stream.trim_start_matches('\u{5}');
        for section in &set.sections {
            let line =
                |id: u32, text: String| format!("{label}\t{}\t0x{id:08x}\t{text}", section.fmtid);
            if let Some(codepage) = section.codepage {
                got.insert(line(ole::CODEPAGE, codepage.to_string()));
            }
            if !section.names.is_empty() {
                let names: Vec<String> = section
                    .names
                    .iter()
                    .map(|(id, name)| format!("{id}={name}"))
                    .collect();
                got.insert(line(ole::DICTIONARY, names.join(", ")));
            }
            for property in &section.properties {
                got.insert(line(property.id, oracle_text(&property.value)));
            }
        }
    }
    assert_eq!(got, oracle("ole-Document.doc.tsv"));
}

fn line(part: &str, name: &str, value: Option<impl Display>) -> Option<String> {
    value.map(|v| format!("{part}\t{name}\t{v}"))
}

/// Every property as `ooxml_props.py` writes it.
fn package_lines(document: &Document) -> BTreeSet<String> {
    let p = &document.properties;
    let micros = |time: Option<common::time::Ts>| {
        time.and_then(|t| t.ticks())
            .map(|t| t.div_euclid(TICKS_PER_MICROSECOND))
    };
    let joined = |items: Vec<String>| (!items.is_empty()).then(|| items.join(" | "));
    let pairs = joined(
        p.heading_pairs
            .iter()
            .map(|(name, count)| format!("{name}={count}"))
            .collect(),
    );
    let lines = [
        line("core", "title", p.title.as_ref()),
        line("core", "subject", p.subject.as_ref()),
        line("core", "creator", p.author.as_ref()),
        line("core", "keywords", p.keywords.as_ref()),
        line("core", "description", p.comments.as_ref()),
        line("core", "lastModifiedBy", p.last_saved_by.as_ref()),
        line("core", "revision", p.revision.as_ref()),
        line("core", "created", micros(p.created)),
        line("core", "modified", micros(p.modified)),
        line("core", "lastPrinted", micros(p.last_printed)),
        line("core", "category", p.category.as_ref()),
        line("core", "contentStatus", p.content_status.as_ref()),
        line("core", "contentType", p.content_type.as_ref()),
        line("core", "language", p.language.as_ref()),
        line("core", "identifier", p.identifier.as_ref()),
        line("core", "version", p.version.as_ref()),
        line("app", "Template", p.template.as_ref()),
        line("app", "Manager", p.manager.as_ref()),
        line("app", "Company", p.company.as_ref()),
        line("app", "Application", p.application.as_ref()),
        line("app", "AppVersion", p.app_version.as_ref()),
        line("app", "PresentationFormat", p.presentation_format.as_ref()),
        line("app", "HyperlinkBase", p.hyperlink_base.as_ref()),
        line(
            "app",
            "TotalTime",
            p.edit_time.map(|t| t.as_secs() / SECONDS_PER_MINUTE),
        ),
        line("app", "Pages", p.pages),
        line("app", "Words", p.words),
        line("app", "Characters", p.characters),
        line("app", "CharactersWithSpaces", p.characters_with_spaces),
        line("app", "Lines", p.lines),
        line("app", "Paragraphs", p.paragraphs),
        line("app", "Slides", p.slides),
        line("app", "Notes", p.notes),
        line("app", "HiddenSlides", p.hidden_slides),
        line("app", "MMClips", p.multimedia_clips),
        line("app", "DocSecurity", p.security),
        line("app", "ScaleCrop", p.scale_crop),
        line("app", "LinksUpToDate", p.links_up_to_date),
        line("app", "SharedDoc", p.shared),
        line("app", "HyperlinksChanged", p.hyperlinks_changed),
        line("app", "HeadingPairs", pairs),
        line("app", "TitlesOfParts", joined(p.titles_of_parts.clone())),
    ];
    let custom = document
        .custom
        .iter()
        .map(|(name, value)| format!("custom\t{name}\t{value}"));
    lines.into_iter().flatten().chain(custom).collect()
}

#[test]
fn package_properties_as_python_reads_them() {
    for (path, expected) in [
        ("plaso/Document.docx", "ooxml-Document.docx.tsv"),
        ("written/Presentation.pptx", "ooxml-Presentation.pptx.tsv"),
    ] {
        let document = office::read(&fixture(path)).unwrap();
        assert_eq!(document.problems, Vec::<String>::new(), "{path}");
        assert_eq!(document.other, Vec::<(String, String)>::new(), "{path}");
        assert_eq!(package_lines(&document), oracle(expected), "{path}");
    }
}
