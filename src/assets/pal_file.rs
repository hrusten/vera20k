//! Parser for RA2 .pal palette files.
//!
//! A .pal file is exactly 768 bytes: 256 entries of 3 bytes each (R, G, B).
//! Color values are in the VGA 6-bit range (0–63), NOT the usual 8-bit (0–255).
//! We scale them to 8-bit on load so the rest of the engine works with standard RGBA.
//!
//! ## House Color Remapping
//! Palette indices 16–31 (16 entries) are reserved for "house colors" — the player's
//! team color. When rendering a unit, these 16 indices get replaced with the owning
//! player's color scheme. This is how RA2 distinguishes player units visually.
//!
//! ## Index 0 = Transparent
//! By convention, palette index 0 is fully transparent. Sprites use index 0
//! for pixels that should show the background behind them.
//!
//! ## Dependency rules
//! - Part of assets/ — no dependencies on game modules.

use std::path::Path;

use crate::assets::error::AssetError;

/// Number of colors in an RA2 palette (always 256 — one per possible byte value).
const PALETTE_COLOR_COUNT: usize = 256;

/// Size of a .pal file in bytes: 256 colors * 3 bytes (R, G, B) each.
const PAL_FILE_SIZE: usize = PALETTE_COLOR_COUNT * 3;

/// First palette index reserved for house (player) colors.
const HOUSE_COLOR_START: usize = 16;

/// Number of palette indices used for house colors (16 through 31 inclusive).
const HOUSE_COLOR_COUNT: usize = 16;

/// A single RGBA color with 8-bit channels.
///
/// Stored as RGBA (not just RGB) because index 0 needs alpha=0 for transparency,
/// and GPU textures expect 4 bytes per pixel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    /// Create a fully opaque color.
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// Create a fully transparent color (used for palette index 0).
    pub const fn transparent() -> Self {
        Self {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        }
    }
}

/// A 256-color palette loaded from a .pal file.
///
/// Colors are already scaled from VGA 6-bit to standard 8-bit.
/// Index 0 is always transparent. Indices 16–31 are house colors
/// that can be remapped per player.
#[derive(Debug, Clone)]
pub struct Palette {
    /// The 256 colors. Index 0 has alpha=0 (transparent).
    pub colors: [Color; PALETTE_COLOR_COUNT],
}

impl Palette {
    /// Parse a palette from raw bytes.
    ///
    /// Input must be exactly 768 bytes (256 colors * 3 bytes RGB).
    /// Each color component is in VGA 6-bit range (0–63) and gets scaled to 8-bit.
    pub fn from_bytes(data: &[u8]) -> Result<Self, AssetError> {
        if data.len() != PAL_FILE_SIZE {
            return Err(AssetError::InvalidPalSize {
                expected: PAL_FILE_SIZE,
                actual: data.len(),
            });
        }

        // Start with all-black, fully opaque colors.
        let mut colors: [Color; PALETTE_COLOR_COUNT] = [Color::rgb(0, 0, 0); PALETTE_COLOR_COUNT];

        for (i, color) in colors.iter_mut().enumerate() {
            let base: usize = i * 3;
            let r: u8 = scale_6bit_vga_to_8bit(data[base]);
            let g: u8 = scale_6bit_vga_to_8bit(data[base + 1]);
            let b: u8 = scale_6bit_vga_to_8bit(data[base + 2]);
            // Index 0 is transparent by convention in all Westwood games.
            // Exact magenta (255, 0, 255) is also a chroma key — RA2 SHP sprites
            // use it as background fill that should be invisible. In isotem.pal this
            // is palette index 10; other palettes may differ.
            let is_transparent: bool = i == 0 || (r == 255 && g == 0 && b == 255);
            *color = Color {
                r,
                g,
                b,
                a: if is_transparent { 0 } else { 255 },
            };
        }

        Ok(Palette { colors })
    }

    /// Load a palette from a .pal file on disk.
    ///
    /// This is a convenience wrapper around from_bytes() for loading loose files.
    /// In production, palettes are extracted from .mix archives and parsed via from_bytes().
    pub fn load(path: &Path) -> Result<Self, AssetError> {
        let data: Vec<u8> = std::fs::read(path)?;
        Self::from_bytes(&data)
    }

