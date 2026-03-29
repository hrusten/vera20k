//! CSF string table parser — localized text for RA2/YR UI, EVA, and menus.
//!
//! CSF files live inside `language.mix` / `langmd.mix` (as `ra2.csf` / `ra2md.csf`).
//! Each entry maps a label name (e.g., `"NAME:MTNK"`) to a Unicode display string
//! (e.g., `"Grizzly Battle Tank"`). The game uses these for all localized text.
//!
//! ## Binary format
//! - 24-byte header: signature ` FSC`, version, label count, string count, language, reserved
//! - Repeating label entries: ` LBL` marker, pair count, ASCII label name, then one or more
//!   string values encoded as bitwise-NOT'd UTF-16-LE.
//!
//! ## Dependency rules
//! - Part of assets/ — no dependencies on game modules.

use std::collections::HashMap;

use crate::assets::error::AssetError;
use crate::util::read_helpers::{read_u16_le, read_u32_le};

/// Magic bytes for the file header: " FSC" (ASCII, little-endian for "CSF ").
const HEADER_MAGIC: u32 = 0x4643_5320;
/// Magic bytes for a label entry: " LBL" (ASCII, little-endian for "LBL ").
const LABEL_MAGIC: u32 = 0x4C42_4C20;
/// Magic bytes for a regular string value: " RTS" (little-endian for "STR ").
const STRING_MAGIC: u32 = 0x5254_5320;
/// Magic bytes for a string value with extra data: "WRTS" (little-endian for "STRW").
const STRING_EXTRA_MAGIC: u32 = 0x5752_5453;
/// Minimum file size: 24-byte header.
const MIN_FILE_SIZE: usize = 24;

/// Parsed CSF string table.
///
/// Keys are stored uppercased for case-insensitive lookup.
/// Values are decoded Unicode strings.
#[derive(Debug, Clone)]
pub struct CsfFile {
    pub version: u32,
    pub language: u16,
    entries: HashMap<String, String>,
}

impl CsfFile {
    /// Parse a CSF file from raw bytes.
    pub fn from_bytes(data: &[u8]) -> Result<Self, AssetError> {
        if data.len() < MIN_FILE_SIZE {
            return Err(AssetError::ParseError {
                format: "CSF".to_string(),
                detail: format!(
                    "file too small: {} bytes (minimum {})",
                    data.len(),
                    MIN_FILE_SIZE
                ),
            });
        }

        let magic: u32 = read_u32_le(data, 0);
        if magic != HEADER_MAGIC {
            return Err(AssetError::ParseError {
                format: "CSF".to_string(),
                detail: format!(
                    "bad header magic: {:#010X} (expected {:#010X})",
                    magic, HEADER_MAGIC
                ),
            });
        }

        let version: u32 = read_u32_le(data, 4);
        let label_count: u32 = read_u32_le(data, 8);
        // Bytes 12..16: string pair count (informational, not needed for parsing).
        let language: u16 = read_u16_le(data, 16);
        // Bytes 18..24: reserved.

        let mut entries: HashMap<String, String> = HashMap::with_capacity(label_count as usize);
        let mut offset: usize = 24;

        for _ in 0..label_count {
            match parse_label_entry(data, offset) {
                Ok((key, value, next_offset)) => {
                    entries.insert(key, value);
                    offset = next_offset;
                }
                Err(_) => {
                    // Malformed entry — stop parsing rather than risk misaligned reads.
                    log::warn!(
                        "CSF: stopped parsing at offset {} ({} of {} labels read)",
                        offset,
                        entries.len(),
                        label_count,
                    );
                    break;
                }
            }
        }

        log::info!(
            "CSF: parsed {} entries (version={}, language={})",
            entries.len(),
            version,
            language,
        );

        Ok(Self {
            version,
            language,
            entries,
        })
    }

