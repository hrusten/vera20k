//! sound.ini / soundmd.ini parser — maps sound IDs to .wav/.aud filenames.
//!
//! RA2's sound.ini has sections like:
//! ```ini
//! [VGCannon1]
//! Sounds=vgcannon.wav
//! Volume=100
//! Priority=1
//! Control=random,all
//! ```
//!
//! Each section name is a sound ID referenced by weapons (Report=), units
//! (VoiceSelect=, VoiceMove=, DieSound=), and EVA announcements.
//! The `Sounds=` key lists one or more sound names separated by whitespace
//! or commas. Names may have `$` or `#` prefixes (legacy Westwood markers,
//! stripped at load time). When multiple sounds are listed, the game picks
//! one at random.
//!
//! ## Dependency rules
//! - Part of rules/ — depends on rules/ini_parser. No sim/render dependencies.

use std::collections::HashMap;

use crate::rules::ini_parser::IniFile;

/// A single sound definition from sound.ini.
#[derive(Debug, Clone)]
pub struct SoundEntry {
    /// Section name / sound ID (e.g., "VGCannon1").
    pub id: String,
    /// List of .wav filenames to choose from (e.g., ["vgcannon.wav"]).
    pub sounds: Vec<String>,
    /// Playback volume (0-100, default from [Defaults] or 100).
    pub volume: u8,
    /// Priority (higher = more important, won't be evicted as easily).
    pub priority: u8,
    /// Audible range in cells (default 10). Used for spatial audio:
    /// max audible pixel distance = range * 60 (original engine multiplier).
    pub range: u16,
    /// Minimum volume floor (0-100, default 0). For GLOBAL-type sounds,
    /// volume never drops below this even at maximum distance.
    pub min_volume: u8,
}

/// Registry of all sound definitions, keyed by uppercase sound ID.
#[derive(Debug, Clone, Default)]
pub struct SoundRegistry {
    entries: HashMap<String, SoundEntry>,
}

impl SoundRegistry {
    /// Parse a SoundRegistry from sound.ini / soundmd.ini data.
    ///
    /// Loads `soundmd.ini` first (YR), then `sound.ini` (base RA2) as fallback.
    /// Sections from soundmd override sound.ini on conflict.
    pub fn from_ini(ini: &IniFile) -> Self {
        let mut entries: HashMap<String, SoundEntry> = HashMap::new();

        // Read [Defaults] section for fallback values (original engine behavior).
        let default_volume: u8 = ini
            .section("Defaults")
            .and_then(|s| s.get_i32("Volume"))
            .unwrap_or(100)
            .clamp(0, 100) as u8;
        let default_range: u16 = ini
            .section("Defaults")
            .and_then(|s| s.get_i32("Range"))
            .unwrap_or(10)
            .clamp(1, 1000) as u16;
        let default_min_volume: u8 = ini
            .section("Defaults")
            .and_then(|s| s.get_i32("MinVolume"))
            .unwrap_or(0)
            .clamp(0, 100) as u8;

        for name in ini.section_names() {
            let Some(section) = ini.section(name) else {
                continue;
            };
            // Skip meta-sections that aren't actual sound definitions.
            if name.eq_ignore_ascii_case("General")
                || name.eq_ignore_ascii_case("Sounds")
                || name.eq_ignore_ascii_case("Defaults")
            {
                continue;
            }

            let sounds_str: &str = match section.get("Sounds") {
                Some(s) => s,
                None => continue,
            };

            // Sounds= can be whitespace-separated (soundmd.ini) or comma-separated
            // (sound.ini). Names may have $ or # prefixes (legacy, stripped).
            // Inline comments starting with ; are filtered out.
            let sounds: Vec<String> = sounds_str
                .split_whitespace()
                .flat_map(|token| token.split(','))
                .map(|s| s.trim())
                .filter(|s| !s.is_empty() && !s.starts_with(';'))
                .map(|s| {
                    s.trim_start_matches('$')
                        .trim_start_matches('#')
                        .to_string()
                })
                .filter(|s| !s.is_empty())
                .collect();

            if sounds.is_empty() {
                continue;
            }

            let volume: u8 = section
                .get_i32("Volume")
                .unwrap_or(default_volume as i32)
                .clamp(0, 100) as u8;
            let priority: u8 = section.get_i32("Priority").unwrap_or(1).clamp(0, 255) as u8;
            let range: u16 = section
                .get_i32("Range")
                .unwrap_or(default_range as i32)
                .clamp(1, 1000) as u16;
            let min_volume: u8 = section
                .get_i32("MinVolume")
                .unwrap_or(default_min_volume as i32)
                .clamp(0, 100) as u8;

            entries.insert(
                name.to_ascii_uppercase(),
                SoundEntry {
                    id: name.to_string(),
                    sounds,
                    volume,
                    priority,
                    range,
                    min_volume,
                },
            );
        }

        log::info!("SoundRegistry: loaded {} sound definitions", entries.len());
        Self { entries }
    }

