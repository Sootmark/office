//! Property set streams (Microsoft's MS-OLEPS), such as a compound file's
//! `\u{5}SummaryInformation` and `\u{5}DocumentSummaryInformation`.
//!
//! A 28-byte header (byte order `FE FF`, version, system identifier, class
//! identifier, number of sections), then each section's format identifier
//! (FMTID) and offset. A section: its size, its number of properties, then
//! one identifier and offset (from the section's start) per property. A
//! property: its type (`VT_*`), two bytes of padding and its value.
//! Property 1 is the code page of the section's 8-bit strings; property 0,
//! in the user-defined section, is a dictionary naming the others.
//!
//! Office packs the strings of a vector (titles of parts, heading pairs)
//! without padding between them; they are read that way here.

use common::bytes::{checked_count, Reader};
use common::time::Ts;
use common::win::guid_to_string;

use crate::codepage;
use crate::Error;

/// FMTID of the summary information section.
pub const SUMMARY_INFORMATION: &str = "f29f85e0-4ff9-1068-ab91-08002b27b3d9";
/// FMTID of the document summary information section.
pub const DOCUMENT_SUMMARY_INFORMATION: &str = "d5cdd502-2e9c-101b-9397-08002b2cf9ae";
/// FMTID of the user-defined (custom) properties section, the second
/// section of `\u{5}DocumentSummaryInformation`.
pub const USER_DEFINED_PROPERTIES: &str = "d5cdd505-2e9c-101b-9397-08002b2cf9ae";

/// Property 0: the dictionary of property names.
pub const DICTIONARY: u32 = 0;
/// Property 1: the code page of 8-bit strings.
pub const CODEPAGE: u32 = 1;

const BYTE_ORDER_MARK: u16 = 0xfffe;
const HEADER_SIZE: usize = 28;
const SECTION_ENTRY_SIZE: usize = 20;
const SECTION_HEADER_SIZE: usize = 8;
const PROPERTY_ENTRY_SIZE: usize = 8;
/// MS-OLEPS allows two sections; a few more are read, the rest reported.
const MAX_SECTIONS: u64 = 8;
const MAX_PROPERTIES: u64 = 4096;
const MAX_VECTOR: u64 = 65_536;
const MAX_DICTIONARY: u64 = 4096;
/// Variants inside vectors inside variants…
const MAX_NESTING: usize = 4;
/// Ten thousandths in a `VT_CY` currency unit.
const CURRENCY_SCALE: f64 = 10_000.0;

/// Property types (`VT_*`).
mod vt {
    pub const EMPTY: u16 = 0x00;
    pub const NULL: u16 = 0x01;
    pub const I2: u16 = 0x02;
    pub const I4: u16 = 0x03;
    pub const R4: u16 = 0x04;
    pub const R8: u16 = 0x05;
    pub const CY: u16 = 0x06;
    pub const DATE: u16 = 0x07;
    pub const BSTR: u16 = 0x08;
    pub const ERROR: u16 = 0x0a;
    pub const BOOL: u16 = 0x0b;
    pub const VARIANT: u16 = 0x0c;
    pub const I1: u16 = 0x10;
    pub const UI1: u16 = 0x11;
    pub const UI2: u16 = 0x12;
    pub const UI4: u16 = 0x13;
    pub const I8: u16 = 0x14;
    pub const UI8: u16 = 0x15;
    pub const INT: u16 = 0x16;
    pub const UINT: u16 = 0x17;
    pub const LPSTR: u16 = 0x1e;
    pub const LPWSTR: u16 = 0x1f;
    pub const FILETIME: u16 = 0x40;
    pub const BLOB: u16 = 0x41;
    pub const BLOB_OBJECT: u16 = 0x46;
    pub const CF: u16 = 0x47;
    pub const CLSID: u16 = 0x48;
    pub const VECTOR: u16 = 0x1000;
}

