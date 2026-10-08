//! Office 97–2003 documents and other compound files: the summary and
//! document summary information property sets, read into [`Properties`].
//!
//! The streams are looked up by name in the whole directory: the first one
//! in directory order is read (the reader doesn't keep the directory's
//! tree, so an embedded object's property sets can't be told from the
//! document's own; Office writes the document's first).

use std::time::Duration;

use common::time::{Ts, TICKS_PER_SECOND};
use shell::compound::{CompoundFile, Entry};

use crate::ole::{self, PropertySet, Section, Value};
use crate::{Document, Error, Format, Item, ItemKind, Properties};

const SUMMARY_STREAM: &str = "\u{5}SummaryInformation";
const DOCUMENT_SUMMARY_STREAM: &str = "\u{5}DocumentSummaryInformation";

/// Directory entry types (MS-CFB).
const STORAGE: u8 = 1;
const STREAM: u8 = 2;
const ROOT: u8 = 5;

/// Nanoseconds in a FILETIME unit.
const NANOS_PER_TICK: u64 = 100;

/// Summary information property identifiers (`PIDSI_*`).
mod pidsi {
    pub const TITLE: u32 = 0x02;
    pub const SUBJECT: u32 = 0x03;
    pub const AUTHOR: u32 = 0x04;
    pub const KEYWORDS: u32 = 0x05;
    pub const COMMENTS: u32 = 0x06;
    pub const TEMPLATE: u32 = 0x07;
    pub const LAST_AUTHOR: u32 = 0x08;
    pub const REVISION_NUMBER: u32 = 0x09;
    pub const EDIT_TIME: u32 = 0x0a;
    pub const LAST_PRINTED: u32 = 0x0b;
    pub const CREATED: u32 = 0x0c;
    pub const LAST_SAVED: u32 = 0x0d;
    pub const PAGE_COUNT: u32 = 0x0e;
    pub const WORD_COUNT: u32 = 0x0f;
    pub const CHARACTER_COUNT: u32 = 0x10;
    pub const THUMBNAIL: u32 = 0x11;
    pub const APPLICATION_NAME: u32 = 0x12;
    pub const SECURITY: u32 = 0x13;
}

/// Document summary information property identifiers (`PIDDSI_*`).
mod piddsi {
    pub const CATEGORY: u32 = 0x02;
    pub const PRESENTATION_FORMAT: u32 = 0x03;
    pub const BYTE_COUNT: u32 = 0x04;
    pub const LINE_COUNT: u32 = 0x05;
    pub const PARAGRAPH_COUNT: u32 = 0x06;
    pub const SLIDE_COUNT: u32 = 0x07;
    pub const NOTE_COUNT: u32 = 0x08;
    pub const HIDDEN_COUNT: u32 = 0x09;
    pub const MULTIMEDIA_CLIP_COUNT: u32 = 0x0a;
    pub const SCALE: u32 = 0x0b;
    pub const HEADING_PAIRS: u32 = 0x0c;
    pub const DOCUMENT_PARTS: u32 = 0x0d;
    pub const MANAGER: u32 = 0x0e;
    pub const COMPANY: u32 = 0x0f;
    pub const LINKS_DIRTY: u32 = 0x10;
    pub const CHARACTERS_WITH_SPACES: u32 = 0x11;
    pub const SHARED_DOCUMENT: u32 = 0x13;
    pub const HYPERLINKS_CHANGED: u32 = 0x16;
    pub const VERSION: u32 = 0x17;
    pub const CONTENT_TYPE: u32 = 0x1a;
    pub const CONTENT_STATUS: u32 = 0x1b;
    pub const LANGUAGE: u32 = 0x1c;
    pub const DOCUMENT_VERSION: u32 = 0x1d;
}

/// Whether a compound file has a summary or document summary information
/// stream.
pub fn has_property_sets(data: &[u8]) -> bool {
    CompoundFile::parse(data).is_ok_and(|file| {
        file.entries.iter().any(|e| {
            e.kind == STREAM && (e.name == SUMMARY_STREAM || e.name == DOCUMENT_SUMMARY_STREAM)
        })
    })
}

