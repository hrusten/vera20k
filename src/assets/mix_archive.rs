//! Reader for RA2 .mix archive files.
//!
//! MIX files are Westwood's archive format — like ZIP but without compression.
//! They store game assets (sprites, palettes, sounds, INI files, and even
//! other nested .mix files).
//!
//! ## Format variants
//!
//! ### New format (Tiberian Sun, Red Alert 2):
//! ```text
//! Offset 0: u16 = 0x0000 (new format marker — always zero)
//! Offset 2: u16 = flags  (bit 0 = checksum, bit 1 = encrypted)
//! Offset 4: if encrypted → 80-byte RSA key block, then encrypted index
//!           if not encrypted → file index header
//! ```
//!
//! ### Old format (Tiberian Dawn, Red Alert 1, theme.mix):
//! ```text
//! Offset 0: u16 = file_count (non-zero — this IS the count, no flags)
//! Offset 2: u32 = body_size
//! Offset 6: file index entries
//! ```
//!
//! ### File index header (6 bytes, inside encrypted block if encrypted):
//! - u16: file_count
//! - u32: body_size
//!
//! ### File index entries (12 bytes each):
//! - i32: id (CRC-32 hash of filename)
//! - u32: offset (from start of body section)
//! - u32: size
//!
//! ## Encryption
//! RA2 MIX files typically have flags = 0x0003 (encrypted + checksum).
//! Only the file index is encrypted — the body data is plaintext.
//! See mix_crypto.rs for the RSA + Blowfish decryption details.
//!
//! ## Dependency rules
//! - Part of assets/ — no dependencies on game modules.
//! - Uses mix_crypto for decryption and mix_hash for filename lookup.

use std::path::Path;

use crate::assets::error::AssetError;
use crate::assets::mix_crypto;
use crate::assets::mix_hash::{mix_hash, westwood_hash};
use crate::util::read_helpers::{read_i32_le, read_u16_le, read_u32_le};

/// MIX flags bit: file index is Blowfish-encrypted (with RSA key block).
const FLAG_ENCRYPTED: u16 = 0x0002;

/// Size of the new-format header: 2-byte marker (0x0000) + 2-byte flags.
/// The RSA key block or file index starts after this.
const NEW_FORMAT_HEADER_SIZE: usize = 4;

/// Size of each file index entry in bytes: id(4) + offset(4) + size(4).
const INDEX_ENTRY_SIZE: usize = 12;

/// Size of the file index sub-header in bytes: file_count(2) + body_size(4).
const INDEX_HEADER_SIZE: usize = 6;

/// A single entry in the MIX file index.
///
/// Each entry maps a hashed filename (i32 id) to a byte range
/// within the MIX body section. The id is computed from the
/// original filename using CRC-32 (see mix_hash.rs).
#[derive(Debug, Clone)]
pub struct MixEntry {
    /// CRC-32 hash of the original filename (cast to i32).
    pub id: i32,
    /// Byte offset of this file's data within the body section.
    pub offset: u32,
    /// Size of this file's data in bytes.
    pub size: u32,
}

/// A parsed MIX archive, ready for file extraction.
///
/// Holds the entire MIX file data in memory and provides
/// methods to look up and extract files by name or hash ID.
///
/// ## Why keep everything in memory?
/// RA2's MIX files are typically 100-300 MB total. Modern systems
/// have plenty of RAM, and keeping the data in memory avoids repeated
/// disk I/O when extracting multiple files. Memory-mapping could be
/// added as an optimization later if needed.
pub struct MixArchive {
    /// The file index entries, sorted by id for binary search lookup.
    entries: Vec<MixEntry>,
    /// Filename hash algorithm used by this archive format.
    hash_kind: MixHashKind,
    /// Byte offset where the body section starts in the raw data.
    body_offset: usize,
    /// The raw file data (entire MIX file kept in memory).
    data: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MixHashKind {
    Crc32,
    Westwood,
}

impl MixArchive {
    /// Parse a MIX archive from raw bytes.
    ///
    /// Detects old vs new format by checking the first u16:
    /// - 0x0000 → new format (RA2/TS): real flags at offset 2
    /// - Non-zero → old format (TD/RA1): that value IS the file_count
    pub fn from_bytes(data: Vec<u8>) -> Result<Self, AssetError> {
        if data.len() < 4 {
            return Err(AssetError::InvalidMixHeader {
                reason: format!("File too small: {} bytes (need at least 4)", data.len()),
            });
        }

        // The first u16 distinguishes old vs new MIX format.
        // New format (RA2/TS): offset 0 is always 0x0000 as a format marker.
        // Old format (TD/RA1): offset 0 is file_count (always non-zero).
        let first_word: u16 = read_u16_le(&data, 0);

        if first_word == 0 {
            // New format: actual flags are at offset 2.
            let flags: u16 = read_u16_le(&data, 2);
            let is_encrypted: bool = (flags & FLAG_ENCRYPTED) != 0;

            if is_encrypted {
                Self::parse_encrypted_new_format(data)
            } else {
                Self::parse_unencrypted_new_format(data)
            }
        } else {
            // Old format: first_word IS the file_count. No flags, no encryption.
            Self::parse_old_format(data, first_word)
        }
    }