/// A property's value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// `VT_EMPTY`.
    Empty,
    /// `VT_NULL`.
    Null,
    /// A signed or 32-bit integer (`VT_I1` to `VT_I8`, `VT_UI1` to
    /// `VT_UI4`, `VT_INT`, `VT_UINT`, `VT_ERROR`).
    Int(i64),
    /// `VT_UI8`.
    UInt(u64),
    /// `VT_R4`, `VT_R8`, or `VT_CY` in currency units.
    Float(f64),
    /// `VT_BOOL`.
    Bool(bool),
    /// `VT_LPSTR` and `VT_BSTR` (in the section's code page), `VT_LPWSTR`.
    Text(String),
    /// `VT_FILETIME` as stored: a time, or for the edit time a duration,
    /// in 100-nanosecond units.
    Filetime(u64),
    /// `VT_DATE`.
    Date(Ts),
    /// `VT_BLOB`, `VT_BLOB_OBJECT`.
    Blob(Vec<u8>),
    /// `VT_CF` (the thumbnail): only the size of its format and data.
    ClipboardData(u32),
    /// `VT_CLSID`.
    Clsid(String),
    /// `VT_VECTOR` of any of these (`VT_VARIANT` elements carry their own
    /// type).
    Vector(Vec<Value>),
    /// A type not read: its `VT_*` number.
    Unsupported(u16),
}

impl core::fmt::Display for Value {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Empty => Ok(()),
            Self::Null => f.write_str("null"),
            Self::Int(n) => write!(f, "{n}"),
            Self::UInt(n) => write!(f, "{n}"),
            Self::Float(n) => write!(f, "{n}"),
            Self::Bool(b) => write!(f, "{b}"),
            Self::Text(text) | Self::Clsid(text) => f.write_str(text),
            Self::Filetime(raw) => match Ts::from_filetime(*raw).to_iso8601() {
                Some(time) => f.write_str(&time),
                None => write!(f, "{raw}"),
            },
            Self::Date(time) => write!(f, "{time}"),
            Self::Blob(bytes) => write!(f, "{} bytes", bytes.len()),
            Self::ClipboardData(size) => write!(f, "clipboard data, {size} bytes"),
            Self::Vector(items) => {
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        f.write_str("; ")?;
                    }
                    write!(f, "{item}")?;
                }
                Ok(())
            }
            Self::Unsupported(vt) => write!(f, "type 0x{vt:04x}"),
        }
    }
}

/// A property: its identifier and value.
#[derive(Debug, Clone, PartialEq)]
pub struct Property {
    /// Its identifier (`PIDSI_*`, `PIDDSI_*`, or user-defined).
    pub id: u32,
    /// Its value.
    pub value: Value,
}

/// A section of a property set.
#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    /// Its format identifier (FMTID), lowercase
    /// (`f29f85e0-4ff9-1068-ab91-08002b27b3d9`).
    pub fmtid: String,
    /// Its code page (property 1), when recorded.
    pub codepage: Option<u16>,
    /// The dictionary (property 0): names of user-defined properties.
    pub names: Vec<(u32, String)>,
    /// Its properties but the code page and the dictionary, in stored order.
    pub properties: Vec<Property>,
}

impl Section {
    /// The name of property `id` from the dictionary.
    #[must_use]
    pub fn name(&self, id: u32) -> Option<&str> {
        self.names
            .iter()
            .find(|(n, _)| *n == id)
            .map(|(_, name)| name.as_str())
    }
}

/// A property set stream.
#[derive(Debug, Clone, PartialEq)]
pub struct PropertySet {
    /// Its version (0 or 1).
    pub version: u16,
    /// The system identifier: the OS version that wrote it, in its low
    /// 16 bits (major, minor), the platform in its high 16 bits.
    pub system: u32,
    /// Its class identifier (usually zero).
    pub clsid: String,
    /// Its sections.
    pub sections: Vec<Section>,
    /// Damage met.
    pub problems: Vec<String>,
}

impl PropertySet {
    /// The section with format identifier `fmtid`.
    #[must_use]
    pub fn section(&self, fmtid: &str) -> Option<&Section> {
        self.sections.iter().find(|s| s.fmtid == fmtid)
    }
}