/// Read a compound file's items and property sets.
pub fn read(data: &[u8]) -> Result<Document, Error> {
    let file = CompoundFile::parse(data).map_err(|e| Error(format!("compound file: {e}")))?;
    let mut document = Document::new(Format::Ole);
    document.items = file.entries.iter().map(item).collect();
    for name in [SUMMARY_STREAM, DOCUMENT_SUMMARY_STREAM] {
        if let Some(set) = property_set(&file, name, &mut document.problems) {
            apply(&set, &mut document);
        }
    }
    Ok(document)
}

fn item(entry: &Entry) -> Item {
    Item {
        name: entry.name.clone(),
        kind: match entry.kind {
            STORAGE => ItemKind::Storage,
            STREAM => ItemKind::Stream,
            ROOT => ItemKind::Root,
            other => ItemKind::Other(other),
        },
        size: entry.size,
        modified: Ts::from_filetime(entry.modified),
    }
}

/// The property set in stream `name`; damage to `problems`.
fn property_set(
    file: &CompoundFile<'_>,
    name: &str,
    problems: &mut Vec<String>,
) -> Option<PropertySet> {
    let label = stream_label(name);
    let stream = match file.stream(name) {
        Ok(stream) => stream?,
        Err(e) => {
            problems.push(format!("{label}: {e}"));
            return None;
        }
    };
    match ole::read_property_set(&stream) {
        Ok(set) => {
            problems.extend(set.problems.iter().map(|p| format!("{label}: {p}")));
            Some(set)
        }
        Err(e) => {
            problems.push(format!("{label}: {e}"));
            None
        }
    }
}

/// A stream's name without its leading control character.
fn stream_label(name: &str) -> &str {
    name.trim_start_matches('\u{5}')
}

/// Every section of a property set into the document.
fn apply(set: &PropertySet, document: &mut Document) {
    for section in &set.sections {
        if document.properties.codepage.is_none() {
            document.properties.codepage = section.codepage;
        }
        match section.fmtid.as_str() {
            ole::USER_DEFINED_PROPERTIES => custom(section, document),
            ole::SUMMARY_INFORMATION => known(section, "SummaryInformation", summary, document),
            ole::DOCUMENT_SUMMARY_INFORMATION => {
                known(
                    section,
                    "DocumentSummaryInformation",
                    document_summary,
                    document,
                );
            }
            fmtid => known(section, fmtid, |_, _, value| Err(value), document),
        }
    }
}

/// User-defined properties, named by the dictionary.
fn custom(section: &Section, document: &mut Document) {
    for property in &section.properties {
        let name = section
            .name(property.id)
            .map_or_else(|| format!("0x{:04x}", property.id), str::to_owned);
        document.custom.push((name, property.value.to_string()));
    }
}

/// A section whose properties `field` reads into the document's fields;
/// those it gives back go to `other` as `label/0x…`.
fn known(
    section: &Section,
    label: &str,
    field: fn(&mut Properties, u32, Value) -> Result<(), Value>,
    document: &mut Document,
) {
    for property in &section.properties {
        if let Err(value) = field(
            &mut document.properties,
            property.id,
            property.value.clone(),
        ) {
            document
                .other
                .push((format!("{label}/0x{:04x}", property.id), value.to_string()));
        }
    }
}