    /// Load a MIX archive from a file path.
    pub fn load(path: &Path) -> Result<Self, AssetError> {
        let data: Vec<u8> = std::fs::read(path)?;
        Self::from_bytes(data)
    }

    /// Look up a file by name. Returns the raw bytes if found.
    ///
    /// MIX archives in the wild are not perfectly format-segregated by hash style:
    /// some old-format archives (notably `theme.mix` / `thememd.mix`) still use the
    /// CRC-style filename hash. Try the archive's primary hash first, then fall back
    /// to the alternate hash before giving up.
    pub fn get_by_name(&self, name: &str) -> Option<&[u8]> {
        let primary: i32 = match self.hash_kind {
            MixHashKind::Crc32 => mix_hash(name),
            MixHashKind::Westwood => westwood_hash(name),
        };
        if let Some(data) = self.get_by_id(primary) {
            return Some(data);
        }

        let alternate: i32 = match self.hash_kind {
            MixHashKind::Crc32 => westwood_hash(name),
            MixHashKind::Westwood => mix_hash(name),
        };
        self.get_by_id(alternate)
    }

    /// Look up a file by its pre-computed hash ID.
    ///
    /// Uses binary search on the sorted index for O(log n) lookup.
    pub fn get_by_id(&self, id: i32) -> Option<&[u8]> {
        let index: usize = self.entries.binary_search_by_key(&id, |e| e.id).ok()?;
        let entry: &MixEntry = &self.entries[index];

        let start: usize = self.body_offset + entry.offset as usize;
        let end: usize = start + entry.size as usize;

        // Bounds check to prevent panics on corrupt data.
        if end > self.data.len() {
            return None;
        }

        Some(&self.data[start..end])
    }

    /// Number of files in this archive.
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Get all entries (for debugging and enumeration).
    pub fn entries(&self) -> &[MixEntry] {
        &self.entries
    }

    /// Size of the raw archive in bytes.
    pub fn byte_len(&self) -> usize {
        self.data.len()
    }

    /// Parse a new-format encrypted MIX file (RA2 typical: flags & 0x0002).
    ///
    /// Layout: [marker:2][flags:2][RSA block:80][encrypted index...][body...]
    /// The RSA block starts at offset 4 (after the 2-byte marker + 2-byte flags).
    fn parse_encrypted_new_format(data: Vec<u8>) -> Result<Self, AssetError> {
        // RSA key block starts after the 4-byte new-format header.
        let rsa_start: usize = NEW_FORMAT_HEADER_SIZE;
        let rsa_end: usize = rsa_start + mix_crypto::RSA_KEY_BLOCK_SIZE;

        if data.len() < rsa_end {
            return Err(AssetError::InvalidMixHeader {
                reason: format!(
                    "File too small for RSA key block: {} bytes (need at least {})",
                    data.len(),
                    rsa_end
                ),
            });
        }

        // Extract the 56-byte Blowfish key from the RSA-encrypted block.
        let blowfish_key: [u8; 56] = mix_crypto::extract_blowfish_key(&data[rsa_start..rsa_end])?;

        // The encrypted index starts right after the RSA block.
        let encrypted_start: usize = rsa_end;

        // Decrypt the first 8-byte block to peek at file_count and body_size.
        if data.len() < encrypted_start + mix_crypto::BLOWFISH_BLOCK_SIZE {
            return Err(AssetError::InvalidMixHeader {
                reason: "File too small for encrypted index header".to_string(),
            });
        }

        let mut first_block: [u8; 8] = [0u8; 8];
        first_block.copy_from_slice(
            &data[encrypted_start..encrypted_start + mix_crypto::BLOWFISH_BLOCK_SIZE],
        );
        mix_crypto::blowfish_decrypt_ecb(&blowfish_key, &mut first_block)?;

        let file_count: u16 = read_u16_le(&first_block, 0);
        let _body_size: u32 = read_u32_le(&first_block, 2);

        // Calculate total encrypted index size and decrypt everything.
        let total_index_bytes: usize = INDEX_HEADER_SIZE + file_count as usize * INDEX_ENTRY_SIZE;
        // Round up to Blowfish block boundary (8 bytes).
        let encrypted_size: usize = total_index_bytes.div_ceil(mix_crypto::BLOWFISH_BLOCK_SIZE)
            * mix_crypto::BLOWFISH_BLOCK_SIZE;

        if data.len() < encrypted_start + encrypted_size {
            return Err(AssetError::InvalidMixHeader {
                reason: format!(
                    "File too small for encrypted index: {} bytes (need {})",
                    data.len(),
                    encrypted_start + encrypted_size
                ),
            });
        }

        // Copy and decrypt the entire index.
        let mut decrypted_index: Vec<u8> =
            data[encrypted_start..encrypted_start + encrypted_size].to_vec();
        mix_crypto::blowfish_decrypt_ecb(&blowfish_key, &mut decrypted_index)?;

        // Parse entries from the decrypted index buffer.
        let entries: Vec<MixEntry> =
            Self::parse_entries(&decrypted_index, INDEX_HEADER_SIZE, file_count)?;

        // Body data starts right after the encrypted index.
        let body_offset: usize = encrypted_start + encrypted_size;

        let mut archive: Self = Self {
            entries,
            hash_kind: MixHashKind::Crc32,
            body_offset,
            data,
        };
        archive.entries.sort_by_key(|e| e.id);

        Ok(archive)
    }