/// Read a property set stream.
///
/// # Errors
/// When it doesn't start with a property set header.
pub fn read_property_set(stream: &[u8]) -> Result<PropertySet, Error> {
    let mut r = Reader::new(stream);
    let header = || Error("not a property set (header truncated)".to_owned());
    if r.u16_le().map_err(|_| header())? != BYTE_ORDER_MARK {
        return Err(Error("not a property set (no FE FF byte order)".to_owned()));
    }
    let version = r.u16_le().map_err(|_| header())?;
    let system = r.u32_le().map_err(|_| header())?;
    let clsid = guid_to_string(&r.array::<16>().map_err(|_| header())?);
    let declared = r.u32_le().map_err(|_| header())?;
    let mut set = PropertySet {
        version,
        system,
        clsid,
        sections: Vec::new(),
        problems: Vec::new(),
    };
    let count = count_that_fits(
        declared,
        SECTION_ENTRY_SIZE,
        MAX_SECTIONS,
        stream,
        HEADER_SIZE,
    )
    .unwrap_or_else(|(fits, e)| {
        set.problems.push(format!("{declared} sections: {e}"));
        fits
    });
    for index in 0..count {
        match section_entry(&mut r) {
            Ok((fmtid, offset)) => {
                let section = read_section(stream, fmtid, offset as usize, &mut set.problems);
                set.sections.push(section);
            }
            Err(e) => set.problems.push(format!("section {index}: {e}")),
        }
    }
    Ok(set)
}

/// A section's format identifier and offset.
fn section_entry(r: &mut Reader<'_>) -> common::bytes::Result<(String, u32)> {
    let fmtid = guid_to_string(&r.array::<16>()?);
    Ok((fmtid, r.u32_le()?))
}

fn read_section(
    stream: &[u8],
    fmtid: String,
    offset: usize,
    problems: &mut Vec<String>,
) -> Section {
    let mut section = Section {
        fmtid,
        codepage: None,
        names: Vec::new(),
        properties: Vec::new(),
    };
    let Some(body) = section_body(stream, offset, &section.fmtid, problems) else {
        return section;
    };
    let entries = property_entries(body, &section.fmtid, problems);
    section.codepage = entries
        .iter()
        .find(|(id, _)| *id == CODEPAGE)
        .and_then(
            |&(_, at)| match value_at(body, at, codepage::WINDOWS_1252) {
                // A VT_I2 holding an unsigned code page: 65001 reads as -535.
                Ok(Value::Int(n)) => Some(n as u16),
                _ => None,
            },
        );
    let codepage = section.codepage.unwrap_or(codepage::WINDOWS_1252);
    if !codepage::supported(codepage) {
        problems.push(format!(
            "section {}: code page {codepage} not decoded: non-ASCII bytes escaped",
            section.fmtid
        ));
    }
    for (id, at) in entries {
        match id {
            CODEPAGE => {}
            DICTIONARY => match dictionary_at(body, at, codepage) {
                Ok(names) => section.names = names,
                Err(e) => problems.push(format!("section {} dictionary: {e}", section.fmtid)),
            },
            _ => match value_at(body, at, codepage) {
                Ok(value) => section.properties.push(Property { id, value }),
                Err(e) => {
                    problems.push(format!("section {} property 0x{id:x}: {e}", section.fmtid));
                }
            },
        }
    }
    section
}

/// The section's bytes: from `offset`, its declared size, cut at the
/// stream's end.
fn section_body<'a>(
    stream: &'a [u8],
    offset: usize,
    fmtid: &str,
    problems: &mut Vec<String>,
) -> Option<&'a [u8]> {
    let Some(rest) = stream.get(offset..) else {
        problems.push(format!(
            "section {fmtid}: offset {offset} past the stream's end"
        ));
        return None;
    };
    let Ok(size) = Reader::new(rest).u32_le() else {
        problems.push(format!("section {fmtid}: header truncated"));
        return None;
    };
    let size = size as usize;
    if size > rest.len() {
        problems.push(format!(
            "section {fmtid}: {size} bytes declared, {} left in the stream",
            rest.len()
        ));
    }
    rest.get(..size.min(rest.len()))
}