    /// Merge another sound.ini (base RA2) into this registry.
    /// Only adds entries that don't already exist (YR-first precedence).
    pub fn merge_fallback(&mut self, ini: &IniFile) {
        let fallback: SoundRegistry = SoundRegistry::from_ini(ini);
        let mut added: usize = 0;
        for (key, entry) in fallback.entries {
            if !self.entries.contains_key(&key) {
                self.entries.insert(key, entry);
                added += 1;
            }
        }
        if added > 0 {
            log::info!(
                "SoundRegistry: merged {} fallback entries (total {})",
                added,
                self.entries.len()
            );
        }
    }

    /// Look up a sound entry by ID (case-insensitive).
    pub fn get(&self, sound_id: &str) -> Option<&SoundEntry> {
        self.entries.get(&sound_id.to_ascii_uppercase())
    }

    /// Total number of registered sound definitions.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Per-faction sound IDs for a single EVA event (e.g., EVA_ConstructionComplete).
#[derive(Debug, Clone, Default)]
struct EvaEntry {
    allied: Option<String>,
    russian: Option<String>,
    yuri: Option<String>,
}

/// Registry of EVA announcements from eva.ini / evamd.ini.
///
/// Maps EVA event names (e.g., "EVA_ConstructionComplete") to per-faction
/// audio.bag sound IDs (e.g., Allied="ceva048", Russian="csof048", Yuri="cyur048").
#[derive(Debug, Clone, Default)]
pub struct EvaRegistry {
    entries: HashMap<String, EvaEntry>,
}

impl EvaRegistry {
    /// Parse an EvaRegistry from eva.ini / evamd.ini data.
    pub fn from_ini(ini: &IniFile) -> Self {
        let mut entries: HashMap<String, EvaEntry> = HashMap::new();

        for name in ini.section_names() {
            // Only parse EVA_ sections (skip DialogList, etc.).
            if !name.starts_with("EVA_") {
                continue;
            }
            let Some(section) = ini.section(name) else {
                continue;
            };

            let entry = EvaEntry {
                allied: section.get("Allied").map(|s| s.to_string()),
                russian: section.get("Russian").map(|s| s.to_string()),
                yuri: section.get("Yuri").map(|s| s.to_string()),
            };

            // Only store if at least one faction has a sound.
            if entry.allied.is_some() || entry.russian.is_some() || entry.yuri.is_some() {
                entries.insert(name.to_ascii_uppercase(), entry);
            }
        }

        log::info!(
            "EvaRegistry: loaded {} EVA event definitions",
            entries.len()
        );
        Self { entries }
    }

    /// Merge another eva.ini (base RA2) into this registry.
    /// Only adds entries that don't already exist (YR-first precedence).
    pub fn merge_fallback(&mut self, ini: &IniFile) {
        let fallback = EvaRegistry::from_ini(ini);
        let mut added: usize = 0;
        for (key, entry) in fallback.entries {
            if !self.entries.contains_key(&key) {
                self.entries.insert(key, entry);
                added += 1;
            }
        }
        if added > 0 {
            log::info!(
                "EvaRegistry: merged {} fallback entries (total {})",
                added,
                self.entries.len()
            );
        }
    }

    /// Look up an EVA sound ID by event name and faction key.
    ///
    /// `event_name` is e.g., "EVA_ConstructionComplete" (case-insensitive).
    /// `faction_key` is one of "Allied", "Russian", or "Yuri".
    pub fn get(&self, event_name: &str, faction_key: &str) -> Option<&str> {
        let entry = self.entries.get(&event_name.to_ascii_uppercase())?;
        let sound = match faction_key {
            "Russian" => entry.russian.as_deref(),
            "Yuri" => entry.yuri.as_deref(),
            _ => entry.allied.as_deref(),
        };
        sound
    }