    /// Parse a new-format unencrypted MIX file (flags has no encryption bit).
    ///
    /// Layout: [marker:2][flags:2][file_count:2][body_size:4][entries...][body...]
    /// The index header starts at offset 4.
    fn parse_unencrypted_new_format(data: Vec<u8>) -> Result<Self, AssetError> {
        if data.len() < NEW_FORMAT_HEADER_SIZE + INDEX_HEADER_SIZE {
            return Err(AssetError::InvalidMixHeader {
                reason: "File too small for new-format unencrypted header".to_string(),
            });
        }

        let file_count: u16 = read_u16_le(&data, NEW_FORMAT_HEADER_SIZE);
        let _body_size: u32 = read_u32_le(&data, NEW_FORMAT_HEADER_SIZE + 2);

        let index_start: usize = NEW_FORMAT_HEADER_SIZE + INDEX_HEADER_SIZE;
        let index_end: usize = index_start + file_count as usize * INDEX_ENTRY_SIZE;

        if data.len() < index_end {
            return Err(AssetError::InvalidMixHeader {
                reason: format!(
                    "File too small for {} index entries: need {} bytes",
                    file_count, index_end
                ),
            });
        }

        let entries: Vec<MixEntry> = Self::parse_entries(&data, index_start, file_count)?;
        let body_offset: usize = index_end;

        let mut archive: Self = Self {
            entries,
            hash_kind: MixHashKind::Crc32,
            body_offset,
            data,
        };
        archive.entries.sort_by_key(|e| e.id);

        Ok(archive)
    }

    /// Parse an old-format MIX file (TD/RA1/theme.mix — no format marker).
    ///
    /// Layout: [file_count:2][body_size:4][entries...][body...]
    /// The first u16 at offset 0 is already the file_count (passed in).
    fn parse_old_format(data: Vec<u8>, file_count: u16) -> Result<Self, AssetError> {
        if data.len() < INDEX_HEADER_SIZE {
            return Err(AssetError::InvalidMixHeader {
                reason: "File too small for old-format header".to_string(),
            });
        }

        // body_size at offset 2 (skip file_count which is the first 2 bytes).
        let _body_size: u32 = read_u32_le(&data, 2);

        let index_start: usize = INDEX_HEADER_SIZE;
        let index_end: usize = index_start + file_count as usize * INDEX_ENTRY_SIZE;

        if data.len() < index_end {
            return Err(AssetError::InvalidMixHeader {
                reason: format!(
                    "File too small for {} old-format index entries: need {} bytes",
                    file_count, index_end
                ),
            });
        }

        let entries: Vec<MixEntry> = Self::parse_entries(&data, index_start, file_count)?;
        let body_offset: usize = index_end;

        let mut archive: Self = Self {
            entries,
            hash_kind: MixHashKind::Westwood,
            body_offset,
            data,
        };
        archive.entries.sort_by_key(|e| e.id);

        Ok(archive)
    }