/// A summary information property into its field; others given back.
fn summary(p: &mut Properties, id: u32, value: Value) -> Result<(), Value> {
    let set = match id {
        pidsi::TITLE => text(&value).map(|v| p.title = Some(v)),
        pidsi::SUBJECT => text(&value).map(|v| p.subject = Some(v)),
        pidsi::AUTHOR => text(&value).map(|v| p.author = Some(v)),
        pidsi::KEYWORDS => text(&value).map(|v| p.keywords = Some(v)),
        pidsi::COMMENTS => text(&value).map(|v| p.comments = Some(v)),
        pidsi::TEMPLATE => text(&value).map(|v| p.template = Some(v)),
        pidsi::LAST_AUTHOR => text(&value).map(|v| p.last_saved_by = Some(v)),
        pidsi::REVISION_NUMBER => text(&value).map(|v| p.revision = Some(v)),
        pidsi::EDIT_TIME => duration(&value).map(|v| p.edit_time = Some(v)),
        pidsi::LAST_PRINTED => time(&value).map(|v| p.last_printed = Some(v)),
        pidsi::CREATED => time(&value).map(|v| p.created = Some(v)),
        pidsi::LAST_SAVED => time(&value).map(|v| p.modified = Some(v)),
        pidsi::PAGE_COUNT => int(&value).map(|v| p.pages = Some(v)),
        pidsi::WORD_COUNT => int(&value).map(|v| p.words = Some(v)),
        pidsi::CHARACTER_COUNT => int(&value).map(|v| p.characters = Some(v)),
        pidsi::THUMBNAIL => match value {
            Value::ClipboardData(size) => {
                p.thumbnail_size = Some(size);
                Some(())
            }
            _ => None,
        },
        pidsi::APPLICATION_NAME => text(&value).map(|v| p.application = Some(v)),
        pidsi::SECURITY => int(&value).map(|v| p.security = Some(v)),
        _ => None,
    };
    set.ok_or(value)
}

/// A document summary information property into its field; others given
/// back.
fn document_summary(p: &mut Properties, id: u32, value: Value) -> Result<(), Value> {
    let set = match id {
        piddsi::CATEGORY => text(&value).map(|v| p.category = Some(v)),
        piddsi::PRESENTATION_FORMAT => text(&value).map(|v| p.presentation_format = Some(v)),
        piddsi::BYTE_COUNT => int(&value).map(|v| p.bytes = Some(v)),
        piddsi::LINE_COUNT => int(&value).map(|v| p.lines = Some(v)),
        piddsi::PARAGRAPH_COUNT => int(&value).map(|v| p.paragraphs = Some(v)),
        piddsi::SLIDE_COUNT => int(&value).map(|v| p.slides = Some(v)),
        piddsi::NOTE_COUNT => int(&value).map(|v| p.notes = Some(v)),
        piddsi::HIDDEN_COUNT => int(&value).map(|v| p.hidden_slides = Some(v)),
        piddsi::MULTIMEDIA_CLIP_COUNT => int(&value).map(|v| p.multimedia_clips = Some(v)),
        piddsi::SCALE => boolean(&value).map(|v| p.scale_crop = Some(v)),
        piddsi::HEADING_PAIRS => heading_pairs(&value).map(|v| p.heading_pairs = v),
        piddsi::DOCUMENT_PARTS => texts(&value).map(|v| p.titles_of_parts = v),
        piddsi::MANAGER => text(&value).map(|v| p.manager = Some(v)),
        piddsi::COMPANY => text(&value).map(|v| p.company = Some(v)),
        piddsi::LINKS_DIRTY => boolean(&value).map(|v| p.links_dirty = Some(v)),
        piddsi::CHARACTERS_WITH_SPACES => int(&value).map(|v| p.characters_with_spaces = Some(v)),
        piddsi::SHARED_DOCUMENT => boolean(&value).map(|v| p.shared = Some(v)),
        piddsi::HYPERLINKS_CHANGED => boolean(&value).map(|v| p.hyperlinks_changed = Some(v)),
        piddsi::VERSION => int(&value).map(|v| p.app_version = Some(app_version(v))),
        piddsi::CONTENT_TYPE => text(&value).map(|v| p.content_type = Some(v)),
        piddsi::CONTENT_STATUS => text(&value).map(|v| p.content_status = Some(v)),
        piddsi::LANGUAGE => text(&value).map(|v| p.language = Some(v)),
        piddsi::DOCUMENT_VERSION => text(&value).map(|v| p.version = Some(v)),
        _ => None,
    };
    set.ok_or(value)
}

fn text(value: &Value) -> Option<String> {
    match value {
        Value::Text(text) => Some(text.clone()),
        _ => None,
    }
}

fn int(value: &Value) -> Option<i64> {
    match value {
        Value::Int(n) => Some(*n),
        _ => None,
    }
}

fn boolean(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(b) => Some(*b),
        _ => None,
    }
}

