//! RLE-Zero decompression and scanline decoding for SHP(TS) sprite frames.
//!
//! RA2's SHP files can compress frame data using "RLE-Zero" encoding,
//! which efficiently handles sprites with large transparent areas.
//! Since most game sprites have transparent backgrounds, this typically
//! achieves 2-5x compression on sprite data.
//!
//! ## Frame format types (set in the per-frame header byte 8):
//! - Format 0/1: raw uncompressed pixel data, width bytes per scanline.
//! - Format 2: length-prefixed uncompressed scanlines.
//! - Format 3: length-prefixed RLE-Zero compressed scanlines.
//!
//! ## Scanline format (formats 2 and 3):
//! 1. u16: total scanline length in bytes **including these 2 bytes**.
//!    Actual data bytes = length - 2.
//! 2. For format 3 (RLE-Zero): each byte in the data region:
//!    - If byte != 0: literal palette index, copy it directly.
//!    - If byte == 0: next byte is a repeat count, write that many zeros.
//! 3. For format 2 (uncompressed): raw palette indices, (length - 2) bytes.
//!
//! ## Dependency rules
//! - Part of assets/ — no dependencies on game modules.

use crate::assets::error::AssetError;

/// Decode RLE-Zero compressed frame data (format 3) from an SHP(TS) file.
///
/// Each scanline is independently compressed with a u16 length prefix
/// (which includes itself, so actual data = length - 2 bytes) followed
/// by RLE-Zero encoded bytes.
///
/// Returns a Vec of `width * height` palette indices.
/// Index 0 means transparent, other values are palette lookups.
pub fn decode_rle_frame(data: &[u8], width: usize, height: usize) -> Result<Vec<u8>, AssetError> {
    let mut pixels: Vec<u8> = Vec::with_capacity(width * height);
    let mut offset: usize = 0;

    for row in 0..height {
        // Each scanline starts with u16: total byte count INCLUDING these 2 bytes.
        if offset + 2 > data.len() {
            return Err(AssetError::ParseError {
                format: "SHP".to_string(),
                detail: format!("RLE data truncated at scanline {} (offset {})", row, offset),
            });
        }

        let raw_length: u16 = u16::from_le_bytes([data[offset], data[offset + 1]]);
        offset += 2;

        // Subtract the 2-byte length field itself to get the actual data size.
        let data_length: usize = (raw_length as usize).saturating_sub(2);
        let line_end: usize = offset + data_length;
        let mut row_pixels: usize = 0;

        while offset < line_end && row_pixels < width {
            let byte: u8 = data[offset];
            offset += 1;

            if byte != 0 {
                // Literal pixel — copy the palette index directly.
                pixels.push(byte);
                row_pixels += 1;
            } else {
                // Zero run — next byte is the count of transparent pixels.
                if offset >= line_end {
                    break;
                }
                let count: u8 = data[offset];
                offset += 1;
                let fill_count: usize = (count as usize).min(width - row_pixels);
                pixels.extend(std::iter::repeat_n(0u8, fill_count));
                row_pixels += fill_count;
            }
        }

        // Pad with transparent (0) if the scanline didn't fill the full width.
        while row_pixels < width {
            pixels.push(0);
            row_pixels += 1;
        }

        // Advance to end of this scanline's data (in case we stopped early).
        offset = line_end;
    }

    Ok(pixels)
}

/// Decode length-prefixed uncompressed frame data (format 2) from an SHP(TS) file.
///
/// Each scanline has a u16 length prefix (includes itself), followed by
/// (length - 2) bytes of raw palette indices. No RLE compression.
pub fn decode_length_prefixed_frame(
    data: &[u8],
    width: usize,
    height: usize,
) -> Result<Vec<u8>, AssetError> {
    let mut pixels: Vec<u8> = Vec::with_capacity(width * height);
    let mut offset: usize = 0;

    for row in 0..height {
        if offset + 2 > data.len() {
            return Err(AssetError::ParseError {
                format: "SHP".to_string(),
                detail: format!(
                    "Length-prefixed data truncated at scanline {} (offset {})",
                    row, offset
                ),
            });
        }

        let raw_length: u16 = u16::from_le_bytes([data[offset], data[offset + 1]]);
        offset += 2;

        let data_length: usize = (raw_length as usize).saturating_sub(2);
        let copy_count: usize = data_length.min(width);

        // Copy available raw bytes (up to frame width).
        let end: usize = (offset + copy_count).min(data.len());
        let available: usize = end - offset;
        pixels.extend_from_slice(&data[offset..end]);

        // Pad if fewer bytes than width.
        for _ in available..width {
            pixels.push(0);
        }

        offset += data_length;
    }

    Ok(pixels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rle_decode_basic() {
        // Scanline: 3 pixels wide, 1 row.
        // u16 length = 6 (includes itself: 2 + 4 data bytes)
        // RLE data: 0x00 0x01 (1 zero), 0x05 (literal), 0x03 (literal)
        let mut data: Vec<u8> = Vec::new();
        data.extend_from_slice(&6u16.to_le_bytes()); // total length including u16
        data.extend_from_slice(&[0x00, 0x01, 0x05, 0x03]);

        let pixels: Vec<u8> = decode_rle_frame(&data, 3, 1).expect("Should decode");
        assert_eq!(pixels, vec![0, 5, 3]);
    }

    #[test]
    fn test_rle_decode_all_transparent() {
        // A fully transparent 4-pixel scanline: one zero-run of 4.
        // u16 length = 4 (includes itself: 2 + 2 data bytes)
        let mut data: Vec<u8> = Vec::new();
        data.extend_from_slice(&4u16.to_le_bytes());
        data.extend_from_slice(&[0x00, 0x04]); // zero-run of 4

        let pixels: Vec<u8> = decode_rle_frame(&data, 4, 1).expect("Should decode");
        assert_eq!(pixels, vec![0, 0, 0, 0]);
    }

    #[test]
    fn test_rle_decode_truncated_error() {
        // Empty data — should error on first scanline's length prefix.
        let data: Vec<u8> = Vec::new();
        assert!(decode_rle_frame(&data, 2, 1).is_err());
    }

    #[test]
    fn test_length_prefixed_decode() {
        // 3-pixel wide scanline, format 2 (uncompressed with length prefix).
        // u16 length = 5 (includes itself: 2 + 3 raw bytes)
        let mut data: Vec<u8> = Vec::new();
        data.extend_from_slice(&5u16.to_le_bytes());
        data.extend_from_slice(&[10, 20, 30]);

        let pixels: Vec<u8> = decode_length_prefixed_frame(&data, 3, 1).expect("Should decode");
        assert_eq!(pixels, vec![10, 20, 30]);
    }
}
