//! Arbitrary bytes, each test file cut anywhere, and random bytes changed
//! in them, read or are refused: never a panic.

use proptest::prelude::*;

fn fixture(path: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/tests/fixtures/{path}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

const FIXTURES: [&str; 3] = [
    "plaso/Document.doc",
    "plaso/Document.docx",
    "written/Presentation.pptx",
];

fn read_all_ways(data: &[u8]) {
    let _ = office::detect(data);
    let _ = office::read(data);
    let _ = office::ole::read_property_set(data);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn arbitrary_bytes(data in proptest::collection::vec(any::<u8>(), 0..4096)) {
        read_all_ways(&data);
    }

    #[test]
    fn arbitrary_bytes_after_a_signature(
        ole in any::<bool>(),
        tail in proptest::collection::vec(any::<u8>(), 0..4096),
    ) {
        let mut data = if ole {
            vec![0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1]
        } else {
            b"PK\x03\x04".to_vec()
        };
        data.extend_from_slice(&tail);
        read_all_ways(&data);
    }

    #[test]
    fn cut_anywhere(which in 0..FIXTURES.len(), fraction in 0.0f64..1.0) {
        let data = fixture(FIXTURES[which]);
        let end = (data.len() as f64 * fraction) as usize;
        read_all_ways(&data[..end]);
    }

    #[test]
    fn bytes_changed(
        which in 0..FIXTURES.len(),
        changes in proptest::collection::vec((0.0f64..1.0, any::<u8>()), 1..16),
    ) {
        let mut data = fixture(FIXTURES[which]);
        for (at, byte) in changes {
            let index = (data.len() as f64 * at) as usize;
            data[index] = byte;
        }
        read_all_ways(&data);
    }
}

/// The property set streams themselves, damaged: the compound file
/// reader is bypassed so every change reaches the property set parser.
#[test]
fn property_set_streams_damaged() {
    let data = fixture("plaso/Document.doc");
    let file = shell::compound::CompoundFile::parse(&data).unwrap();
    for name in ["\u{5}SummaryInformation", "\u{5}DocumentSummaryInformation"] {
        let stream = file.stream(name).unwrap().unwrap();
        for end in 0..=stream.len().min(512) {
            let _ = office::ole::read_property_set(&stream[..end]);
        }
        for at in 0..stream.len().min(512) {
            for byte in [0x00, 0x7f, 0xff] {
                let mut changed = stream.clone();
                changed[at] = byte;
                let _ = office::ole::read_property_set(&changed);
            }
        }
    }
}