fn time(value: &Value) -> Option<Ts> {
    match value {
        Value::Filetime(raw) => Some(Ts::from_filetime(*raw)),
        _ => None,
    }
}

/// A FILETIME read as a duration (the total editing time).
fn duration(value: &Value) -> Option<Duration> {
    const TICKS_PER_SECOND_U64: u64 = TICKS_PER_SECOND as u64;
    match value {
        Value::Filetime(ticks) => Some(Duration::new(
            ticks / TICKS_PER_SECOND_U64,
            (ticks % TICKS_PER_SECOND_U64 * NANOS_PER_TICK) as u32,
        )),
        _ => None,
    }
}

/// A vector of strings.
fn texts(value: &Value) -> Option<Vec<String>> {
    match value {
        Value::Vector(items) => items.iter().map(text).collect(),
        _ => None,
    }
}

/// A vector of (name, count) variant pairs.
fn heading_pairs(value: &Value) -> Option<Vec<(String, i64)>> {
    match value {
        Value::Vector(items) if items.len() % 2 == 0 => items
            .chunks_exact(2)
            .map(|pair| Some((text(pair.first()?)?, int(pair.get(1)?)?)))
            .collect(),
        _ => None,
    }
}

/// `PIDDSI_VERSION`: the major version in the high 16 bits, the minor in
/// the low.
fn app_version(packed: i64) -> String {
    const HALF: u32 = 16;
    const LOW_HALF: i64 = 0xffff;
    format!("{}.{}", (packed >> HALF) & LOW_HALF, packed & LOW_HALF)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_halves() {
        assert_eq!(app_version(0x000e_0000), "14.0");
        assert_eq!(app_version(0x000c_0002), "12.2");
    }

    #[test]
    fn edit_time_is_a_duration() {
        let minutes = 1385;
        let ticks = minutes * 60 * TICKS_PER_SECOND as u64;
        assert_eq!(
            duration(&Value::Filetime(ticks)),
            Some(Duration::from_secs(minutes * 60))
        );
        assert_eq!(
            duration(&Value::Filetime(15)),
            Some(Duration::from_nanos(1500))
        );
    }

    #[test]
    fn user_defined_and_unknown_properties() {
        let section = |fmtid: &str, properties: Vec<ole::Property>| Section {
            fmtid: fmtid.to_owned(),
            codepage: Some(1252),
            names: vec![(2, "Client".to_owned())],
            properties,
        };
        let property = |id, value| ole::Property { id, value };
        let set = PropertySet {
            version: 0,
            system: 0,
            clsid: String::new(),
            sections: vec![
                section(
                    ole::DOCUMENT_SUMMARY_INFORMATION,
                    vec![
                        property(piddsi::CATEGORY, Value::Text("Memo".into())),
                        property(0x15, Value::Blob(vec![1, 2])),
                    ],
                ),
                section(
                    ole::USER_DEFINED_PROPERTIES,
                    vec![
                        property(2, Value::Text("Acme".into())),
                        property(3, Value::Bool(true)),
                    ],
                ),
            ],
            problems: Vec::new(),
        };
        let mut document = Document::new(Format::Ole);
        apply(&set, &mut document);
        assert_eq!(document.properties.category.as_deref(), Some("Memo"));
        assert_eq!(document.properties.codepage, Some(1252));
        let pairs = |list: &[(&str, &str)]| {
            list.iter()
                .map(|&(a, b)| (a.to_owned(), b.to_owned()))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            document.other,
            pairs(&[("DocumentSummaryInformation/0x0015", "2 bytes")])
        );
        assert_eq!(
            document.custom,
            pairs(&[("Client", "Acme"), ("0x0003", "true")])
        );
    }

    #[test]
    fn unexpected_types_are_given_back() {
        let mut p = Properties::default();
        assert_eq!(
            summary(&mut p, pidsi::TITLE, Value::Int(3)),
            Err(Value::Int(3))
        );
        assert_eq!(summary(&mut p, 0x99, Value::Int(3)), Err(Value::Int(3)));
        assert_eq!(
            summary(&mut p, pidsi::TITLE, Value::Text("t".into())),
            Ok(())
        );
        assert_eq!(p.title.as_deref(), Some("t"));
    }
}
