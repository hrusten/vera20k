//! High-level asset loading API for the RA2 engine.
//!
//! The AssetManager opens top-level MIX archives from the RA2 installation
//! directory, then brute-forces nested MIX archives within them to build a
//! search path. Callers ask for filenames and do not need to know where an
//! asset physically lives.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::assets::error::AssetError;
use crate::assets::mix_archive::MixArchive;
use crate::assets::mix_hash::mix_hash;

/// A MIX archive with a human-readable name for logging and diagnostics.
struct NamedArchive {
    /// Name for logs and tools, for example `ra2.mix` or `ra2.mix -> local.mix`.
    name: String,
    /// Parsed archive data.
    archive: MixArchive,
}

/// Manages loaded MIX archives and provides name-based lookups.
///
/// Archives are searched in priority order. Earlier archives win.
pub struct AssetManager {
    /// Loaded MIX archives in search priority order.
    archives: Vec<NamedArchive>,
    /// Path to the RA2 installation directory.
    ra2_dir: PathBuf,
}

/// Minimum entry size to consider as a possible nested MIX archive.
const MIN_NESTED_MIX_SIZE: u32 = 1000;

/// Optional top-level archives used by the runtime.
const OPTIONAL_TOP_LEVEL: &[&str] = &[
    "language.mix",
    "langmd.mix",
    "theme.mix",
    "thememd.mix",
    "multi.mix",
    "multimd.mix",
];

/// Yuri's Revenge expansion archives on disk.
const YR_EXPANSION_MIXES: &[&str] = &["expandmd01.mix"];

/// Common nested archive filenames used to label brute-force discovered MIX entries.
const KNOWN_NESTED_MIX_NAMES: &[&str] = &[
    "audio.mix",
    "audiomd.mix",
    "cache.mix",
    "cachemd.mix",
    "cameo.mix",
    "cameomd.mix",
    "cameos.mix",
    "conquer.mix",
    "conqmd.mix",
    "generic.mix",
    "genermd.mix",
    "isodes.mix",
    "isogen.mix",
    "isogenmd.mix",
    "isolun.mix",
    "isosnow.mix",
    "isosnowmd.mix",
    "isotem.mix",
    "isotemmd.mix",
    "isourb.mix",
    "isourbnmd.mix",
    "langmd.mix",
    "local.mix",
    "localmd.mix",
    "mousemd.mix",
    "sidec01.mix",
    "sidec01md.mix",
    "sidec02.mix",
    "sidec02md.mix",
    "snow.mix",
    "temperat.mix",
    "theme.mix",
    "thememd.mix",
    "urban.mix",
];

