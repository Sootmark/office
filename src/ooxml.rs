//! Office Open XML packages (ECMA-376 Part 2, Open Packaging Conventions):
//! a zip archive with `[Content_Types].xml`, whose package relationships
//! (`_rels/.rels`) point to the core properties (`docProps/core.xml`), the
//! extended, application-specific ones (`docProps/app.xml`) and the custom
//! ones (`docProps/custom.xml`). Where the relationships don't name a part,
//! its usual path is tried.

use std::io::{Cursor, Read};
use std::time::Duration;

use zip::Archive;

use crate::xml::{self, Element};
use crate::{w3cdtf, Document, Error, Format, Properties};

const CONTENT_TYPES: &str = "[Content_Types].xml";
const PACKAGE_RELATIONSHIPS: &str = "_rels/.rels";
const CORE_PART: &str = "docProps/core.xml";
const APP_PART: &str = "docProps/app.xml";
const CUSTOM_PART: &str = "docProps/custom.xml";
/// Relationship types end with these (transitional and strict alike).
const CORE_RELATIONSHIP: &str = "/metadata/core-properties";
const APP_RELATIONSHIP: &str = "/extended-properties";
const CUSTOM_RELATIONSHIP: &str = "/custom-properties";
const EXTERNAL_TARGET: &str = "External";
/// The largest XML part read (property parts are a few kilobytes).
const MAX_PART_SIZE: u64 = 4 << 20;
const SECONDS_PER_MINUTE: u64 = 60;
const UTF8_BOM: &[u8] = b"\xef\xbb\xbf";
const UTF16LE_BOM: &[u8] = b"\xff\xfe";
const UTF16BE_BOM: &[u8] = b"\xfe\xff";

type Package<'a> = Archive<Cursor<&'a [u8]>>;

/// Whether `data` is a zip archive with `[Content_Types].xml`.
pub fn is_package(data: &[u8]) -> bool {
    Archive::open(Cursor::new(data)).is_ok_and(|package| find(&package, CONTENT_TYPES).is_some())
}

/// Read a package's core, extended and custom properties.
pub fn read(data: &[u8]) -> Result<Document, Error> {
    let mut package = Archive::open(Cursor::new(data)).map_err(|e| Error(format!("zip: {e}")))?;
    if find(&package, CONTENT_TYPES).is_none() {
        return Err(Error(format!(
            "not an Office Open XML package (no {CONTENT_TYPES})"
        )));
    }
    let mut document = Document::new(Format::OfficeOpenXml);
    let parts = Parts::locate(&mut package, &mut document.problems);
    if let Some(root) = read_xml(&mut package, &parts.core, &mut document.problems) {
        for element in &root.children {
            let result = core(
                &mut document.properties,
                element.local_name(),
                &element.text,
            );
            keep(result, "core", element, &mut document);
        }
    }
    if let Some(root) = read_xml(&mut package, &parts.app, &mut document.problems) {
        for element in &root.children {
            let result = app(&mut document.properties, element);
            keep(result, "app", element, &mut document);
        }
    }
    if let Some(root) = read_xml(&mut package, &parts.custom, &mut document.problems) {
        document.custom = custom(&root);
    }
    Ok(document)
}

/// Where the property parts are.
struct Parts {
    core: String,
    app: String,
    custom: String,
}

impl Parts {
    /// From the package relationships, else the usual paths.
    fn locate(package: &mut Package<'_>, problems: &mut Vec<String>) -> Self {
        let mut parts = Self {
            core: CORE_PART.to_owned(),
            app: APP_PART.to_owned(),
            custom: CUSTOM_PART.to_owned(),
        };
        let Some(root) = read_xml(package, PACKAGE_RELATIONSHIPS, problems) else {
            return parts;
        };
        for relationship in &root.children {
            let (Some(kind), Some(target)) = (
                relationship.attribute("Type"),
                relationship.attribute("Target"),
            ) else {
                continue;
            };
            if relationship.attribute("TargetMode") == Some(EXTERNAL_TARGET) {
                continue;
            }
            // Targets are relative to the package root, the relationships'
            // source; a leading '/' names the root too.
            let path = target.trim_start_matches('/').to_owned();
            if kind.ends_with(CORE_RELATIONSHIP) {
                parts.core = path;
            } else if kind.ends_with(APP_RELATIONSHIP) {
                parts.app = path;
            } else if kind.ends_with(CUSTOM_RELATIONSHIP) {
                parts.custom = path;
            }
        }
        parts
    }
}