    /// Parse file index entries from a buffer starting at the given offset.
    fn parse_entries(
        data: &[u8],
        start: usize,
        file_count: u16,
    ) -> Result<Vec<MixEntry>, AssetError> {
        let mut entries: Vec<MixEntry> = Vec::with_capacity(file_count as usize);

        for i in 0..file_count as usize {
            let offset: usize = start + i * INDEX_ENTRY_SIZE;

            if offset + INDEX_ENTRY_SIZE > data.len() {
                return Err(AssetError::InvalidMixHeader {
                    reason: format!(
                        "Index entry {} extends past data ({} > {})",
                        i,
                        offset + INDEX_ENTRY_SIZE,
                        data.len()
                    ),
                });
            }

            entries.push(MixEntry {
                id: read_i32_le(data, offset),
                offset: read_u32_le(data, offset + 4),
                size: read_u32_le(data, offset + 8),
            });
        }

        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal new-format unencrypted MIX file with one entry.
    ///
    /// Layout: [marker:0x0000][flags:0x0000][file_count:1][body_size:5]
    ///         [index entry: 12 bytes][body: "hello"]
    fn make_test_mix_new_format() -> Vec<u8> {
        let mut data: Vec<u8> = Vec::new();

        // New-format header: marker=0x0000, flags=0x0000 (no encryption)
        data.extend_from_slice(&0u16.to_le_bytes()); // marker
        data.extend_from_slice(&0u16.to_le_bytes()); // flags

        // Index header: file_count=1, body_size=5
        data.extend_from_slice(&1u16.to_le_bytes()); // file_count
        data.extend_from_slice(&5u32.to_le_bytes()); // body_size

        // Index entry: id=hash("test.txt"), offset=0, size=5
        let test_id: i32 = mix_hash("test.txt");
        data.extend_from_slice(&test_id.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes()); // offset
        data.extend_from_slice(&5u32.to_le_bytes()); // size

        // Body data
        data.extend_from_slice(b"hello");

        data
    }

    /// Build a minimal old-format MIX file (like theme.mix).
    ///
    /// Layout: [file_count:1][body_size:5][index entry: 12 bytes][body: "world"]
    fn make_test_mix_old_format() -> Vec<u8> {
        let mut data: Vec<u8> = Vec::new();

        // Old format: file_count directly at offset 0 (non-zero).
        data.extend_from_slice(&1u16.to_le_bytes()); // file_count=1
        data.extend_from_slice(&5u32.to_le_bytes()); // body_size=5

        // Index entry
        let test_id: i32 = westwood_hash("data.bin");
        data.extend_from_slice(&test_id.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes()); // offset
        data.extend_from_slice(&5u32.to_le_bytes()); // size

        // Body data
        data.extend_from_slice(b"world");

        data
    }

    #[test]
    fn test_parse_new_format_unencrypted() {
        let data: Vec<u8> = make_test_mix_new_format();
        let archive: MixArchive =
            MixArchive::from_bytes(data).expect("Should parse new-format MIX");
        assert_eq!(archive.entry_count(), 1);
    }

    #[test]
    fn test_parse_old_format() {
        let data: Vec<u8> = make_test_mix_old_format();
        let archive: MixArchive =
            MixArchive::from_bytes(data).expect("Should parse old-format MIX");
        assert_eq!(archive.entry_count(), 1);
    }

    #[test]
    fn test_get_by_name_new_format() {
        let data: Vec<u8> = make_test_mix_new_format();
        let archive: MixArchive = MixArchive::from_bytes(data).expect("Should parse");

        let content: &[u8] = archive
            .get_by_name("test.txt")
            .expect("Should find test.txt");
        assert_eq!(content, b"hello");
    }

    #[test]
    fn test_get_by_name_old_format() {
        let data: Vec<u8> = make_test_mix_old_format();
        let archive: MixArchive = MixArchive::from_bytes(data).expect("Should parse");

        let content: &[u8] = archive
            .get_by_name("data.bin")
            .expect("Should find data.bin");
        assert_eq!(content, b"world");
    }

    #[test]
    fn test_get_by_name_case_insensitive() {
        let data: Vec<u8> = make_test_mix_new_format();
        let archive: MixArchive = MixArchive::from_bytes(data).expect("Should parse");

        assert!(archive.get_by_name("TEST.TXT").is_some());
        assert!(archive.get_by_name("Test.Txt").is_some());
    }

    #[test]
    fn test_get_nonexistent_file() {
        let data: Vec<u8> = make_test_mix_new_format();
        let archive: MixArchive = MixArchive::from_bytes(data).expect("Should parse");

        assert!(archive.get_by_name("nonexistent.dat").is_none());
    }

    #[test]
    fn test_reject_too_small() {
        let data: Vec<u8> = vec![0; 2]; // Way too small
        assert!(MixArchive::from_bytes(data).is_err());
    }
}