impl AssetManager {
    /// Load the core runtime archive stack.
    pub fn new(ra2_dir: &Path) -> Result<Self, AssetError> {
        let mut manager = Self {
            archives: Vec::new(),
            ra2_dir: ra2_dir.to_path_buf(),
        };

        let ra2_mix_path = ra2_dir.join("ra2.mix");
        log::info!("Loading master archive: {}", ra2_mix_path.display());
        let ra2_mix = MixArchive::load(&ra2_mix_path)?;
        log::info!("ra2.mix loaded: {} entries", ra2_mix.entry_count());
        let ra2_nested = Self::extract_all_nested(&ra2_mix, "ra2.mix");
        log::info!(
            "Extracted {} nested archives from ra2.mix",
            ra2_nested.len()
        );

        let md_path = ra2_dir.join("ra2md.mix");
        let md_result = match MixArchive::load(&md_path) {
            Ok(md_mix) => {
                log::info!("ra2md.mix loaded: {} entries", md_mix.entry_count());
                let md_nested = Self::extract_all_nested(&md_mix, "ra2md.mix");
                log::info!(
                    "Extracted {} nested archives from ra2md.mix",
                    md_nested.len()
                );
                Some((md_mix, md_nested))
            }
            Err(_) => {
                log::debug!("ra2md.mix not found (optional, skipping)");
                None
            }
        };

        let expansion_archives = Self::load_expansion_archives(ra2_dir);

        for (name, archive, nested) in expansion_archives {
            if !nested.is_empty() {
                log::info!("  {} nested archives from {}", nested.len(), name);
            }
            manager.archives.extend(nested);
            manager.archives.push(NamedArchive { name, archive });
        }

        if let Some((md_mix, md_nested)) = md_result {
            manager.archives.extend(md_nested);
            manager.archives.push(NamedArchive {
                name: "ra2md.mix".to_string(),
                archive: md_mix,
            });
        }

        manager.archives.extend(ra2_nested);
        manager.archives.push(NamedArchive {
            name: "ra2.mix".to_string(),
            archive: ra2_mix,
        });

        for &top_name in OPTIONAL_TOP_LEVEL {
            let path = ra2_dir.join(top_name);
            match MixArchive::load(&path) {
                Ok(archive) => {
                    log::info!("Loaded {}: {} entries", top_name, archive.entry_count());
                    // Extract nested archives (e.g., AUDIO.MIX inside language.mix).
                    let nested = Self::extract_all_nested(&archive, top_name);
                    if !nested.is_empty() {
                        log::info!("  {} nested archives from {}", nested.len(), top_name);
                    }
                    manager.archives.extend(nested);
                    manager.archives.push(NamedArchive {
                        name: top_name.to_string(),
                        archive,
                    });
                }
                Err(_) => {
                    log::debug!("{} not found (optional, skipping)", top_name);
                }
            }
        }

        log::info!(
            "Archive search order ({} archives, first match wins):",
            manager.archives.len()
        );
        for (i, named) in manager.archives.iter().enumerate() {
            log::info!(
                "  [{:2}] {} ({} entries)",
                i,
                named.name,
                named.archive.entry_count()
            );
        }

        Ok(manager)
    }

    /// Brute-force nested MIX archives from a parent archive.
    fn extract_all_nested(parent: &MixArchive, parent_name: &str) -> Vec<NamedArchive> {
        let mut result = Vec::new();

        for entry in parent.entries() {
            if entry.size < MIN_NESTED_MIX_SIZE {
                continue;
            }

            let Some(data) = parent.get_by_id(entry.id) else {
                continue;
            };

            match MixArchive::from_bytes(data.to_vec()) {
                Ok(nested) if nested.entry_count() > 0 => {
                    let nested_name = guess_nested_mix_name(entry.id)
                        .map(|name| format!("{parent_name} -> {name}"))
                        .unwrap_or_else(|| {
                            format!("{} -> #{:#010X}", parent_name, entry.id as u32)
                        });
                    log::info!(
                        "  Loaded nested id={:#010X} ({} entries, {:.1} MB)",
                        entry.id as u32,
                        nested.entry_count(),
                        entry.size as f64 / (1024.0 * 1024.0),
                    );
                    result.push(NamedArchive {
                        name: nested_name,
                        archive: nested,
                    });
                }
                _ => {
                    // Not a MIX archive or empty.
                }
            }
        }

        result
    }

    /// Load YR expansion archives from disk and extract their nested archives.
    fn load_expansion_archives(ra2_dir: &Path) -> Vec<(String, MixArchive, Vec<NamedArchive>)> {
        let mut result = Vec::new();

        for &name in YR_EXPANSION_MIXES {
            let path = ra2_dir.join(name);
            let archive = match MixArchive::load(&path) {
                Ok(archive) => {
                    log::info!("{} loaded: {} entries", name, archive.entry_count());
                    archive
                }
                Err(_) => {
                    log::debug!("{} not found (optional, skipping)", name);
                    continue;
                }
            };
            let nested = Self::extract_all_nested(&archive, name);
            result.push((name.to_string(), archive, nested));
        }

        result
    }

    /// Look up a file by name across all loaded archives.
    pub fn get(&self, name: &str) -> Option<Vec<u8>> {
        for named in &self.archives {
            if let Some(data) = named.archive.get_by_name(name) {
                log::trace!("Found '{}' in {}", name, named.name);
                return Some(data.to_vec());
            }
        }
        None
    }

