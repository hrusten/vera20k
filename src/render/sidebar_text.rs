//! Sidebar bitmap font renderer — packs glyph bitmaps into a GPU texture atlas
//! and emits `SpriteInstance` quads for each character.
//!
//! Two construction paths:
//!   - `from_fnt()` — real GAME.FNT data (proportional 17px bitmap font)
//!   - `new()` — hardcoded 5×7 fallback for when GAME.FNT is unavailable
//!
//! Also owns a 1×1 "darken" texture (RGBA 0,0,0,175) used for the dark strip
//! overlay behind "Ready" text, matching the original alpha blend at 0xAF.

use std::collections::HashMap;

use crate::assets::fnt_file::FntFile;
use crate::render::batch::{BatchRenderer, BatchTexture, SpriteInstance};
use crate::render::gpu::GpuContext;

// --- Fallback font constants (5×7 hardcoded glyphs) ---
const GLYPH_W: u32 = 5;
const GLYPH_H: u32 = 7;
const GLYPH_PAD: u32 = 1;
const ATLAS_COLUMNS: usize = 8;
/// Fallback inter-char spacing.
const FALLBACK_SPACING: f32 = 1.0;

/// Alpha level for the dark strip overlay (0xAF = 175).
const DARKEN_ALPHA: u8 = 175;

#[derive(Clone, Copy)]
struct GlyphEntry {
    uv_origin: [f32; 2],
    uv_size: [f32; 2],
    /// Actual pixel width of this glyph (variable for FNT, fixed for fallback).
    pixel_width: f32,
}

pub struct SidebarTextRenderer {
    texture: BatchTexture,
    glyphs: HashMap<char, GlyphEntry>,
    /// Inter-character spacing in pixels (1 for GAME.FNT, 1 for fallback).
    char_spacing: f32,
    /// Glyph display height in pixels (16 for GAME.FNT bitmap rows, 7 for fallback).
    glyph_height: f32,
    /// 1×1 RGBA(0,0,0,175) texture for darkening cameo strips.
    darken_texture: Option<BatchTexture>,
}