    /// Create a copy of this palette with house colors (indices 16–31) replaced.
    ///
    /// Each player has a unique set of 16 colors. When rendering a unit owned by
    /// that player, we swap palette indices 16–31 with the player's house colors.
    /// This is how RA2 makes allied units blue, soviet units red, etc.
    pub fn with_house_colors(&self, house_colors: &[Color; HOUSE_COLOR_COUNT]) -> Palette {
        let mut remapped: Palette = self.clone();
        remapped.colors[HOUSE_COLOR_START..HOUSE_COLOR_START + HOUSE_COLOR_COUNT]
            .copy_from_slice(house_colors);
        remapped
    }

    /// Convert this palette's colors to a flat RGBA byte array (1024 bytes).
    ///
    /// Useful for uploading the palette as a 256x1 GPU texture.
    pub fn to_rgba_bytes(&self) -> Vec<u8> {
        let mut bytes: Vec<u8> = Vec::with_capacity(PALETTE_COLOR_COUNT * 4);
        for color in &self.colors {
            bytes.push(color.r);
            bytes.push(color.g);
            bytes.push(color.b);
            bytes.push(color.a);
        }
        bytes
    }
}

/// Scale a VGA 6-bit color component (0–63) to standard 8-bit (0–255).
///
/// The original VGA hardware used 6 bits per channel (64 levels).
/// We need to expand to 8 bits (256 levels) for modern displays.
/// Formula: output = (input * 255 + 31) / 63
/// This maps 0→0 and 63→255 with correct rounding.
fn scale_6bit_vga_to_8bit(value: u8) -> u8 {
    let v: u16 = value as u16;
    ((v * 255 + 31) / 63) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scale_6bit_boundaries() {
        // 0 must map to 0 (full black stays black).
        assert_eq!(scale_6bit_vga_to_8bit(0), 0);
        // 63 must map to 255 (full white stays white).
        assert_eq!(scale_6bit_vga_to_8bit(63), 255);
    }

    #[test]
    fn test_scale_6bit_midpoint() {
        // Midpoint should be approximately half of 255 (~127).
        let mid: u8 = scale_6bit_vga_to_8bit(31);
        assert!(mid >= 124 && mid <= 126, "Midpoint was {}", mid);
    }

    #[test]
    fn test_parse_palette_basic() {
        // Create a minimal valid palette: all zeros except color 1 = max red.
        let mut data: Vec<u8> = vec![0u8; PAL_FILE_SIZE];
        data[3] = 63; // Color 1: R = 63 (max VGA red)
        data[4] = 0; // Color 1: G = 0
        data[5] = 0; // Color 1: B = 0

        let pal: Palette = Palette::from_bytes(&data).expect("Should parse valid palette");

        // Index 0 should be transparent (alpha = 0).
        assert_eq!(pal.colors[0].a, 0);
        // Index 1 should be fully opaque red.
        assert_eq!(pal.colors[1].r, 255);
        assert_eq!(pal.colors[1].g, 0);
        assert_eq!(pal.colors[1].b, 0);
        assert_eq!(pal.colors[1].a, 255);
    }

    #[test]
    fn test_reject_wrong_size() {
        let data: Vec<u8> = vec![0u8; 100]; // Too small
        let result = Palette::from_bytes(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_to_rgba_bytes_length() {
        let data: Vec<u8> = vec![0u8; PAL_FILE_SIZE];
        let pal: Palette = Palette::from_bytes(&data).expect("Should parse");
        let rgba: Vec<u8> = pal.to_rgba_bytes();
        // 256 colors * 4 bytes (RGBA) each = 1024 bytes
        assert_eq!(rgba.len(), 1024);
    }

    #[test]
    fn test_house_color_remap() {
        let data: Vec<u8> = vec![0u8; PAL_FILE_SIZE];
        let pal: Palette = Palette::from_bytes(&data).expect("Should parse");

        // Create red house colors.
        let red_house: [Color; HOUSE_COLOR_COUNT] = [Color::rgb(255, 0, 0); HOUSE_COLOR_COUNT];
        let remapped: Palette = pal.with_house_colors(&red_house);

        // Indices 16–31 should now be red.
        for i in HOUSE_COLOR_START..(HOUSE_COLOR_START + HOUSE_COLOR_COUNT) {
            assert_eq!(remapped.colors[i].r, 255);
            assert_eq!(remapped.colors[i].g, 0);
            assert_eq!(remapped.colors[i].b, 0);
        }

        // Other indices should be unchanged (still black from the zero input).
        assert_eq!(remapped.colors[0].r, 0);
        assert_eq!(remapped.colors[32].r, 0);
    }
}