    /// Look up a string by label name (case-insensitive).
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .get(&key.to_ascii_uppercase())
            .map(|s| s.as_str())
    }

    /// Number of entries in the string table.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the string table is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Iterate over all (key, value) pairs in the string table.
    pub fn entries(&self) -> impl Iterator<Item = (&str, &str)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }
}

/// Parse one label entry starting at `offset`. Returns (key, value, next_offset).
fn parse_label_entry(data: &[u8], offset: usize) -> Result<(String, String, usize), ()> {
    // Need at least 12 bytes: 4 (LBL magic) + 4 (pair count) + 4 (name length).
    if offset + 12 > data.len() {
        return Err(());
    }

    let magic: u32 = read_u32_le(data, offset);
    if magic != LABEL_MAGIC {
        return Err(());
    }

    let pair_count: u32 = read_u32_le(data, offset + 4);
    let name_len: u32 = read_u32_le(data, offset + 8);
    let name_start: usize = offset + 12;
    let name_end: usize = name_start + name_len as usize;

    if name_end > data.len() {
        return Err(());
    }

    let label_name: String =
        String::from_utf8_lossy(&data[name_start..name_end]).to_ascii_uppercase();

    let mut pos: usize = name_end;
    let mut value: String = String::new();

    // Parse string pairs (usually just 1).
    for _ in 0..pair_count {
        if pos + 8 > data.len() {
            return Err(());
        }

        let str_magic: u32 = read_u32_le(data, pos);
        let has_extra: bool = str_magic == STRING_EXTRA_MAGIC;

        if str_magic != STRING_MAGIC && str_magic != STRING_EXTRA_MAGIC {
            return Err(());
        }

        let char_count: u32 = read_u32_le(data, pos + 4);
        let byte_count: usize = char_count as usize * 2; // UTF-16-LE: 2 bytes per char.
        pos += 8;

        if pos + byte_count > data.len() {
            return Err(());
        }

        // Only keep the first string value (subsequent pairs are rare/unused).
        if value.is_empty() {
            value = decode_csf_string(&data[pos..pos + byte_count]);
        }
        pos += byte_count;

        // Skip the extra value if present (used for audio cue names, not display text).
        if has_extra {
            if pos + 4 > data.len() {
                return Err(());
            }
            let extra_len: u32 = read_u32_le(data, pos);
            pos += 4 + extra_len as usize;
            if pos > data.len() {
                return Err(());
            }
        }
    }

    Ok((label_name, value, pos))
}