    /// Number of EVA event definitions.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_single_sound() {
        let ini: IniFile =
            IniFile::from_str("[VGCannon1]\nSounds=vgcannon.wav\nVolume=80\nPriority=5\n");
        let reg: SoundRegistry = SoundRegistry::from_ini(&ini);
        assert_eq!(reg.len(), 1);
        let entry: &SoundEntry = reg.get("VGCannon1").expect("should find entry");
        assert_eq!(entry.sounds, vec!["vgcannon.wav"]);
        assert_eq!(entry.volume, 80);
        assert_eq!(entry.priority, 5);
    }

    #[test]
    fn test_parse_multi_sound() {
        let ini: IniFile = IniFile::from_str(
            "[E1Voice]\nSounds=e1sel01.wav,e1sel02.wav,e1sel03.wav\nVolume=100\n",
        );
        let reg: SoundRegistry = SoundRegistry::from_ini(&ini);
        let entry: &SoundEntry = reg.get("E1Voice").expect("should find entry");
        assert_eq!(entry.sounds.len(), 3);
        assert_eq!(entry.sounds[0], "e1sel01.wav");
    }

    #[test]
    fn test_case_insensitive_lookup() {
        let ini: IniFile = IniFile::from_str("[TestSound]\nSounds=test.wav\n");
        let reg: SoundRegistry = SoundRegistry::from_ini(&ini);
        assert!(reg.get("testsound").is_some());
        assert!(reg.get("TESTSOUND").is_some());
    }

    #[test]
    fn test_skip_general_section() {
        let ini: IniFile =
            IniFile::from_str("[General]\nSounds=nothing.wav\n[Real]\nSounds=real.wav\n");
        let reg: SoundRegistry = SoundRegistry::from_ini(&ini);
        assert!(reg.get("General").is_none());
        assert!(reg.get("Real").is_some());
    }

    #[test]
    fn test_merge_fallback() {
        let ini1: IniFile = IniFile::from_str("[SoundA]\nSounds=a.wav\n");
        let ini2: IniFile =
            IniFile::from_str("[SoundA]\nSounds=a_old.wav\n[SoundB]\nSounds=b.wav\n");
        let mut reg: SoundRegistry = SoundRegistry::from_ini(&ini1);
        reg.merge_fallback(&ini2);
        // SoundA should keep ini1 version (YR precedence)
        assert_eq!(reg.get("SoundA").unwrap().sounds[0], "a.wav");
        // SoundB added from fallback
        assert!(reg.get("SoundB").is_some());
    }

    #[test]
    fn test_defaults() {
        let ini: IniFile = IniFile::from_str("[MinimalSound]\nSounds=min.wav\n");
        let reg: SoundRegistry = SoundRegistry::from_ini(&ini);
        let entry: &SoundEntry = reg.get("MinimalSound").unwrap();
        assert_eq!(entry.volume, 100);
        assert_eq!(entry.priority, 1);
    }

    #[test]
    fn test_whitespace_separated() {
        let ini: IniFile =
            IniFile::from_str("[GISelect]\nSounds= igisea igiseb igisec\nVolume=85\n");
        let reg: SoundRegistry = SoundRegistry::from_ini(&ini);
        let entry: &SoundEntry = reg.get("GISelect").expect("should find entry");
        assert_eq!(entry.sounds, vec!["igisea", "igiseb", "igisec"]);
        assert_eq!(entry.volume, 85);
    }

    #[test]
    fn test_strip_dollar_prefix() {
        let ini: IniFile =
            IniFile::from_str("[VoiceTest]\nSounds= $igisea $igiseb $igisec\nVolume=85\n");
        let reg: SoundRegistry = SoundRegistry::from_ini(&ini);
        let entry: &SoundEntry = reg.get("VoiceTest").expect("should find entry");
        assert_eq!(entry.sounds, vec!["igisea", "igiseb", "igisec"]);
    }

    #[test]
    fn test_strip_hash_prefix() {
        let ini: IniFile = IniFile::from_str("[HashTest]\nSounds= #sound1 #sound2\n");
        let reg: SoundRegistry = SoundRegistry::from_ini(&ini);
        let entry: &SoundEntry = reg.get("HashTest").expect("should find entry");
        assert_eq!(entry.sounds, vec!["sound1", "sound2"]);
    }

    #[test]
    fn test_inline_comment_filtered() {
        let ini: IniFile =
            IniFile::from_str("[CommentTest]\nSounds= irocdiea ;$irocdib $irocdic\n");
        let reg: SoundRegistry = SoundRegistry::from_ini(&ini);
        let entry: &SoundEntry = reg.get("CommentTest").expect("should find entry");
        assert_eq!(entry.sounds, vec!["irocdiea"]);
    }
}