impl SidebarTextRenderer {
    /// Build from a parsed GAME.FNT file — the authentic path.
    pub fn from_fnt(gpu: &GpuContext, batch: &BatchRenderer, fnt: &FntFile) -> Self {
        // Only pack glyphs we'll actually use (ASCII + Latin-1 supplement).
        // GAME.FNT has ~29,000 glyphs but packing all of them exceeds the
        // GPU texture size limit (8192px). We only need printable ASCII and
        // common Latin characters for sidebar text.
        let mut entries: Vec<(u16, &crate::assets::fnt_file::FntGlyph)> = Vec::new();
        for cp in 0x20u16..0x0180 {
            if let Some(g) = fnt.glyph(cp) {
                entries.push((cp, g));
            }
        }

        if entries.is_empty() {
            log::warn!("FNT has no glyphs, falling back to hardcoded font");
            return Self::new(gpu, batch);
        }

        // Shelf-pack glyphs into an atlas.
        // All glyphs have the same height (bitmap_rows), so one shelf height suffices.
        let row_h = fnt.bitmap_rows;
        let pad = 1u32;
        let max_atlas_w = 512u32;

        // Compute layout: pack glyphs left-to-right, wrap to next row when full.
        struct Placement {
            x: u32,
            y: u32,
        }
        let mut placements: Vec<Placement> = Vec::with_capacity(entries.len());
        let mut cursor_x = 0u32;
        let mut cursor_y = 0u32;
        let mut atlas_w = 0u32;

        for (_cp, g) in &entries {
            let w = g.width + pad * 2;
            if cursor_x + w > max_atlas_w {
                cursor_x = 0;
                cursor_y += row_h + pad * 2;
            }
            placements.push(Placement {
                x: cursor_x + pad,
                y: cursor_y + pad,
            });
            cursor_x += w;
            if cursor_x > atlas_w {
                atlas_w = cursor_x;
            }
        }
        let atlas_h = cursor_y + row_h + pad * 2;

        // Blit glyphs into RGBA atlas.
        let mut rgba = vec![0u8; (atlas_w * atlas_h * 4) as usize];
        let mut glyphs = HashMap::new();

        for (idx, (cp, g)) in entries.iter().enumerate() {
            let pl = &placements[idx];
            // Blit glyph RGBA into atlas.
            for row in 0..row_h {
                for col in 0..g.width {
                    let src = ((row * g.width + col) * 4) as usize;
                    if src + 3 >= g.rgba.len() {
                        continue;
                    }
                    let dst_x = pl.x + col;
                    let dst_y = pl.y + row;
                    let dst = ((dst_y * atlas_w + dst_x) * 4) as usize;
                    rgba[dst..dst + 4].copy_from_slice(&g.rgba[src..src + 4]);
                }
            }

            let ch = char::from_u32(*cp as u32).unwrap_or('?');
            glyphs.insert(
                ch,
                GlyphEntry {
                    uv_origin: [pl.x as f32 / atlas_w as f32, pl.y as f32 / atlas_h as f32],
                    uv_size: [
                        g.width as f32 / atlas_w as f32,
                        row_h as f32 / atlas_h as f32,
                    ],
                    pixel_width: g.width as f32,
                },
            );
        }

        let texture = batch.create_texture(gpu, &rgba, atlas_w, atlas_h);

        // Create 1×1 darken texture.
        let darken_rgba = [0u8, 0, 0, DARKEN_ALPHA];
        let darken_texture = batch.create_texture(gpu, &darken_rgba, 1, 1);

        log::info!(
            "FNT atlas: {}×{} px, {} glyphs",
            atlas_w,
            atlas_h,
            glyphs.len()
        );

        Self {
            texture,
            glyphs,
            char_spacing: 1.0,
            glyph_height: row_h as f32,
            darken_texture: Some(darken_texture),
        }
    }

    /// Fallback constructor using hardcoded 5×7 ASCII glyphs.
    pub fn new(gpu: &GpuContext, batch: &BatchRenderer) -> Self {
        let supported = supported_glyphs();
        let rows = supported.len().div_ceil(ATLAS_COLUMNS);
        let cell_w = GLYPH_W + GLYPH_PAD * 2;
        let cell_h = GLYPH_H + GLYPH_PAD * 2;
        let atlas_w = (ATLAS_COLUMNS as u32) * cell_w;
        let atlas_h = (rows as u32) * cell_h;
        let mut rgba = vec![0u8; (atlas_w * atlas_h * 4) as usize];
        let mut glyphs = HashMap::new();

        for (idx, (ch, bitmap)) in supported.iter().enumerate() {
            let col = (idx % ATLAS_COLUMNS) as u32;
            let row = (idx / ATLAS_COLUMNS) as u32;
            let origin_x = col * cell_w + GLYPH_PAD;
            let origin_y = row * cell_h + GLYPH_PAD;
            write_glyph_bitmap(&mut rgba, atlas_w, origin_x, origin_y, bitmap);
            glyphs.insert(
                *ch,
                GlyphEntry {
                    uv_origin: [
                        origin_x as f32 / atlas_w as f32,
                        origin_y as f32 / atlas_h as f32,
                    ],
                    uv_size: [
                        GLYPH_W as f32 / atlas_w as f32,
                        GLYPH_H as f32 / atlas_h as f32,
                    ],
                    pixel_width: GLYPH_W as f32,
                },
            );
        }

        Self {
            texture: batch.create_texture(gpu, &rgba, atlas_w, atlas_h),
            glyphs,
            char_spacing: FALLBACK_SPACING,
            glyph_height: GLYPH_H as f32,
            darken_texture: None,
        }
    }

    pub fn texture(&self) -> &BatchTexture {
        &self.texture
    }

