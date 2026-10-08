//! Property sets are read from the root storage only: plaso's
//! `Document.doc` with its two property set streams renamed, and a stream
//! of an embedded storage renamed `\u{5}SummaryInformation` in their place.

const SUMMARY: &str = "\u{5}SummaryInformation";
const DOCUMENT_SUMMARY: &str = "\u{5}DocumentSummaryInformation";
/// Bytes of a directory entry's name field, and where its length sits.
const NAME_FIELD: usize = 64;

fn fixture(path: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/tests/fixtures/{path}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

/// A name as a directory entry stores it: UTF-16LE with a terminating null.
fn stored(name: &str) -> Vec<u8> {
    name.encode_utf16()
        .chain([0])
        .flat_map(u16::to_le_bytes)
        .collect()
}

/// Rename the directory entry named `from` to `to`: its name field, found
/// by its bytes and its length, rewritten.
fn rename(data: &mut [u8], from: &str, to: &str) {
    let (old, new) = (stored(from), stored(to));
    let at = (0..data.len() - NAME_FIELD - 2)
        .find(|&at| {
            data[at..].starts_with(&old)
                && usize::from(u16::from_le_bytes([
                    data[at + NAME_FIELD],
                    data[at + NAME_FIELD + 1],
                ])) == old.len()
        })
        .unwrap();
    data[at..at + NAME_FIELD].fill(0);
    data[at..at + new.len()].copy_from_slice(&new);
    data[at + NAME_FIELD..at + NAME_FIELD + 2]
        .copy_from_slice(&u16::try_from(new.len()).unwrap().to_le_bytes());
}

#[test]
fn an_embedded_storages_property_set_is_not_the_documents() {
    let mut data = fixture("plaso/Document.doc");
    rename(&mut data, SUMMARY, "SummaryInformation");
    rename(&mut data, DOCUMENT_SUMMARY, "DocumentSummaryInformation");
    rename(&mut data, "Properties", SUMMARY);
    assert!(!office::detect(&data));
    let document = office::read(&data).unwrap();
    assert_eq!(document.problems, Vec::<String>::new());
    assert_eq!(document.properties, office::Properties::default());
    let embedded: Vec<&str> = document
        .items
        .iter()
        .filter(|item| item.name == SUMMARY)
        .map(|item| item.path.as_str())
        .collect();
    assert_eq!(embedded.len(), 1);
    assert!(embedded[0].starts_with("Root Entry/MsoDataStore/"));
}

#[test]
fn items_have_their_paths_and_times() {
    let document = office::read(&fixture("plaso/Document.doc")).unwrap();
    let store = document
        .items
        .iter()
        .find(|item| item.path == "Root Entry/MsoDataStore")
        .unwrap();
    let iso = |time: Option<common::time::Ts>| time.and_then(|t| t.to_iso8601());
    assert_eq!(
        iso(store.created).as_deref(),
        Some("2013-05-16T02:29:49.7040000Z")
    );
    assert_eq!(
        iso(store.modified).as_deref(),
        Some("2013-05-16T02:29:49.7850000Z")
    );
    let summary = document
        .items
        .iter()
        .find(|item| item.path == format!("Root Entry/{SUMMARY}"))
        .unwrap();
    assert_eq!((summary.created, summary.modified), (None, None));
}
