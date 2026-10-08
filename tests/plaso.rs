//! Every event plaso (log2timeline/plaso:20260720) reads from plaso's
//! `Document.doc` and `Document.docx` and from `Presentation.pptx` (written
//! for these tests), with every value it gives, read the same
//! (`tests/oracle/plaso.tsv`, written from plaso's output with its
//! `olecf_summary`, `olecf_document_summary`, `olecf_default` and `oxml`
//! plugins): 14 events, 12 of them read.
//!
//! How plaso's names map here, and where it differs:
//!
//! - plaso names properties it has no name for `0x` followed by the
//!   identifier in decimal: `0x0017` is property 17 (0x11, the thumbnail:
//!   plaso gives its bytes, compared here by count), `0x0022` is 22 (0x16,
//!   hyperlinks changed), `0x0012` is 12 (0x0C, the heading pairs).
//! - plaso's heading pairs are two bytes of the value's first element; the
//!   crate reads the pairs (`("Title", 1)`), so that value isn't compared.
//! - plaso leaves the titles of parts out; the crate reads them ("Table of
//!   Context", checked against `tests/oracle/ole_props.py`).
//! - plaso's `links_up_to_date` for a compound file is `PIDDSI_LINKSDIRTY`
//!   as stored, which says the opposite: it is `links_dirty` here.
//! - plaso leaves out an editing time of zero; the crate reports zero.
//! - plaso's two `olecf:item` creation times (storage `MsoDataStore` and
//!   its child) aren't read: the compound file reader (`sootmark-shell`
//!   0.2.2) gives each item's modification time, not its creation time.

use std::fmt::Display;
use std::time::Duration;

use common::time::Ts;
use office::{Document, ItemKind};

const PLASO_EVENTS: usize = 14;
const UNREAD_CREATION_TIMES: usize = 2;
const SECONDS_PER_MINUTE: u64 = 60;
const TICKS_PER_MICROSECOND: i64 = 10;