    /// 1×1 semi-transparent black texture for "Ready" dark strip overlay.
    pub fn darken_texture(&self) -> Option<&BatchTexture> {
        self.darken_texture.as_ref()
    }

    /// Glyph display height in pixels (unscaled).
    pub fn glyph_height(&self) -> f32 {
        self.glyph_height
    }

    /// Measure the pixel width of a text string (unscaled).
    pub fn text_width(&self, text: &str) -> f32 {
        let mut width = 0.0_f32;
        let mut count = 0u32;
        for ch in text.chars() {
            if ch == ' ' {
                width += self.glyph_height * 0.4; // approximate space width
                count += 1;
                continue;
            }
            if let Some(entry) = self.glyphs.get(&ch) {
                width += entry.pixel_width;
                count += 1;
            }
        }
        if count > 1 {
            width += (count - 1) as f32 * self.char_spacing;
        }
        width
    }

    /// Emit sprite instances for a text string.
    pub fn build_text(
        &self,
        text: &str,
        x: f32,
        y: f32,
        scale: f32,
        depth: f32,
        tint: [f32; 3],
        camera_offset: [f32; 2],
    ) -> Vec<SpriteInstance> {
        let mut instances = Vec::new();
        let mut cursor_x = x;
        let spacing = self.char_spacing * scale;
        let h = self.glyph_height * scale;

        for ch in text.chars() {
            if ch == ' ' {
                cursor_x += self.glyph_height * 0.4 * scale + spacing;
                continue;
            }
            let Some(entry) = self.glyphs.get(&ch).copied() else {
                continue;
            };
            let w = entry.pixel_width * scale;
            instances.push(SpriteInstance {
                position: [cursor_x + camera_offset[0], y + camera_offset[1]],
                size: [w, h],
                uv_origin: entry.uv_origin,
                uv_size: entry.uv_size,
                depth,
                tint,
                alpha: 1.0,
            });
            cursor_x += w + spacing;
        }
        instances
    }
}

fn write_glyph_bitmap(
    rgba: &mut [u8],
    atlas_w: u32,
    origin_x: u32,
    origin_y: u32,
    rows: &[&str; 7],
) {
    for (y, row) in rows.iter().enumerate() {
        for (x, pixel) in row.as_bytes().iter().enumerate() {
            if *pixel != b'#' {
                continue;
            }
            let idx = (((origin_y + y as u32) * atlas_w + (origin_x + x as u32)) * 4) as usize;
            rgba[idx..idx + 4].copy_from_slice(&[255, 255, 255, 255]);
        }
    }
}

