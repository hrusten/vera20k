//! Parser for the `[Basic]` map section.
//!
//! `[Basic]` carries scenario/map metadata used by the original games and tools.
//! This module keeps the first-pass support intentionally narrow: parse the most
//! useful metadata now and leave the broader scenario semantics for later work.

use crate::rules::ini_parser::IniFile;

/// Parsed metadata from a map's `[Basic]` section.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BasicSection {
    /// Human-facing map/scenario name when present.
    pub name: Option<String>,
    /// Author text when present.
    pub author: Option<String>,
    /// String-table key or raw intro/briefing hook.
    pub intro: Option<String>,
    /// String-table key or raw briefing hook.
    pub briefing: Option<String>,
    /// Theme/music id requested by the map.
    pub theme: Option<String>,
    /// Declared INI format version used by the map.
    pub new_ini_format: Option<i32>,
    /// Whether tiberium/ore growth is enabled for this map (TiberiumGrowthEnabled=).
    pub tiberium_growth_enabled: Option<bool>,
}

/// Parsed flags from a map's `[SpecialFlags]` section.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpecialFlagsSection {
    /// Map-level override: does ore grow denser? (TiberiumGrows=)
    pub tiberium_grows: Option<bool>,
    /// Map-level override: does ore spread to adjacent cells? (TiberiumSpreads=)
    pub tiberium_spreads: Option<bool>,
    /// Map-level override: are bridges destroyable? (DestroyableBridges=)
    pub destroyable_bridges: Option<bool>,
}

/// Parse the `[Basic]` section from a map INI.
pub fn parse_basic_section(ini: &IniFile) -> BasicSection {
    let Some(section) = ini.section("Basic") else {
        return BasicSection::default();
    };

    BasicSection {
        name: section.get("Name").map(str::to_string),
        author: section.get("Author").map(str::to_string),
        intro: section.get("Intro").map(str::to_string),
        briefing: section.get("Brief").map(str::to_string),
        theme: section.get("Theme").map(str::to_string),
        new_ini_format: section.get_i32("NewINIFormat"),
        tiberium_growth_enabled: section.get_bool("TiberiumGrowthEnabled"),
    }
}

/// Parse the `[SpecialFlags]` section from a map INI.
pub fn parse_special_flags_section(ini: &IniFile) -> SpecialFlagsSection {
    let Some(section) = ini.section("SpecialFlags") else {
        return SpecialFlagsSection::default();
    };

    SpecialFlagsSection {
        tiberium_grows: section.get_bool("TiberiumGrows"),
        tiberium_spreads: section.get_bool("TiberiumSpreads"),
        destroyable_bridges: section.get_bool("DestroyableBridges"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::ini_parser::IniFile;

    #[test]
    fn parse_basic_metadata() {
        let ini = IniFile::from_str(
            "[Basic]\nName=Mission One\nAuthor=Westwood\nIntro=TXT_M01INTRO\n\
             Brief=TXT_M01BRIEF\nTheme=BIGF226M\nNewINIFormat=4\n",
        );

        let basic = parse_basic_section(&ini);
        assert_eq!(basic.name.as_deref(), Some("Mission One"));
        assert_eq!(basic.author.as_deref(), Some("Westwood"));
        assert_eq!(basic.intro.as_deref(), Some("TXT_M01INTRO"));
        assert_eq!(basic.briefing.as_deref(), Some("TXT_M01BRIEF"));
        assert_eq!(basic.theme.as_deref(), Some("BIGF226M"));
        assert_eq!(basic.new_ini_format, Some(4));
    }

    #[test]
    fn missing_basic_section_returns_defaults() {
        let ini = IniFile::from_str("[Map]\nTheater=TEMPERATE\n");
        let basic = parse_basic_section(&ini);
        assert_eq!(basic, BasicSection::default());
    }

    #[test]
    fn parse_special_flags_bridge_override() {
        let ini = IniFile::from_str(
            "[SpecialFlags]\nTiberiumGrows=yes\nTiberiumSpreads=no\nDestroyableBridges=no\n",
        );
        let flags = parse_special_flags_section(&ini);
        assert_eq!(flags.tiberium_grows, Some(true));
        assert_eq!(flags.tiberium_spreads, Some(false));
        assert_eq!(flags.destroyable_bridges, Some(false));
    }
}
