//! Map tag parsing.
//!
//! `[Tags]` assigns stable identifiers to raw map tag records. We keep the
//! parsing low-assumption for now so later trigger work can decide semantics
//! without having to undo an early guess.

use std::collections::HashMap;

use crate::rules::ini_parser::IniFile;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapTag {
    pub id: String,
    pub fields: Vec<String>,
}

pub type TagMap = HashMap<String, MapTag>;

/// Parse `[Tags]` into a tag-id keyed map.
pub fn parse_tags(ini: &IniFile) -> TagMap {
    let Some(section) = ini.section("Tags") else {
        return HashMap::new();
    };

    let mut tags: TagMap = HashMap::new();
    for key in section.keys() {
        let Some(raw_value) = section.get(key) else {
            continue;
        };
        let id = key.trim();
        if id.is_empty() {
            continue;
        }
        let id = id.to_ascii_uppercase();
        let fields: Vec<String> = raw_value
            .split(',')
            .map(|part| part.trim().to_string())
            .collect();
        tags.insert(id.clone(), MapTag { id, fields });
    }

    if !tags.is_empty() {
        log::info!("Parsed {} tags from [Tags]", tags.len());
    }
    tags
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tags() {
        let ini = IniFile::from_str("[Tags]\nTRIG_A=0,1,SomeName\nOBJ_01=2,0\n");
        let tags = parse_tags(&ini);
        assert_eq!(tags.len(), 2);
        assert_eq!(
            tags.get("TRIG_A"),
            Some(&MapTag {
                id: "TRIG_A".to_string(),
                fields: vec!["0".to_string(), "1".to_string(), "SomeName".to_string()],
            })
        );
        assert_eq!(
            tags.get("OBJ_01").map(|tag| tag.fields.as_slice()),
            Some(&["2".to_string(), "0".to_string()][..])
        );
    }

    #[test]
    fn test_missing_tags_is_empty() {
        let ini = IniFile::from_str("[Map]\nTheater=TEMPERATE\n");
        assert!(parse_tags(&ini).is_empty());
    }
}