/// Decode a CSF-encoded string: bitwise-NOT each byte, then interpret as UTF-16-LE.
fn decode_csf_string(encoded: &[u8]) -> String {
    let decoded_bytes: Vec<u8> = encoded.iter().map(|b| !b).collect();
    let u16_values: Vec<u16> = decoded_bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    String::from_utf16_lossy(&u16_values)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encode a string into CSF format (UTF-16-LE then bitwise-NOT).
    fn encode_csf_string(s: &str) -> Vec<u8> {
        let utf16: Vec<u16> = s.encode_utf16().collect();
        utf16
            .iter()
            .flat_map(|c| c.to_le_bytes())
            .map(|b| !b)
            .collect()
    }

    /// Build a minimal CSF file with one label entry.
    fn build_test_csf(label: &str, value: &str) -> Vec<u8> {
        let encoded_value: Vec<u8> = encode_csf_string(value);
        let char_count: u32 = value.encode_utf16().count() as u32;

        let mut data: Vec<u8> = Vec::new();

        // Header (24 bytes).
        data.extend_from_slice(&HEADER_MAGIC.to_le_bytes()); // " FSC"
        data.extend_from_slice(&3u32.to_le_bytes()); // version
        data.extend_from_slice(&1u32.to_le_bytes()); // label count
        data.extend_from_slice(&1u32.to_le_bytes()); // string pair count
        data.extend_from_slice(&0u16.to_le_bytes()); // language (US English)
        data.extend_from_slice(&[0u8; 6]); // reserved

        // Label entry.
        data.extend_from_slice(&LABEL_MAGIC.to_le_bytes()); // " LBL"
        data.extend_from_slice(&1u32.to_le_bytes()); // 1 string pair
        data.extend_from_slice(&(label.len() as u32).to_le_bytes());
        data.extend_from_slice(label.as_bytes());

        // String value.
        data.extend_from_slice(&STRING_MAGIC.to_le_bytes()); // " RTS"
        data.extend_from_slice(&char_count.to_le_bytes());
        data.extend_from_slice(&encoded_value);

        data
    }

    #[test]
    fn parse_minimal_csf() {
        let data: Vec<u8> = build_test_csf("NAME:MTNK", "Grizzly Battle Tank");
        let csf: CsfFile = CsfFile::from_bytes(&data).expect("should parse");
        assert_eq!(csf.version, 3);
        assert_eq!(csf.language, 0);
        assert_eq!(csf.len(), 1);
        assert_eq!(csf.get("NAME:MTNK"), Some("Grizzly Battle Tank"));
    }

    #[test]
    fn lookup_is_case_insensitive() {
        let data: Vec<u8> = build_test_csf("Name:MTNK", "Grizzly Battle Tank");
        let csf: CsfFile = CsfFile::from_bytes(&data).expect("should parse");
        assert_eq!(csf.get("name:mtnk"), Some("Grizzly Battle Tank"));
        assert_eq!(csf.get("NAME:MTNK"), Some("Grizzly Battle Tank"));
    }

    #[test]
    fn missing_key_returns_none() {
        let data: Vec<u8> = build_test_csf("NAME:MTNK", "Grizzly Battle Tank");
        let csf: CsfFile = CsfFile::from_bytes(&data).expect("should parse");
        assert_eq!(csf.get("NAME:NONEXISTENT"), None);
    }

    #[test]
    fn reject_bad_magic() {
        let mut data: Vec<u8> = build_test_csf("NAME:MTNK", "Tank");
        data[0] = 0x00; // corrupt magic
        assert!(CsfFile::from_bytes(&data).is_err());
    }

    #[test]
    fn reject_truncated_header() {
        let data: Vec<u8> = vec![0x20, 0x46, 0x53, 0x43]; // just the magic, no rest
        assert!(CsfFile::from_bytes(&data).is_err());
    }

    #[test]
    fn decode_roundtrip() {
        let original: &str = "Hello, World! 🌍";
        let encoded: Vec<u8> = encode_csf_string(original);
        let decoded: String = decode_csf_string(&encoded);
        assert_eq!(decoded, original);
    }

    #[test]
    fn wrts_entry_with_extra_value() {
        let encoded_value: Vec<u8> = encode_csf_string("Tank");
        let char_count: u32 = "Tank".encode_utf16().count() as u32;
        let extra: &[u8] = b"some_audio_cue";

        let mut data: Vec<u8> = Vec::new();

        // Header.
        data.extend_from_slice(&HEADER_MAGIC.to_le_bytes());
        data.extend_from_slice(&3u32.to_le_bytes());
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        data.extend_from_slice(&[0u8; 6]);

        // Label.
        data.extend_from_slice(&LABEL_MAGIC.to_le_bytes());
        data.extend_from_slice(&1u32.to_le_bytes());
        let label: &str = "EVA:TankReady";
        data.extend_from_slice(&(label.len() as u32).to_le_bytes());
        data.extend_from_slice(label.as_bytes());

        // WRTS string with extra value.
        data.extend_from_slice(&STRING_EXTRA_MAGIC.to_le_bytes());
        data.extend_from_slice(&char_count.to_le_bytes());
        data.extend_from_slice(&encoded_value);
        data.extend_from_slice(&(extra.len() as u32).to_le_bytes());
        data.extend_from_slice(extra);

        let csf: CsfFile = CsfFile::from_bytes(&data).expect("should parse WRTS");
        assert_eq!(csf.get("EVA:TANKREADY"), Some("Tank"));
    }
}