/// The index of the entry named `path` (part names ignore ASCII case).
fn find(package: &Package<'_>, path: &str) -> Option<usize> {
    package
        .entries()
        .iter()
        .position(|e| e.name.eq_ignore_ascii_case(path))
}

/// The root element of the XML part at `path`, when there is one; damage to
/// `problems`.
fn read_xml(package: &mut Package<'_>, path: &str, problems: &mut Vec<String>) -> Option<Element> {
    let index = find(package, path)?;
    let size = package.entries().get(index)?.size;
    if size > MAX_PART_SIZE {
        problems.push(format!(
            "{path}: {size} bytes, more than the {MAX_PART_SIZE} read"
        ));
        return None;
    }
    let mut bytes = Vec::new();
    let read = package
        .reader(index)
        .and_then(|reader| reader.take(MAX_PART_SIZE).read_to_end(&mut bytes));
    if let Err(e) = read {
        problems.push(format!("{path}: {e}"));
        return None;
    }
    let tree = xml::parse(&text(&bytes, path, problems));
    if let Some(damage) = tree.damage {
        problems.push(format!("{path}: {damage}"));
    }
    tree.root
}

/// An XML part's text: UTF-8 or, after a byte order mark, UTF-16.
fn text(bytes: &[u8], path: &str, problems: &mut Vec<String>) -> String {
    if let Some(utf16) = bytes.strip_prefix(UTF16LE_BOM) {
        return common::text::utf16le(utf16).text;
    }
    if let Some(utf16) = bytes.strip_prefix(UTF16BE_BOM) {
        let swapped: Vec<u8> = utf16
            .chunks(2)
            .flat_map(|pair| pair.iter().rev().copied())
            .collect();
        return common::text::utf16le(&swapped).text;
    }
    let utf8 = bytes.strip_prefix(UTF8_BOM).unwrap_or(bytes);
    String::from_utf8(utf8.to_vec()).unwrap_or_else(|e| {
        problems.push(format!("{path}: not UTF-8 ({e}): invalid bytes replaced"));
        String::from_utf8_lossy(utf8).into_owned()
    })
}

/// Why an element wasn't read into a field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unread {
    /// No field holds it.
    Unknown,
    /// Its text isn't the value its field holds (a number, a time…).
    Malformed(&'static str),
}

/// An element not read into a field goes to `other`, as `part/name`
/// (elements with children only when malformed); a malformed one is also a
/// problem.
fn keep(result: Result<(), Unread>, part: &str, element: &Element, document: &mut Document) {
    let key = format!("{part}/{}", element.name);
    match result {
        Ok(()) => return,
        Err(Unread::Unknown) if !element.children.is_empty() => return,
        Err(Unread::Unknown) => {}
        Err(Unread::Malformed(kind)) => document
            .problems
            .push(format!("{key}: {:?} is not {kind}", element.text)),
    }
    document.other.push((key, element.text.clone()));
}