fn supported_glyphs() -> Vec<(char, [&'static str; 7])> {
    vec![
        (
            ' ',
            [
                ".....", ".....", ".....", ".....", ".....", ".....", ".....",
            ],
        ),
        (
            '-',
            [
                ".....", ".....", ".....", ".###.", ".....", ".....", ".....",
            ],
        ),
        (
            ':',
            [
                ".....", "..#..", ".....", ".....", "..#..", ".....", ".....",
            ],
        ),
        (
            '/',
            [
                "....#", "...#.", "..#..", ".#...", "#....", ".....", ".....",
            ],
        ),
        (
            '0',
            [
                "#####", "#...#", "#...#", "#...#", "#...#", "#...#", "#####",
            ],
        ),
        (
            '1',
            [
                "..#..", ".##..", "..#..", "..#..", "..#..", "..#..", ".###.",
            ],
        ),
        (
            '2',
            [
                "#####", "....#", "....#", "#####", "#....", "#....", "#####",
            ],
        ),
        (
            '3',
            [
                "#####", "....#", "..##.", "....#", "....#", "....#", "#####",
            ],
        ),
        (
            '4',
            [
                "#...#", "#...#", "#...#", "#####", "....#", "....#", "....#",
            ],
        ),
        (
            '5',
            [
                "#####", "#....", "#....", "#####", "....#", "....#", "#####",
            ],
        ),
        (
            '6',
            [
                "#####", "#....", "#....", "#####", "#...#", "#...#", "#####",
            ],
        ),
        (
            '7',
            [
                "#####", "....#", "...#.", "..#..", ".#...", ".#...", ".#...",
            ],
        ),
        (
            '8',
            [
                "#####", "#...#", "#...#", "#####", "#...#", "#...#", "#####",
            ],
        ),
        (
            '9',
            [
                "#####", "#...#", "#...#", "#####", "....#", "....#", "#####",
            ],
        ),
        (
            'A',
            [
                ".###.", "#...#", "#...#", "#####", "#...#", "#...#", "#...#",
            ],
        ),
        (
            'B',
            [
                "####.", "#...#", "#...#", "####.", "#...#", "#...#", "####.",
            ],
        ),
        (
            'C',
            [
                ".####", "#....", "#....", "#....", "#....", "#....", ".####",
            ],
        ),
        (
            'D',
            [
                "####.", "#...#", "#...#", "#...#", "#...#", "#...#", "####.",
            ],
        ),
        (
            'E',
            [
                "#####", "#....", "#....", "####.", "#....", "#....", "#####",
            ],
        ),
        (
            'F',
            [
                "#####", "#....", "#....", "####.", "#....", "#....", "#....",
            ],
        ),
        (
            'G',
            [
                ".####", "#....", "#....", "#.###", "#...#", "#...#", ".###.",
            ],
        ),
        (
            'H',
            [
                "#...#", "#...#", "#...#", "#####", "#...#", "#...#", "#...#",
            ],
        ),
        (
            'I',
            [
                "#####", "..#..", "..#..", "..#..", "..#..", "..#..", "#####",
            ],
        ),
        (
            'J',
            [
                "#####", "...#.", "...#.", "...#.", "...#.", "#..#.", ".##..",
            ],
        ),
        (
            'K',
            [
                "#...#", "#..#.", "#.#..", "##...", "#.#..", "#..#.", "#...#",
            ],
        ),
        (
            'L',
            [
                "#....", "#....", "#....", "#....", "#....", "#....", "#####",
            ],
        ),
        (
            'M',
            [
                "#...#", "##.##", "#.#.#", "#.#.#", "#...#", "#...#", "#...#",
            ],
        ),
        (
            'N',
            [
                "#...#", "##..#", "#.#.#", "#..##", "#...#", "#...#", "#...#",
            ],
        ),
        (
            'O',
            [
                ".###.", "#...#", "#...#", "#...#", "#...#", "#...#", ".###.",
            ],
        ),
        (
            'P',
            [
                "####.", "#...#", "#...#", "####.", "#....", "#....", "#....",
            ],
        ),
        (
            'Q',
            [
                ".###.", "#...#", "#...#", "#...#", "#.#.#", "#..#.", ".##.#",
            ],
        ),
        (
            'R',
            [
                "####.", "#...#", "#...#", "####.", "#.#..", "#..#.", "#...#",
            ],
        ),
        (
            'S',
            [
                ".####", "#....", "#....", ".###.", "....#", "....#", "####.",
            ],
        ),
        (
            'T',
            [
                "#####", "..#..", "..#..", "..#..", "..#..", "..#..", "..#..",
            ],
        ),
        (
            'U',
            [
                "#...#", "#...#", "#...#", "#...#", "#...#", "#...#", ".###.",
            ],
        ),
        (
            'V',
            [
                "#...#", "#...#", "#...#", "#...#", "#...#", ".#.#.", "..#..",
            ],
        ),
        (
            'W',
            [
                "#...#", "#...#", "#...#", "#.#.#", "#.#.#", "##.##", "#...#",
            ],
        ),
        (
            'X',
            [
                "#...#", "#...#", ".#.#.", "..#..", ".#.#.", "#...#", "#...#",
            ],
        ),
        (
            'Y',
            [
                "#...#", "#...#", ".#.#.", "..#..", "..#..", "..#..", "..#..",
            ],
        ),
        (
            'Z',
            [
                "#####", "....#", "...#.", "..#..", ".#...", "#....", "#####",
            ],
        ),
        (
            'a',
            [
                ".....", ".....", ".###.", "....#", ".####", "#...#", ".####",
            ],
        ),
        (
            'b',
            [
                "#....", "#....", "####.", "#...#", "#...#", "#...#", "####.",
            ],
        ),
        (
            'c',
            [
                ".....", ".....", ".####", "#....", "#....", "#....", ".####",
            ],
        ),
        (
            'd',
            [
                "....#", "....#", ".####", "#...#", "#...#", "#...#", ".####",
            ],
        ),
        (
            'e',
            [
                ".....", ".....", ".###.", "#...#", "#####", "#....", ".###.",
            ],
        ),
        (
            'f',
            [
                "..##.", ".#...", ".#...", "####.", ".#...", ".#...", ".#...",
            ],
        ),
        (
            'g',
            [
                ".....", ".####", "#...#", "#...#", ".####", "....#", ".###.",
            ],
        ),
        (
            'h',
            [
                "#....", "#....", "####.", "#...#", "#...#", "#...#", "#...#",
            ],
        ),
        (
            'i',
            [
                "..#..", ".....", "..#..", "..#..", "..#..", "..#..", "..#..",
            ],
        ),
        (
            'j',
            [
                "...#.", ".....", "...#.", "...#.", "...#.", "#..#.", ".##..",
            ],
        ),
        (
            'k',
            [
                "#....", "#....", "#..#.", "#.#..", "##...", "#.#..", "#..#.",
            ],
        ),
        (
            'l',
            [
                ".##..", "..#..", "..#..", "..#..", "..#..", "..#..", ".###.",
            ],
        ),
        (
            'm',
            [
                ".....", ".....", "##.#.", "#.#.#", "#.#.#", "#...#", "#...#",
            ],
        ),
        (
            'n',
            [
                ".....", ".....", "####.", "#...#", "#...#", "#...#", "#...#",
            ],
        ),
        (
            'o',
            [
                ".....", ".....", ".###.", "#...#", "#...#", "#...#", ".###.",
            ],
        ),
        (
            'p',
            [
                ".....", "####.", "#...#", "#...#", "####.", "#....", "#....",
            ],
        ),
        (
            'q',
            [
                ".....", ".####", "#...#", "#...#", ".####", "....#", "....#",
            ],
        ),
        (
            'r',
            [
                ".....", ".....", ".####", "#....", "#....", "#....", "#....",
            ],
        ),
        (
            's',
            [
                ".....", ".....", ".####", "#....", ".###.", "....#", "####.",
            ],
        ),
        (
            't',
            [
                ".#...", ".#...", "####.", ".#...", ".#...", ".#...", "..##.",
            ],
        ),
        (
            'u',
            [
                ".....", ".....", "#...#", "#...#", "#...#", "#...#", ".####",
            ],
        ),
        (
            'v',
            [
                ".....", ".....", "#...#", "#...#", "#...#", ".#.#.", "..#..",
            ],
        ),
        (
            'w',
            [
                ".....", ".....", "#...#", "#...#", "#.#.#", "#.#.#", ".#.#.",
            ],
        ),
        (
            'x',
            [
                ".....", ".....", "#...#", ".#.#.", "..#..", ".#.#.", "#...#",
            ],
        ),
        (
            'y',
            [
                ".....", "#...#", "#...#", ".####", "....#", "...#.", ".##..",
            ],
        ),
        (
            'z',
            [
                ".....", ".....", "#####", "...#.", "..#..", ".#...", "#####",
            ],
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_preserves_case() {
        // normalize_glyph was removed; glyphs are stored as-is.
        assert!(supported_glyphs().iter().any(|(c, _)| *c == 'b'));
        assert!(supported_glyphs().iter().any(|(c, _)| *c == 'B'));
    }
}
