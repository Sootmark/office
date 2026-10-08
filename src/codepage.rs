//! The 8-bit strings of a property set, decoded by its code page
//! (property 1). Windows-1252, UTF-16 (1200), UTF-8, Latin-1 and US-ASCII
//! are decoded; for any other code page ASCII is kept and every other byte
//! is escaped as `\x{XX}`, so nothing is lost or guessed.

use core::fmt::Write;

/// UTF-16LE, Windows' "Unicode" code page.
pub const UTF16: u16 = 1200;
/// Windows-1252 (Western European), the usual code page of English and
/// Western European documents.
pub const WINDOWS_1252: u16 = 1252;
/// ISO 8859-1.
const LATIN1: u16 = 28591;
/// 7-bit ASCII.
const US_ASCII: u16 = 20127;
/// UTF-8.
const UTF8: u16 = 65001;

/// The first byte Windows-1252 maps differently from Latin-1.
const WINDOWS_1252_SPECIAL_START: u8 = 0x80;

/// Windows-1252's 0x80 to 0x9F. Its five unassigned bytes map to the C1
/// control of the same value, as Windows maps them.
const WINDOWS_1252_SPECIAL: [char; 32] = [
    '\u{20ac}', '\u{0081}', '\u{201a}', '\u{0192}', '\u{201e}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{02c6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{008d}', '\u{017d}', '\u{008f}',
    '\u{0090}', '\u{2018}', '\u{2019}', '\u{201c}', '\u{201d}', '\u{2022}', '\u{2013}', '\u{2014}',
    '\u{02dc}', '\u{2122}', '\u{0161}', '\u{203a}', '\u{0153}', '\u{009d}', '\u{017e}', '\u{0178}',
];

/// Whether strings in `codepage` are decoded (not escaped).
pub fn supported(codepage: u16) -> bool {
    matches!(codepage, UTF16 | WINDOWS_1252 | LATIN1 | US_ASCII | UTF8)
}

/// The text of `bytes` in `codepage`, up to its first NUL.
pub fn decode(bytes: &[u8], codepage: u16) -> String {
    if codepage == UTF16 {
        return common::text::utf16le_until_nul(bytes).text;
    }
    let bytes = bytes
        .iter()
        .position(|&b| b == 0)
        .map_or(bytes, |nul| bytes.get(..nul).unwrap_or(bytes));
    match codepage {
        WINDOWS_1252 => bytes.iter().map(|&b| windows_1252(b)).collect(),
        LATIN1 => bytes.iter().map(|&b| char::from(b)).collect(),
        UTF8 => utf8(bytes),
        _ => ascii(bytes),
    }
}

fn windows_1252(byte: u8) -> char {
    byte.checked_sub(WINDOWS_1252_SPECIAL_START)
        .and_then(|i| WINDOWS_1252_SPECIAL.get(usize::from(i)))
        .copied()
        .unwrap_or(char::from(byte))
}

/// UTF-8, invalid bytes escaped.
fn utf8(mut bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len());
    loop {
        match core::str::from_utf8(bytes) {
            Ok(valid) => {
                text.push_str(valid);
                return text;
            }
            Err(e) => {
                let (valid, rest) = bytes.split_at(e.valid_up_to());
                text.push_str(core::str::from_utf8(valid).unwrap_or_default());
                let bad = e.error_len().unwrap_or(rest.len());
                let (invalid, after) = rest.split_at(bad.min(rest.len()));
                escape_into(&mut text, invalid);
                bytes = after;
            }
        }
    }
}

/// ASCII kept, every other byte escaped.
fn ascii(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len());
    for &byte in bytes {
        if byte.is_ascii() {
            text.push(char::from(byte));
        } else {
            escape_into(&mut text, &[byte]);
        }
    }
    text
}

fn escape_into(text: &mut String, bytes: &[u8]) {
    for byte in bytes {
        let _ = write!(text, "\\x{{{byte:02x}}}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_1252_maps_its_special_range() {
        assert_eq!(
            decode(b"\x80 caf\xe9 \x93ok\x94\0junk", WINDOWS_1252),
            "\u{20ac} café \u{201c}ok\u{201d}"
        );
    }

    #[test]
    fn utf16_stops_at_nul() {
        assert_eq!(decode(b"a\0\xe9\0\0\0b\0", UTF16), "aé");
    }

    #[test]
    fn utf8_escapes_bad_bytes() {
        assert_eq!(decode("é\u{2014}".as_bytes(), UTF8), "é\u{2014}");
        assert_eq!(decode(b"a\xffb", UTF8), "a\\x{ff}b");
    }

    #[test]
    fn other_code_pages_keep_ascii_and_escape_the_rest() {
        assert!(!supported(932));
        assert_eq!(decode(b"ab\x82\xa0", 932), "ab\\x{82}\\x{a0}");
    }
}