    /// Look up a file by name and return both the bytes and source archive name.
    pub fn get_with_source(&self, name: &str) -> Option<(Vec<u8>, String)> {
        for named in &self.archives {
            if let Some(data) = named.archive.get_by_name(name) {
                return Some((data.to_vec(), named.name.clone()));
            }
        }
        None
    }

    /// Load an additional nested archive from within already-loaded archives.
    pub fn load_nested(&mut self, name: &str) -> Result<(), AssetError> {
        let data = self.get(name).ok_or_else(|| AssetError::AssetNotFound {
            name: name.to_string(),
        })?;

        let archive = MixArchive::from_bytes(data)?;
        log::info!(
            "Loaded nested archive: {} ({} entries)",
            name,
            archive.entry_count()
        );

        self.archives.insert(
            0,
            NamedArchive {
                name: format!("nested:{name}"),
                archive,
            },
        );

        Ok(())
    }

    /// Load every additional top-level `.mix` file present in the RA2 directory.
    ///
    /// This is meant for tooling. Runtime code does not call it.
    pub fn load_all_disk_mixes(&mut self) -> Result<usize, AssetError> {
        let mut paths: Vec<PathBuf> = std::fs::read_dir(&self.ra2_dir)?
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| {
                path.is_file()
                    && path
                        .extension()
                        .and_then(OsStr::to_str)
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("mix"))
            })
            .collect();
        paths.sort_by(|a, b| {
            a.file_name()
                .unwrap_or_default()
                .cmp(b.file_name().unwrap_or_default())
        });

        let mut loaded_count = 0usize;
        for path in paths {
            let Some(file_name) = path.file_name().and_then(OsStr::to_str) else {
                continue;
            };
            if self
                .archives
                .iter()
                .any(|archive| archive.name.eq_ignore_ascii_case(file_name))
            {
                continue;
            }

            let archive = match MixArchive::load(&path) {
                Ok(archive) => archive,
                Err(err) => {
                    log::debug!("Skipping {}: {}", path.display(), err);
                    continue;
                }
            };
            log::info!(
                "Loaded extra disk archive {} ({} entries)",
                file_name,
                archive.entry_count()
            );
            let nested = Self::extract_all_nested(&archive, file_name);
            self.archives.extend(nested);
            self.archives.push(NamedArchive {
                name: file_name.to_string(),
                archive,
            });
            loaded_count += 1;
        }

        Ok(loaded_count)
    }

    /// Check if a file exists in any loaded archive.
    pub fn contains(&self, name: &str) -> bool {
        self.archives
            .iter()
            .any(|archive| archive.archive.get_by_name(name).is_some())
    }

    /// Look up a loaded archive by its display/debug name.
    pub fn archive(&self, name: &str) -> Option<&MixArchive> {
        self.archives
            .iter()
            .find(|archive| archive.name.eq_ignore_ascii_case(name))
            .map(|archive| &archive.archive)
    }

    /// Read one entry from a specific archive by entry hash.
    pub fn archive_entry_data(&self, archive_name: &str, entry_id: i32) -> Option<Vec<u8>> {
        let archive = self.archive(archive_name)?;
        archive.get_by_id(entry_id).map(|data| data.to_vec())
    }

    /// Visit all loaded archives in search order.
    pub fn visit_archives<F>(&self, mut visitor: F)
    where
        F: FnMut(&str, &MixArchive),
    {
        for archive in &self.archives {
            visitor(&archive.name, &archive.archive);
        }
    }

    /// Return the names of all loaded archives.
    pub fn loaded_archive_names(&self) -> Vec<String> {
        self.archives
            .iter()
            .map(|archive| archive.name.clone())
            .collect()
    }

    /// Return the RA2 installation directory.
    pub fn ra2_dir(&self) -> &Path {
        &self.ra2_dir
    }
}

fn guess_nested_mix_name(entry_id: i32) -> Option<&'static str> {
    KNOWN_NESTED_MIX_NAMES
        .iter()
        .copied()
        .find(|name| mix_hash(name) == entry_id)
}
