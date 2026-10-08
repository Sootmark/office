//! A document's metadata, the same fields from either format.

use std::time::Duration;

use common::time::Ts;

/// A document's metadata. Each field is `None` when the document doesn't
/// record it; text is kept as recorded, empty strings included.
///
/// Where the two formats name a field differently, the OLE property and the
/// OOXML element are given as `OLE / OOXML`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Properties {
    /// Title (`PIDSI_TITLE` / `dc:title`).
    pub title: Option<String>,
    /// Subject (`PIDSI_SUBJECT` / `dc:subject`).
    pub subject: Option<String>,
    /// Author: who created it (`PIDSI_AUTHOR` / `dc:creator`).
    pub author: Option<String>,
    /// Keywords (`PIDSI_KEYWORDS` / `cp:keywords`).
    pub keywords: Option<String>,
    /// Comments (`PIDSI_COMMENTS` / `dc:description`).
    pub comments: Option<String>,
    /// The template it was made from (`PIDSI_TEMPLATE` / `Template`).
    pub template: Option<String>,
    /// Who saved it last (`PIDSI_LASTAUTHOR` / `cp:lastModifiedBy`).
    pub last_saved_by: Option<String>,
    /// Revision number, as recorded (`PIDSI_REVNUMBER` / `cp:revision`).
    pub revision: Option<String>,
    /// When it was created (`PIDSI_CREATE_DTM` / `dcterms:created`).
    pub created: Option<Ts>,
    /// When it was last saved (`PIDSI_LASTSAVE_DTM` / `dcterms:modified`).
    pub modified: Option<Ts>,
    /// When it was last printed (`PIDSI_LASTPRINTED` / `cp:lastPrinted`).
    pub last_printed: Option<Ts>,
    /// Total editing time (`PIDSI_EDITTIME`, 100 ns units / `TotalTime`,
    /// minutes).
    pub edit_time: Option<Duration>,
    /// Pages (`PIDSI_PAGECOUNT` / `Pages`).
    pub pages: Option<i64>,
    /// Words (`PIDSI_WORDCOUNT` / `Words`).
    pub words: Option<i64>,
    /// Characters without spaces (`PIDSI_CHARCOUNT` / `Characters`).
    pub characters: Option<i64>,
    /// Characters with spaces (`PIDDSI_CCHWITHSPACES` /
    /// `CharactersWithSpaces`).
    pub characters_with_spaces: Option<i64>,
    /// Size in bytes, as the application counted it (`PIDDSI_BYTECOUNT`).
    pub bytes: Option<i64>,
    /// Lines (`PIDDSI_LINECOUNT` / `Lines`).
    pub lines: Option<i64>,
    /// Paragraphs (`PIDDSI_PARCOUNT` / `Paragraphs`).
    pub paragraphs: Option<i64>,
    /// Slides (`PIDDSI_SLIDECOUNT` / `Slides`).
    pub slides: Option<i64>,
    /// Slides with notes (`PIDDSI_NOTECOUNT` / `Notes`).
    pub notes: Option<i64>,
    /// Hidden slides (`PIDDSI_HIDDENCOUNT` / `HiddenSlides`).
    pub hidden_slides: Option<i64>,
    /// Multimedia clips (`PIDDSI_MMCLIPCOUNT` / `MMClips`).
    pub multimedia_clips: Option<i64>,
    /// The application that wrote it (`PIDSI_APPNAME` / `Application`).
    pub application: Option<String>,
    /// Its version: `major.minor` from `PIDDSI_VERSION`, `AppVersion` as
    /// written (`14.0000`).
    pub app_version: Option<String>,
    /// Company (`PIDDSI_COMPANY` / `Company`).
    pub company: Option<String>,
    /// Manager (`PIDDSI_MANAGER` / `Manager`).
    pub manager: Option<String>,
    /// Category (`PIDDSI_CATEGORY` / `cp:category`).
    pub category: Option<String>,
    /// Presentation format, such as "On-screen Show (4:3)"
    /// (`PIDDSI_PRESFORMAT` / `PresentationFormat`).
    pub presentation_format: Option<String>,
    /// Content type (`PIDDSI_CONTENTTYPE` / `cp:contentType`).
    pub content_type: Option<String>,
    /// Content status, such as "Draft" (`PIDDSI_CONTENTSTATUS` /
    /// `cp:contentStatus`).
    pub content_status: Option<String>,
    /// Language (`PIDDSI_LANGUAGE` / `dc:language`).
    pub language: Option<String>,
    /// Identifier (`dc:identifier`).
    pub identifier: Option<String>,
    /// Document version (`PIDDSI_DOCVERSION` / `cp:version`).
    pub version: Option<String>,
    /// Security flags: 1 password protected, 2 read-only recommended,
    /// 4 read-only enforced, 8 locked for annotations (`PIDSI_SECURITY` /
    /// `DocSecurity`).
    pub security: Option<i64>,
    /// Whether the thumbnail is scaled (`true`) or cropped (`PIDDSI_SCALE`
    /// / `ScaleCrop`).
    pub scale_crop: Option<bool>,
    /// Whether links need updating (`PIDDSI_LINKSDIRTY`, OLE only).
    pub links_dirty: Option<bool>,
    /// Whether links are up to date (`LinksUpToDate`, OOXML only).
    pub links_up_to_date: Option<bool>,
    /// Whether it is shared between several producers (`PIDDSI_SHAREDDOC`
    /// / `SharedDoc`).
    pub shared: Option<bool>,
    /// Whether hyperlinks were changed by another application
    /// (`PIDDSI_HLINKSCHANGED` / `HyperlinksChanged`).
    pub hyperlinks_changed: Option<bool>,
    /// The base of relative hyperlinks (`HyperlinkBase`, OOXML only).
    pub hyperlink_base: Option<String>,
    /// How many parts of each kind ("Title", 1; "Worksheets", 3)
    /// (`PIDDSI_HEADINGPAIR` / `HeadingPairs`).
    pub heading_pairs: Vec<(String, i64)>,
    /// The parts' titles: sheet names, slide titles (`PIDDSI_DOCPARTS` /
    /// `TitlesOfParts`).
    pub titles_of_parts: Vec<String>,
    /// The size of the thumbnail (`PIDSI_THUMBNAIL`), in bytes.
    pub thumbnail_size: Option<u32>,
    /// The code page of the summary information's strings (OLE only; the
    /// document summary information's when there is no summary
    /// information).
    pub codepage: Option<u16>,
}