/// Each property's identifier and offset in the section.
fn property_entries(body: &[u8], fmtid: &str, problems: &mut Vec<String>) -> Vec<(u32, usize)> {
    let mut r = Reader::new(body);
    let declared = r.skip(4).and_then(|()| r.u32_le());
    let Ok(declared) = declared else {
        problems.push(format!("section {fmtid}: header truncated"));
        return Vec::new();
    };
    let count = count_that_fits(
        declared,
        PROPERTY_ENTRY_SIZE,
        MAX_PROPERTIES,
        body,
        SECTION_HEADER_SIZE,
    )
    .unwrap_or_else(|(fits, e)| {
        problems.push(format!("section {fmtid}: {declared} properties: {e}"));
        fits
    });
    (0..count)
        .map_while(|_| Some((r.u32_le().ok()?, r.u32_le().ok()? as usize)))
        .collect()
}

/// `declared` entries of `size` bytes from offset `at`, when `bytes` holds
/// them and they are at most `max`; else how many fit, and why.
fn count_that_fits(
    declared: u32,
    size: usize,
    max: u64,
    bytes: &[u8],
    at: usize,
) -> Result<usize, (usize, common::bytes::Error)> {
    let available = bytes.len().saturating_sub(at);
    checked_count(u64::from(declared), size, max, available, at)
        .map_err(|e| ((available / size).min(max as usize), e))
}

/// The typed value at `at` in the section.
fn value_at(body: &[u8], at: usize, codepage: u16) -> common::bytes::Result<Value> {
    let mut r = Reader::new(body);
    r.seek(at)?;
    typed_value(&mut r, codepage, 0)
}

/// A type, two bytes of padding, and a value of that type.
fn typed_value(r: &mut Reader<'_>, codepage: u16, depth: usize) -> common::bytes::Result<Value> {
    let vt = r.u16_le()?;
    r.skip(2)?;
    value_of_type(r, vt, codepage, depth)
}

fn value_of_type(
    r: &mut Reader<'_>,
    vt: u16,
    codepage: u16,
    depth: usize,
) -> common::bytes::Result<Value> {
    if vt & vt::VECTOR == 0 {
        return scalar(r, vt, codepage, depth);
    }
    if depth >= MAX_NESTING {
        return Ok(Value::Unsupported(vt));
    }
    let element = vt & !vt::VECTOR;
    let at = r.offset();
    let declared = r.u32_le()?;
    // Every element takes at least one byte.
    let count = checked_count(u64::from(declared), 1, MAX_VECTOR, r.remaining(), at)?;
    let mut items = Vec::with_capacity(count);
    for _ in 0..count {
        let item = value_of_type(r, element, codepage, depth + 1)?;
        if let Value::Unsupported(_) = item {
            // Its size is unknown: the rest can't be found.
            items.push(item);
            break;
        }
        items.push(item);
    }
    Ok(Value::Vector(items))
}

fn scalar(
    r: &mut Reader<'_>,
    vt: u16,
    codepage: u16,
    depth: usize,
) -> common::bytes::Result<Value> {
    Ok(match vt {
        vt::EMPTY => Value::Empty,
        vt::NULL => Value::Null,
        vt::I1 => Value::Int(i64::from(r.u8()? as i8)),
        vt::UI1 => Value::Int(i64::from(r.u8()?)),
        vt::I2 => Value::Int(i64::from(r.i16_le()?)),
        vt::UI2 => Value::Int(i64::from(r.u16_le()?)),
        vt::I4 | vt::INT => Value::Int(i64::from(r.i32_le()?)),
        vt::UI4 | vt::UINT | vt::ERROR => Value::Int(i64::from(r.u32_le()?)),
        vt::I8 => Value::Int(r.i64_le()?),
        vt::UI8 => Value::UInt(r.u64_le()?),
        vt::R4 => Value::Float(f64::from(f32::from_bits(r.u32_le()?))),
        vt::R8 => Value::Float(r.f64_le()?),
        vt::CY => Value::Float(r.i64_le()? as f64 / CURRENCY_SCALE),
        vt::DATE => Value::Date(Ts::from_ole_date(r.f64_le()?)),
        vt::BOOL => Value::Bool(r.u16_le()? != 0),
        vt::FILETIME => Value::Filetime(r.u64_le()?),
        vt::LPSTR | vt::BSTR => {
            let size = r.u32_le()? as usize;
            Value::Text(codepage::decode(r.bytes(size)?, codepage))
        }
        vt::LPWSTR => {
            let units = r.u32_le()? as usize;
            let bytes = r.bytes(units.saturating_mul(2))?;
            skip_padding(r);
            Value::Text(codepage::decode(bytes, codepage::UTF16))
        }
        vt::BLOB | vt::BLOB_OBJECT => {
            let size = r.u32_le()? as usize;
            Value::Blob(r.bytes(size)?.to_vec())
        }
        vt::CF => {
            let size = r.u32_le()?;
            r.skip(size as usize)?;
            Value::ClipboardData(size)
        }
        vt::CLSID => Value::Clsid(guid_to_string(&r.array::<16>()?)),
        vt::VARIANT if depth > 0 => typed_value(r, codepage, depth)?,
        other => Value::Unsupported(other),
    })
}