/// A core property (`docProps/core.xml`) into its field.
fn core(p: &mut Properties, name: &str, text: &str) -> Result<(), Unread> {
    match name {
        "title" => p.title = Some(text.to_owned()),
        "subject" => p.subject = Some(text.to_owned()),
        "creator" => p.author = Some(text.to_owned()),
        "keywords" => p.keywords = Some(text.to_owned()),
        "description" => p.comments = Some(text.to_owned()),
        "lastModifiedBy" => p.last_saved_by = Some(text.to_owned()),
        "revision" => p.revision = Some(text.to_owned()),
        "created" => p.created = Some(time(text)?),
        "modified" => p.modified = Some(time(text)?),
        "lastPrinted" => p.last_printed = Some(time(text)?),
        "category" => p.category = Some(text.to_owned()),
        "contentStatus" => p.content_status = Some(text.to_owned()),
        "contentType" => p.content_type = Some(text.to_owned()),
        "language" => p.language = Some(text.to_owned()),
        "identifier" => p.identifier = Some(text.to_owned()),
        "version" => p.version = Some(text.to_owned()),
        _ => return Err(Unread::Unknown),
    }
    Ok(())
}

/// An extended property (`docProps/app.xml`) into its field.
fn app(p: &mut Properties, element: &Element) -> Result<(), Unread> {
    let text = element.text.as_str();
    match element.local_name() {
        "Template" => p.template = Some(text.to_owned()),
        "Manager" => p.manager = Some(text.to_owned()),
        "Company" => p.company = Some(text.to_owned()),
        "Application" => p.application = Some(text.to_owned()),
        "AppVersion" => p.app_version = Some(text.to_owned()),
        "PresentationFormat" => p.presentation_format = Some(text.to_owned()),
        "HyperlinkBase" => p.hyperlink_base = Some(text.to_owned()),
        "TotalTime" => p.edit_time = Some(minutes(text)?),
        "Pages" => p.pages = Some(int(text)?),
        "Words" => p.words = Some(int(text)?),
        "Characters" => p.characters = Some(int(text)?),
        "CharactersWithSpaces" => p.characters_with_spaces = Some(int(text)?),
        "Lines" => p.lines = Some(int(text)?),
        "Paragraphs" => p.paragraphs = Some(int(text)?),
        "Slides" => p.slides = Some(int(text)?),
        "Notes" => p.notes = Some(int(text)?),
        "HiddenSlides" => p.hidden_slides = Some(int(text)?),
        "MMClips" => p.multimedia_clips = Some(int(text)?),
        "DocSecurity" => p.security = Some(int(text)?),
        "ScaleCrop" => p.scale_crop = Some(boolean(text)?),
        "LinksUpToDate" => p.links_up_to_date = Some(boolean(text)?),
        "SharedDoc" => p.shared = Some(boolean(text)?),
        "HyperlinksChanged" => p.hyperlinks_changed = Some(boolean(text)?),
        "HeadingPairs" => p.heading_pairs = heading_pairs(element)?,
        "TitlesOfParts" => p.titles_of_parts = vector(element).map(|e| e.text.clone()).collect(),
        _ => return Err(Unread::Unknown),
    }
    Ok(())
}

/// The elements of an element's `vt:vector`.
fn vector(element: &Element) -> impl Iterator<Item = &Element> {
    element
        .child("vector")
        .into_iter()
        .flat_map(|vector| vector.children.iter())
}

/// `HeadingPairs`: a vector of variants, alternately a name and a count.
fn heading_pairs(element: &Element) -> Result<Vec<(String, i64)>, Unread> {
    const MALFORMED: Unread = Unread::Malformed("a vector of name and count pairs");
    let values: Vec<&Element> = vector(element)
        .map(|variant| variant.children.first().ok_or(MALFORMED))
        .collect::<Result<_, _>>()?;
    if values.len() % 2 != 0 {
        return Err(MALFORMED);
    }
    values
        .chunks_exact(2)
        .map(|pair| match pair {
            [name, count] => Ok((name.text.clone(), int(&count.text).map_err(|_| MALFORMED)?)),
            _ => Err(MALFORMED),
        })
        .collect()
}

