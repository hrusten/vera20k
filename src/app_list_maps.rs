//! Map file discovery and loading utilities.
//!
//! Scans the RA2 directory for available maps and loads them from disk.
//! Extracted from app_init_helpers.rs for file-size limits.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::app_init::MapMenuEntry;
use crate::assets::mix_archive::MixArchive;
use crate::map::briefing::BriefingSection;
use crate::map::map_file::{self, MapFile};
use crate::map::preview::PreviewSection;
use crate::rules::ini_parser::IniFile;
use crate::util::config::GameConfig;

/// List available maps in the RA2 directory for the main-menu map selector.
///
/// Includes `.mmx`, `.map`, and `.mpr` files (case-insensitive), with light
/// metadata extracted from `[Basic]` when available.
pub fn list_available_maps() -> Result<Vec<MapMenuEntry>> {
    let config: GameConfig = GameConfig::load()?;
    let ra2_dir: PathBuf = config.paths.ra2_dir;
    let mut maps: Vec<MapMenuEntry> = Vec::new();
    for entry in std::fs::read_dir(&ra2_dir)? {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        let ext = ext.to_ascii_lowercase();
        if ext == "mmx" || ext == "map" || ext == "mpr" {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                maps.push(read_map_menu_entry(&path, name));
            }
        }
    }
    maps.sort_by_key(|m| m.display_name.to_ascii_lowercase());
    Ok(maps)
}

pub(crate) fn read_map_menu_entry(path: &Path, file_name: &str) -> MapMenuEntry {
    let fallback = || MapMenuEntry {
        file_name: file_name.to_string(),
        display_name: file_name.to_string(),
        author: None,
        briefing: BriefingSection::default(),
        preview: PreviewSection::default(),
    };

    let ini = match read_map_ini_for_metadata(path) {
        Some(ini) => ini,
        None => return fallback(),
    };

    let basic = crate::map::basic::parse_basic_section(&ini);
    let display_name = basic
        .name
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| file_name.to_string());

    MapMenuEntry {
        file_name: file_name.to_string(),
        display_name,
        author: basic.author,
        briefing: crate::map::briefing::parse_briefing_section(&ini),
        preview: crate::map::preview::parse_preview_section(&ini),
    }
}

pub(crate) fn read_map_ini_for_metadata(path: &Path) -> Option<IniFile> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())?;
    if ext == "mmx" {
        let archive = MixArchive::load(path).ok()?;
        let first_id = archive.entries().first()?.id;
        let map_data = archive.get_by_id(first_id)?;
        IniFile::from_bytes(map_data).ok()
    } else {
        let bytes = std::fs::read(path).ok()?;
        IniFile::from_bytes(&bytes).ok()
    }
}

pub(crate) fn load_map_by_name_or_path(ra2_dir: &Path, map_name: &str) -> Result<MapFile> {
    let direct: PathBuf = PathBuf::from(map_name);
    if direct.exists() {
        return load_map_from_path(&direct);
    }

    let in_ra2: PathBuf = ra2_dir.join(map_name);
    if in_ra2.exists() {
        return load_map_from_path(&in_ra2);
    }

    for ext in ["mmx", "map", "mpr"] {
        let candidate = ra2_dir.join(format!("{}.{}", map_name, ext));
        if candidate.exists() {
            return load_map_from_path(&candidate);
        }
    }

    Err(anyhow::anyhow!(
        "Map '{}' not found (checked cwd, RA2 dir, and .mmx/.map/.mpr variants)",
        map_name
    ))
}

pub(crate) fn load_map_from_path(path: &Path) -> Result<MapFile> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    if ext == "mmx" {
        map_file::load_mmx(path).map_err(Into::into)
    } else {
        let bytes: Vec<u8> = std::fs::read(path)
            .map_err(|e| anyhow::anyhow!("Failed to read {}: {}", path.display(), e))?;
        MapFile::from_bytes(&bytes).map_err(Into::into)
    }
}

/// Try loading .mmx map files from a list of candidates.
pub(crate) fn try_load_mmx(ra2_dir: &Path, names: &[&str]) -> Result<MapFile> {
    for &name in names {
        let path: PathBuf = ra2_dir.join(name);
        if path.exists() {
            match map_file::load_mmx(&path) {
                Ok(mf) => {
                    log::info!("Loaded map from {}", name);
                    return Ok(mf);
                }
                Err(err) => {
                    log::warn!("Failed to load {}: {:#}", name, err);
                }
            }
        }
    }
    Err(anyhow::anyhow!(
        "No .mmx map files found in {}",
        ra2_dir.display()
    ))
}