/// The dictionary: identifier and name pairs, names in the section's code
/// page (UTF-16 ones padded to 4 bytes).
fn dictionary_at(
    body: &[u8],
    at: usize,
    codepage: u16,
) -> common::bytes::Result<Vec<(u32, String)>> {
    let mut r = Reader::new(body);
    r.seek(at)?;
    let declared = r.u32_le()?;
    let count = checked_count(
        u64::from(declared),
        PROPERTY_ENTRY_SIZE,
        MAX_DICTIONARY,
        r.remaining(),
        r.offset(),
    )?;
    let unit = if codepage == codepage::UTF16 { 2 } else { 1 };
    let mut names = Vec::with_capacity(count);
    for _ in 0..count {
        let id = r.u32_le()?;
        let length = r.u32_le()? as usize;
        let bytes = r.bytes(length.saturating_mul(unit))?;
        if codepage == codepage::UTF16 {
            skip_padding(&mut r);
        }
        names.push((id, codepage::decode(bytes, codepage)));
    }
    Ok(names)
}

/// Skip to the next multiple of 4 bytes (or the end).
fn skip_padding(r: &mut Reader<'_>) {
    const ALIGNMENT: usize = 4;
    let padding = (ALIGNMENT - r.position() % ALIGNMENT) % ALIGNMENT;
    let _ = r.skip(padding.min(r.remaining()));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A property set of one section with these (id, type, value bytes).
    fn property_set(fmtid: &[u8; 16], properties: &[(u32, u16, Vec<u8>)]) -> Vec<u8> {
        let mut values = Vec::new();
        let mut entries = Vec::new();
        let table = SECTION_HEADER_SIZE + PROPERTY_ENTRY_SIZE * properties.len();
        for (id, vt, bytes) in properties {
            entries.extend_from_slice(&id.to_le_bytes());
            entries.extend_from_slice(&((table + values.len()) as u32).to_le_bytes());
            if *id != DICTIONARY {
                values.extend_from_slice(&vt.to_le_bytes());
                values.extend_from_slice(&[0, 0]);
            }
            values.extend_from_slice(bytes);
            while values.len() % 4 != 0 {
                values.push(0);
            }
        }
        let mut section = ((table + values.len()) as u32).to_le_bytes().to_vec();
        section.extend_from_slice(&(properties.len() as u32).to_le_bytes());
        section.extend_from_slice(&entries);
        section.extend_from_slice(&values);
        let mut stream = vec![0xfe, 0xff, 0, 0, 0x06, 0x01, 0x02, 0x00];
        stream.extend_from_slice(&[0; 16]);
        stream.extend_from_slice(&1u32.to_le_bytes());
        stream.extend_from_slice(fmtid);
        stream.extend_from_slice(&48u32.to_le_bytes());
        stream.extend_from_slice(&section);
        stream
    }

    const USER_DEFINED: [u8; 16] = [
        0x05, 0xd5, 0xcd, 0xd5, 0x9c, 0x2e, 0x1b, 0x10, 0x93, 0x97, 0x08, 0x00, 0x2b, 0x2c, 0xf9,
        0xae,
    ];

    fn sized(bytes: &[u8]) -> Vec<u8> {
        let mut out = (bytes.len() as u32).to_le_bytes().to_vec();
        out.extend_from_slice(bytes);
        out
    }

    #[test]
    fn utf16_section_with_a_dictionary() {
        let mut dictionary = 1u32.to_le_bytes().to_vec();
        dictionary.extend_from_slice(&2u32.to_le_bytes());
        dictionary.extend_from_slice(&7u32.to_le_bytes());
        dictionary.extend_from_slice(
            &"Client\0"
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>(),
        );
        let client: Vec<u8> = "Acme é\0"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        let set = property_set(
            &USER_DEFINED,
            &[
                (CODEPAGE, vt::I2, 1200u16.to_le_bytes().to_vec()),
                (DICTIONARY, 0, dictionary),
                (2, vt::LPSTR, sized(&client)),
            ],
        );
        let set = read_property_set(&set).unwrap();
        assert_eq!(set.problems, Vec::<String>::new());
        let section = set.section(USER_DEFINED_PROPERTIES).unwrap();
        assert_eq!(section.codepage, Some(1200));
        assert_eq!(section.name(2), Some("Client"));
        assert_eq!(
            section.properties[0].value,
            Value::Text("Acme é".to_owned())
        );
    }

    #[test]
    fn packed_vector_of_variants() {
        // Heading pairs as Word writes them: no padding after "Title\0".
        let mut pairs = 2u32.to_le_bytes().to_vec();
        pairs.extend_from_slice(&[0x1e, 0, 0, 0]);
        pairs.extend_from_slice(&sized(b"Title\0"));
        pairs.extend_from_slice(&[0x03, 0, 0, 0, 1, 0, 0, 0]);
        let set = property_set(&USER_DEFINED, &[(12, vt::VECTOR | vt::VARIANT, pairs)]);
        let set = read_property_set(&set).unwrap();
        assert_eq!(
            set.sections[0].properties[0].value,
            Value::Vector(vec![Value::Text("Title".to_owned()), Value::Int(1)])
        );
    }

    #[test]
    fn scalars() {
        let mut thumbnail = 8u32.to_le_bytes().to_vec();
        thumbnail.extend_from_slice(&[0xff; 8]);
        let set = property_set(
            &USER_DEFINED,
            &[
                (2, vt::BOOL, vec![0xff, 0xff]),
                (
                    3,
                    vt::FILETIME,
                    129_996_382_800_000_000u64.to_le_bytes().to_vec(),
                ),
                (4, vt::CF, thumbnail),
                (5, vt::LPWSTR, {
                    let mut text = 3u32.to_le_bytes().to_vec();
                    text.extend_from_slice(b"o\0k\0\0\0");
                    text
                }),
                (6, 0x0e, vec![0; 16]),
            ],
        );
        let set = read_property_set(&set).unwrap();
        let values: Vec<_> = set.sections[0]
            .properties
            .iter()
            .map(|p| p.value.clone())
            .collect();
        assert_eq!(
            values,
            [
                Value::Bool(true),
                Value::Filetime(129_996_382_800_000_000),
                Value::ClipboardData(8),
                Value::Text("ok".to_owned()),
                Value::Unsupported(0x0e),
            ]
        );
        assert_eq!(values[1].to_string(), "2012-12-10T18:38:00.0000000Z");
    }

    #[test]
    fn damage_is_reported() {
        let mut set = property_set(&USER_DEFINED, &[(2, vt::LPSTR, sized(b"text\0"))]);
        // The string claims more bytes than there are.
        let at = set.len() - 12;
        set[at..at + 4].copy_from_slice(&1000u32.to_le_bytes());
        let set = read_property_set(&set).unwrap();
        assert!(set.sections[0].properties.is_empty());
        assert_eq!(set.problems.len(), 1, "{:?}", set.problems);
        assert!(read_property_set(b"\xfe\xff").is_err());
        assert!(read_property_set(b"PK").is_err());
    }
}