/// User-defined properties (`docProps/custom.xml`): each `property`'s name
/// and value (a vector's elements joined by "; ").
fn custom(root: &Element) -> Vec<(String, String)> {
    root.children
        .iter()
        .filter(|property| property.local_name() == "property")
        .map(|property| {
            let name = property.attribute("name").unwrap_or_default().to_owned();
            let value = property.children.first().map_or_else(String::new, |value| {
                if value.children.is_empty() {
                    value.text.clone()
                } else {
                    let items: Vec<&str> = value.children.iter().map(|e| e.text.as_str()).collect();
                    items.join("; ")
                }
            });
            (name, value)
        })
        .collect()
}

fn time(text: &str) -> Result<common::time::Ts, Unread> {
    w3cdtf::parse(text).ok_or(Unread::Malformed("a W3CDTF time"))
}

fn int(text: &str) -> Result<i64, Unread> {
    text.trim()
        .parse()
        .map_err(|_| Unread::Malformed("a number"))
}

/// `TotalTime`: minutes.
fn minutes(text: &str) -> Result<Duration, Unread> {
    const MALFORMED: Unread = Unread::Malformed("a number of minutes");
    let minutes: u64 = text.trim().parse().map_err(|_| MALFORMED)?;
    let seconds = minutes.checked_mul(SECONDS_PER_MINUTE).ok_or(MALFORMED)?;
    Ok(Duration::from_secs(seconds))
}

/// An `xsd:boolean`.
fn boolean(text: &str) -> Result<bool, Unread> {
    match text.trim() {
        "true" | "1" => Ok(true),
        "false" | "0" => Ok(false),
        _ => Err(Unread::Malformed("a boolean")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heading_pairs_and_parts() {
        let tree = xml::parse(
            "<Properties><HeadingPairs><vt:vector size=\"4\" baseType=\"variant\">\
             <vt:variant><vt:lpstr>Worksheets</vt:lpstr></vt:variant><vt:variant><vt:i4>2</vt:i4></vt:variant>\
             <vt:variant><vt:lpstr>Named Ranges</vt:lpstr></vt:variant><vt:variant><vt:i4>1</vt:i4></vt:variant>\
             </vt:vector></HeadingPairs><TitlesOfParts><vt:vector size=\"3\" baseType=\"lpstr\">\
             <vt:lpstr>Sheet1</vt:lpstr><vt:lpstr>Sheet2</vt:lpstr><vt:lpstr>Print_Area</vt:lpstr>\
             </vt:vector></TitlesOfParts><Pages>x</Pages></Properties>",
        );
        let root = tree.root.unwrap();
        let mut p = Properties::default();
        for element in &root.children {
            let _ = app(&mut p, element);
        }
        assert_eq!(
            p.heading_pairs,
            [("Worksheets".to_owned(), 2), ("Named Ranges".to_owned(), 1)]
        );
        assert_eq!(p.titles_of_parts, ["Sheet1", "Sheet2", "Print_Area"]);
        assert_eq!(
            app(&mut p, &root.children[2]),
            Err(Unread::Malformed("a number"))
        );
    }

    #[test]
    fn values() {
        assert_eq!(minutes("1385"), Ok(Duration::from_secs(1385 * 60)));
        assert!(minutes("-1").is_err());
        assert_eq!(boolean(" 1 "), Ok(true));
        assert!(boolean("yes").is_err());
    }

    #[test]
    fn custom_properties() {
        let tree = xml::parse(
            "<Properties><property fmtid=\"{D5CDD505-2E9C-101B-9397-08002B2CF9AE}\" pid=\"2\" name=\"Client\">\
             <vt:lpwstr>Acme</vt:lpwstr></property><property pid=\"3\" name=\"Tags\"><vt:vector>\
             <vt:lpwstr>a</vt:lpwstr><vt:lpwstr>b</vt:lpwstr></vt:vector></property></Properties>",
        );
        assert_eq!(
            custom(&tree.root.unwrap()),
            [
                ("Client".to_owned(), "Acme".to_owned()),
                ("Tags".to_owned(), "a; b".to_owned())
            ]
        );
    }
}
