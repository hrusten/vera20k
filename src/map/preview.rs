//! Parser for `[Preview]` and `[PreviewPack]` map sections.
//!
//! This first-pass support records whether a map carries preview data and any
//! simple metadata we can read directly from the INI. Full original preview
//! image decoding can build on this later without changing the map-load shape.

use crate::rules::ini_parser::IniFile;

/// Parsed preview-related metadata from a map.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PreviewSection {
    /// Size metadata from `[Preview] Size=` when present.
    pub size: Option<(u32, u32)>,
    /// True if `[PreviewPack]` exists and contains data.
    pub has_packed_preview: bool,
}

/// Parse preview metadata from map INI sections.
pub fn parse_preview_section(ini: &IniFile) -> PreviewSection {
    let size = ini
        .section("Preview")
        .and_then(|section| section.get("Size"))
        .and_then(parse_preview_size);

    let has_packed_preview = ini
        .section("PreviewPack")
        .map(|section| {
            section
                .keys()
                .filter_map(|key| section.get(key))
                .any(|value| !value.trim().is_empty())
        })
        .unwrap_or(false);

    PreviewSection {
        size,
        has_packed_preview,
    }
}

fn parse_preview_size(value: &str) -> Option<(u32, u32)> {
    let parts: Vec<u32> = value
        .split(',')
        .map(str::trim)
        .map(str::parse::<u32>)
        .collect::<Result<Vec<u32>, _>>()
        .ok()?;
    if parts.len() < 2 {
        return None;
    }
    Some((parts[0], parts[1]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::ini_parser::IniFile;

    #[test]
    fn parse_preview_metadata() {
        let ini = IniFile::from_str("[Preview]\nSize=80,50\n[PreviewPack]\n1=ABC\n2=DEF\n");
        let preview = parse_preview_section(&ini);
        assert_eq!(preview.size, Some((80, 50)));
        assert!(preview.has_packed_preview);
    }

    #[test]
    fn empty_preview_pack_is_not_counted() {
        let ini = IniFile::from_str("[PreviewPack]\n1=\n");
        let preview = parse_preview_section(&ini);
        assert_eq!(preview.size, None);
        assert!(!preview.has_packed_preview);
    }
}
