//! INI file parser for RA2 game data (rules.ini, art.ini, map files).
//!
//! RA2's INI files are the backbone of all game data. rules.ini alone defines
//! every unit, building, weapon, warhead, projectile, and game constant.
//! This parser handles the specific quirks of Westwood's INI format:
//!
//! ## Format details
//! - Sections: `[SectionName]`
//! - Key-value pairs: `Key=Value`
//! - Comments: lines starting with `;` or `#`
//! - Inline comments: `;` after a value (e.g., `Cost=1000 ; credits`)
//! - Case-insensitive section and key lookups
//! - Duplicate sections merge (later keys override earlier ones)
//! - Blank lines are ignored
//! - Lines without `=` and not a section header are skipped (with a warning)
//!
//! ## Dependency rules
//! - Part of rules/ — depends on assets/ for loading data from MIX archives.
//! - No dependencies on sim/, render/, ui/, etc.

use std::collections::HashMap;

use crate::rules::error::RulesError;

/// A single section of an INI file, containing key-value pairs.
///
/// All lookups are case-insensitive (keys stored as lowercase internally).
/// Original key casing is not preserved — RA2 INI files are case-insensitive.
#[derive(Debug, Clone)]
pub struct IniSection {
    /// Section name as it appeared in the file (original casing preserved).
    pub name: String,
    /// Key-value pairs, with keys stored in lowercase for case-insensitive lookup.
    /// Values are stored as-is (trimmed of whitespace but not lowercased).
    entries: HashMap<String, String>,
    /// Keys in insertion order, for ordered iteration.
    /// Stored in lowercase to match `entries` keys.
    key_order: Vec<String>,
}

impl IniSection {
    /// Create a new empty section with the given name.
    fn new(name: String) -> Self {
        Self {
            name,
            entries: HashMap::new(),
            key_order: Vec::new(),
        }
    }

    /// Set a key-value pair. If the key already exists, the value is overwritten.
    pub(crate) fn set(&mut self, key: &str, value: &str) {
        let lower_key: String = key.to_ascii_lowercase();
        if !self.entries.contains_key(&lower_key) {
            self.key_order.push(lower_key.clone());
        }
        self.entries.insert(lower_key, value.to_string());
    }

    /// Get a value by key (case-insensitive).
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .get(&key.to_ascii_lowercase())
            .map(|s| s.as_str())
    }

    /// Get a value parsed as i32.
    ///
    /// Returns None if the key doesn't exist or the value isn't a valid integer.
    pub fn get_i32(&self, key: &str) -> Option<i32> {
        self.get(key)?.trim().parse::<i32>().ok()
    }

    /// Get a value parsed as f32.
    ///
    /// Returns None if the key doesn't exist or the value isn't a valid float.
    pub fn get_f32(&self, key: &str) -> Option<f32> {
        self.get(key)?.trim().parse::<f32>().ok()
    }

    /// Get a value parsed as a percentage (0.0–1.0).
    ///
    /// Handles both `"25%"` (divides by 100 → 0.25) and bare floats like `"0.25"`.
    /// Returns None if the key doesn't exist or the value can't be parsed.
    pub fn get_percent(&self, key: &str) -> Option<f32> {
        let val: &str = self.get(key)?.trim();
        if let Some(stripped) = val.strip_suffix('%') {
            stripped.trim().parse::<f32>().ok().map(|v| v / 100.0)
        } else {
            val.parse::<f32>().ok()
        }
    }

    /// Get a value parsed as boolean.
    ///
    /// Recognizes: "yes", "true", "1" → true; "no", "false", "0" → false.
    /// Case-insensitive. Returns None if the key doesn't exist or the value
    /// isn't a recognized boolean string.
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        let val: &str = self.get(key)?.trim();
        match val.to_ascii_lowercase().as_str() {
            "yes" | "true" | "1" => Some(true),
            "no" | "false" | "0" => Some(false),
            _ => None,
        }
    }

    /// Get a value parsed as a comma-separated list of strings.
    ///
    /// Splits on commas and trims whitespace from each element.
    /// Returns None if the key doesn't exist.
    /// Returns an empty Vec if the value is empty.
    pub fn get_list(&self, key: &str) -> Option<Vec<&str>> {
        let val: &str = self.get(key)?;
        if val.trim().is_empty() {
            return Some(Vec::new());
        }
        Some(val.split(',').map(|s| s.trim()).collect())
    }

    /// Collect values from numbered keys in order.
    ///
    /// RA2 type registries (e.g., [InfantryTypes]) use numbered keys:
    /// `1=E1`, `2=E2`, `3=ENGINEER`, ... (1-indexed in retail rules.ini).
    /// Some merged RA2/YR registries contain gaps after patching, so collect
    /// every numeric key and return the values sorted by numeric index.
    pub fn get_values(&self) -> Vec<&str> {
        let mut numbered: Vec<(u32, &str)> = self
            .key_order
            .iter()
            .filter_map(|key| {
                key.parse::<u32>().ok().and_then(|index| {
                    self.entries
                        .get(key)
                        .map(|value| (index, value.as_str()))
                        .filter(|(_, value)| !value.is_empty())
                })
            })
            .collect();
        numbered.sort_by_key(|(index, _)| *index);
        numbered.into_iter().map(|(_, value)| value).collect()
    }

    /// Number of key-value pairs in this section.
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Iterate over keys in insertion order.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.key_order.iter().map(|s| s.as_str())
    }
}

