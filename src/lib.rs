//! Office documents' metadata for forensics: who wrote a document and who
//! saved it last, when it was created, saved and printed, how long it was
//! edited, its counts, company, manager and template. From Office 97–2003
//! files (`.doc`, `.xls`, `.ppt`: compound files) and Office Open XML
//! (`.docx`, `.xlsx`, `.pptx`: zip packages).
//!
//! **Compound files** (OLE, Microsoft's MS-CFB) keep it in two property
//! set streams (MS-OLEPS, see [`ole`]): `\u{5}SummaryInformation` (title,
//! author, times, counts, thumbnail, application) and
//! `\u{5}DocumentSummaryInformation` (category, company, manager, more
//! counts, the parts of the document, the application's version; its
//! second section holds the user-defined properties), read from the root
//! storage (an embedded object's are its own). The file's storages and
//! streams, with their paths and their creation and modification times,
//! are listed too.
//!
//! **Office Open XML** packages keep it in XML parts found through
//! `_rels/.rels`: `docProps/core.xml` (Dublin Core: title, creator,
//! created and modified times, last modified by, revision),
//! `docProps/app.xml` (application, template, editing time, counts,
//! company) and `docProps/custom.xml` (user-defined properties).
//!
//! ```no_run
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let document = office::read(&std::fs::read("report.docx")?)?;
//! let p = &document.properties;
//! println!("{:?} by {:?}, last saved by {:?}", p.title, p.author, p.last_saved_by);
//! println!("created {:?}, saved {:?}, edited {:?}", p.created, p.modified, p.edit_time);
//! # Ok(())
//! # }
//! ```

mod codepage;
mod compound;
pub mod ole;
mod ooxml;
mod properties;
mod w3cdtf;
mod xml;

use common::time::Ts;

pub use properties::Properties;

/// This crate's version, for records of what parsed them.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// A compound file's first bytes.
const OLE_SIGNATURE: [u8; 8] = [0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1];
/// A zip archive's first bytes (a local file header).
const ZIP_SIGNATURE: [u8; 4] = *b"PK\x03\x04";

/// A document's format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// A compound file (Office 97–2003 and other OLE files).
    Ole,
    /// An Office Open XML package (`.docx`, `.xlsx`, `.pptx`).
    OfficeOpenXml,
}

/// A storage or stream of a compound file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// Its name.
    pub name: String,
    /// The names from the root's down to its own, joined by `/`
    /// (`Root Entry/MsoDataStore`); an item the directory tree doesn't reach
    /// from the root has its name alone.
    pub path: String,
    /// Storage, stream or root.
    pub kind: ItemKind,
    /// Its size in bytes (the root's is that of the mini stream).
    pub size: u64,
    /// When it was created (storages; not recorded for streams and usually
    /// the root).
    pub created: Option<Ts>,
    /// When it was last modified (storages and the root; not recorded for
    /// streams).
    pub modified: Option<Ts>,
}

/// What a compound file's item is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    /// A storage (a folder).
    Storage,
    /// A stream (a file).
    Stream,
    /// The root storage.
    Root,
    /// Another directory entry type.
    Other(u8),
}

/// A document's metadata and what couldn't be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    /// Its format.
    pub format: Format,
    /// The properties read into fields.
    pub properties: Properties,
    /// User-defined properties: name and value (the user-defined section of
    /// `\u{5}DocumentSummaryInformation`, or `docProps/custom.xml`).
    pub custom: Vec<(String, String)>,
    /// Other properties, without a field: `SummaryInformation/0x0019`,
    /// `DocumentSummaryInformation/0x0015`, `core/dcterms:available`; a
    /// package's elements with children are left out.
    pub other: Vec<(String, String)>,
    /// A compound file's storages and streams, in directory order (none for
    /// Office Open XML).
    pub items: Vec<Item>,
    /// Damage met.
    pub problems: Vec<String>,
}

impl Document {
    fn new(format: Format) -> Self {
        Self {
            format,
            properties: Properties::default(),
            custom: Vec::new(),
            other: Vec::new(),
            items: Vec::new(),
            problems: Vec::new(),
        }
    }
}

/// Why a file can't be read as an Office document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

/// Whether `data` is a compound file with a property set stream, or an
/// Office Open XML package.
#[must_use]
pub fn detect(data: &[u8]) -> bool {
    if data.starts_with(&OLE_SIGNATURE) {
        compound::has_property_sets(data)
    } else if data.starts_with(&ZIP_SIGNATURE) {
        ooxml::is_package(data)
    } else {
        false
    }
}

/// Read a document's metadata.
///
/// # Errors
/// When it is neither a compound file nor an Office Open XML package, or
/// its directory can't be read.
pub fn read(data: &[u8]) -> Result<Document, Error> {
    if data.starts_with(&OLE_SIGNATURE) {
        compound::read(data)
    } else if data.starts_with(&ZIP_SIGNATURE) {
        ooxml::read(data)
    } else {
        Err(Error(
            "not an Office document (neither a compound file nor a zip package)".to_owned(),
        ))
    }
}