fn read(path: &str) -> Document {
    let data = std::fs::read(format!(
        "{}/tests/fixtures/{path}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    assert!(office::detect(&data), "{path}");
    let document = office::read(&data).unwrap();
    assert_eq!(document.problems, Vec::<String>::new(), "{path}");
    document
}

/// `name=value` when there is a value.
fn value(name: &str, value: Option<impl Display>) -> Option<String> {
    value.map(|v| format!("{name}={v}"))
}

/// A plaso event line: file, data type, description, time, values.
fn event(
    file: &str,
    data_type: &str,
    description: &str,
    time: Ts,
    values: &[Option<String>],
) -> Option<String> {
    let micros = time.ticks()?.div_euclid(TICKS_PER_MICROSECOND);
    let mut values: Vec<&str> = values.iter().flatten().map(String::as_str).collect();
    values.sort_unstable();
    let mut fields = vec![
        file.to_owned(),
        data_type.to_owned(),
        description.to_owned(),
        micros.to_string(),
    ];
    fields.extend(values.into_iter().map(str::to_owned));
    Some(fields.join("\t"))
}

fn compound_file_events(file: &str, document: &Document) -> Vec<String> {
    let p = &document.properties;
    let root = document
        .items
        .iter()
        .find(|i| i.kind == ItemKind::Root)
        .unwrap();
    let edit_time = p.edit_time.filter(|t| !t.is_zero()).map(|t| t.as_secs());
    let summary = [
        value("0x0017", p.thumbnail_size.map(|n| format!("{n} bytes"))),
        value("application", p.application.as_ref()),
        value("author", p.author.as_ref()),
        value("codepage", p.codepage),
        value("comments", p.comments.as_ref()),
        value("edit_duration", edit_time),
        value("keywords", p.keywords.as_ref()),
        value("last_saved_by", p.last_saved_by.as_ref()),
        value("number_of_characters", p.characters),
        value("number_of_pages", p.pages),
        value("number_of_words", p.words),
        value("revision_number", p.revision.as_ref()),
        value("security_flags", p.security),
        value("subject", p.subject.as_ref()),
        value("template", p.template.as_ref()),
        value("title", p.title.as_ref()),
    ];
    let document_summary = [
        value("0x0022", p.hyperlinks_changed),
        value("application_version", p.app_version.as_ref()),
        value("category", p.category.as_ref()),
        value("codepage", p.codepage),
        value("company", p.company.as_ref()),
        value("links_up_to_date", p.links_dirty),
        value("manager", p.manager.as_ref()),
        value("number_of_bytes", p.bytes),
        value(
            "number_of_characters_with_white_space",
            p.characters_with_spaces,
        ),
        value("number_of_clips", p.multimedia_clips),
        value("number_of_hidden_slides", p.hidden_slides),
        value("number_of_lines", p.lines),
        value("number_of_notes", p.notes),
        value("number_of_paragraphs", p.paragraphs),
        value("number_of_slides", p.slides),
        value("presentation_format", p.presentation_format.as_ref()),
        value("scale", p.scale_crop),
        value("shared_document", p.shared),
    ];
    let summary_times = [
        ("Document Creation Time", p.created),
        ("Document Last Save Time", p.modified),
        ("Document Last Printed Time", p.last_printed),
        ("Item Modification Time", Some(root.modified)),
    ];
    let mut events: Vec<String> = summary_times
        .iter()
        .filter_map(|&(description, time)| {
            event(file, "olecf:summary_info", description, time?, &summary)
        })
        .collect();
    events.extend(event(
        file,
        "olecf:document_summary_info",
        "Item Modification Time",
        root.modified,
        &document_summary,
    ));
    events.extend(document.items.iter().filter_map(|item| {
        let values = [
            value("name", Some(&item.name)),
            value("size", Some(item.size)),
        ];
        event(
            file,
            "olecf:item",
            "Content Modification Time",
            item.modified,
            &values,
        )
    }));
    events
}

fn package_events(file: &str, document: &Document) -> Vec<String> {
    let p = &document.properties;
    let values = [
        value("application", p.application.as_ref()),
        value("application_version", p.app_version.as_ref()),
        value("author", p.author.as_ref()),
        value(
            "edit_duration",
            p.edit_time.map(|t| t.as_secs() / SECONDS_PER_MINUTE),
        ),
        value("hyperlinks_changed", p.hyperlinks_changed),
        value("last_saved_by", p.last_saved_by.as_ref()),
        value("links_up_to_date", p.links_up_to_date),
        value("number_of_characters", p.characters),
        value("number_of_characters_with_spaces", p.characters_with_spaces),
        value("number_of_clips", p.multimedia_clips),
        value("number_of_hidden_slides", p.hidden_slides),
        value("number_of_lines", p.lines),
        value("number_of_pages", p.pages),
        value("number_of_paragraphs", p.paragraphs),
        value("number_of_slides", p.slides),
        value("number_of_words", p.words),
        value("revision_number", p.revision.as_ref()),
        value("scale", p.scale_crop),
        value("security_flags", p.security),
        value("shared_doc", p.shared),
        value("template", p.template.as_ref()),
    ];
    [
        ("Creation Time", p.created),
        ("Content Modification Time", p.modified),
        ("Last Printed Time", p.last_printed),
    ]
    .iter()
    .filter_map(|&(description, time)| event(file, "openxml:metadata", description, time?, &values))
    .collect()
}

/// plaso's line without the values it gets wrong (see the module docs).
fn without_known_differences(line: &str) -> String {
    line.split('\t')
        .filter(|field| !field.starts_with("0x0012="))
        .collect::<Vec<_>>()
        .join("\t")
}

#[test]
fn every_event_as_plaso_reads_it() {
    let doc = read("plaso/Document.doc");
    let mut got = compound_file_events("Document.doc", &doc);
    got.extend(package_events(
        "Document.docx",
        &read("plaso/Document.docx"),
    ));
    got.extend(package_events(
        "Presentation.pptx",
        &read("written/Presentation.pptx"),
    ));
    got.sort();
    let oracle = std::fs::read_to_string(format!(
        "{}/tests/oracle/plaso.tsv",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    let (creation_times, expected): (Vec<&str>, Vec<&str>) = oracle
        .lines()
        .partition(|line| line.contains("\tolecf:item\tCreation Time\t"));
    assert_eq!(oracle.lines().count(), PLASO_EVENTS);
    assert_eq!(creation_times.len(), UNREAD_CREATION_TIMES);
    let expected: Vec<String> = expected
        .into_iter()
        .map(without_known_differences)
        .collect();
    assert_eq!(got, expected);
}

#[test]
fn what_plaso_gets_wrong_or_leaves_out() {
    let p = read("plaso/Document.doc").properties;
    assert_eq!(p.heading_pairs, [("Title".to_owned(), 1)]);
    assert_eq!(p.titles_of_parts, ["Table of Context"]);
    assert_eq!(p.edit_time, Some(Duration::ZERO));
    assert_eq!(p.links_dirty, Some(false));
}