/// A parsed INI file containing named sections.
///
/// Sections are looked up case-insensitively. Duplicate section headers
/// in the source file are merged (later keys override earlier ones within
/// the same section, matching RA2 behavior).
#[derive(Debug, Clone)]
pub struct IniFile {
    /// Sections indexed by lowercase name for case-insensitive lookup.
    sections: HashMap<String, IniSection>,
    /// Section names in order of first appearance (lowercase).
    section_order: Vec<String>,
}

impl IniFile {
    /// Parse an INI file from raw bytes.
    ///
    /// The data is expected to be UTF-8 (or ASCII, which is a subset).
    /// RA2 INI files are ASCII, so this should always work.
    pub fn from_bytes(data: &[u8]) -> Result<Self, RulesError> {
        let text: &str = std::str::from_utf8(data)?;
        Ok(Self::from_str(text))
    }

    /// Parse an INI file from a string.
    ///
    /// Handles all Westwood INI quirks: comments, inline comments,
    /// duplicate sections, blank lines, and malformed lines.
    pub fn from_str(text: &str) -> Self {
        let mut sections: HashMap<String, IniSection> = HashMap::new();
        let mut section_order: Vec<String> = Vec::new();
        let mut current_section: Option<String> = None;

        for (line_num, raw_line) in text.lines().enumerate() {
            let line: &str = raw_line.trim();

            // Skip empty lines.
            if line.is_empty() {
                continue;
            }

            // Skip full-line comments (`;` or `#` at start).
            if line.starts_with(';') || line.starts_with('#') {
                continue;
            }

            // Section header: [SectionName]
            if line.starts_with('[') {
                if let Some(end) = line.find(']') {
                    let name: &str = &line[1..end];
                    let lower_name: String = name.to_ascii_lowercase();

                    if !sections.contains_key(&lower_name) {
                        sections.insert(lower_name.clone(), IniSection::new(name.to_string()));
                        section_order.push(lower_name.clone());
                    }

                    current_section = Some(lower_name);
                    continue;
                }
                // Malformed section header (no closing bracket) — skip.
                log::warn!(
                    "INI line {}: malformed section header: {}",
                    line_num + 1,
                    line
                );
                continue;
            }

            // Key=Value pair.
            if let Some(eq_pos) = line.find('=') {
                let key: &str = line[..eq_pos].trim();
                let mut value: &str = line[eq_pos + 1..].trim();

                // Strip inline comments: everything after `;` is a comment.
                // RA2's INI parser always treats `;` as a comment marker,
                // regardless of surrounding whitespace (e.g., "Image=USELESS;comment").
                if let Some(comment_pos) = value.find(';') {
                    value = value[..comment_pos].trim();
                }

                if let Some(section_key) = &current_section {
                    if let Some(section) = sections.get_mut(section_key) {
                        section.set(key, value);
                    }
                } else {
                    // Key=Value before any section header — skip with warning.
                    log::warn!(
                        "INI line {}: key '{}' outside any section (ignored)",
                        line_num + 1,
                        key
                    );
                }

                continue;
            }

            // Line is not empty, not a comment, not a section, not a key=value.
            // RA2 INI files sometimes have junk lines — skip with a warning.
            log::trace!(
                "INI line {}: unrecognized line format (skipped): {}",
                line_num + 1,
                line
            );
        }

        Self {
            sections,
            section_order,
        }
    }

    /// Look up a section by name (case-insensitive).
    pub fn section(&self, name: &str) -> Option<&IniSection> {
        self.sections.get(&name.to_ascii_lowercase())
    }

    /// Get all section names in order of first appearance.
    pub fn section_names(&self) -> Vec<&str> {
        self.section_order
            .iter()
            .filter_map(|key| self.sections.get(key).map(|s| s.name.as_str()))
            .collect()
    }

    /// Number of sections in this INI file.
    pub fn section_count(&self) -> usize {
        self.sections.len()
    }

    /// Merge another INI file on top of this one (patch/overlay).
    ///
    /// In YR, rulesmd.ini / artmd.ini are patches on top of rules.ini / art.ini.
    /// For each section in `patch`: if the section exists in self, merge keys
    /// (patch keys override base keys). If the section is new, add it.
    pub fn merge(&mut self, patch: &IniFile) {
        for patch_key in &patch.section_order {
            let patch_section: &IniSection = match patch.sections.get(patch_key) {
                Some(s) => s,
                None => continue,
            };
            if let Some(base_section) = self.sections.get_mut(patch_key) {
                // Merge keys: patch overrides base.
                for key in patch_section.keys() {
                    if let Some(val) = patch_section.get(key) {
                        base_section.set(key, val);
                    }
                }
            } else {
                // New section from patch.
                self.sections
                    .insert(patch_key.clone(), patch_section.clone());
                self.section_order.push(patch_key.clone());
            }
        }
    }
}

#[cfg(test)]
#[path = "ini_parser_tests.rs"]
mod tests;
