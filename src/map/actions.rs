//! Map action parsing.
//!
//! `[Actions]` stores a counted list of 8-field action chunks per trigger id.
//! We preserve the raw field list and also expose normalized action entries so
//! runtime code can stop guessing at the original payload shape.

use std::collections::HashMap;

use crate::rules::ini_parser::IniFile;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionEntry {
    pub kind: i32,
    pub params: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapAction {
    pub id: String,
    pub fields: Vec<String>,
    pub entries: Vec<ActionEntry>,
}

pub type ActionMap = HashMap<String, MapAction>;

/// Parse `[Actions]` into an id -> action record map.
pub fn parse_actions(ini: &IniFile) -> ActionMap {
    let Some(section) = ini.section("Actions") else {
        return HashMap::new();
    };

    let mut actions: ActionMap = HashMap::new();
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
        let entries = parse_action_entries(&fields);
        actions.insert(
            id.clone(),
            MapAction {
                id,
                fields,
                entries,
            },
        );
    }

    if !actions.is_empty() {
        log::info!("Parsed {} actions from [Actions]", actions.len());
    }
    actions
}

fn parse_action_entries(fields: &[String]) -> Vec<ActionEntry> {
    if fields.is_empty() {
        return Vec::new();
    }

    let declared_count = fields[0].trim().parse::<usize>().ok();
    if let Some(count) = declared_count {
        let payload = &fields[1..];
        let chunk_len = 8;
        let max_chunks = payload.len() / chunk_len;
        let chunk_count = count.min(max_chunks);
        if chunk_count > 0 {
            return payload
                .chunks_exact(chunk_len)
                .take(chunk_count)
                .filter_map(|chunk| {
                    let kind = chunk[0].trim().parse::<i32>().ok()?;
                    Some(ActionEntry {
                        kind,
                        params: chunk[1..].to_vec(),
                    })
                })
                .collect();
        }
    }

    let kind = fields[0].trim().parse::<i32>().ok();
    kind.map(|kind| {
        vec![ActionEntry {
            kind,
            params: fields[1..].to_vec(),
        }]
    })
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_actions() {
        let ini = IniFile::from_str(
            "[Actions]\nAC_A=2,28,7,0,0,0,0,0,0,112,0,0,0,0,0,0,9\nAC_B=11,Americans,5\n",
        );
        let actions = parse_actions(&ini);
        assert_eq!(actions.len(), 2);
        assert_eq!(
            actions.get("AC_A"),
            Some(&MapAction {
                id: "AC_A".to_string(),
                fields: vec![
                    "2".to_string(),
                    "28".to_string(),
                    "7".to_string(),
                    "0".to_string(),
                    "0".to_string(),
                    "0".to_string(),
                    "0".to_string(),
                    "0".to_string(),
                    "0".to_string(),
                    "112".to_string(),
                    "0".to_string(),
                    "0".to_string(),
                    "0".to_string(),
                    "0".to_string(),
                    "0".to_string(),
                    "0".to_string(),
                    "9".to_string(),
                ],
                entries: vec![
                    ActionEntry {
                        kind: 28,
                        params: vec![
                            "7".to_string(),
                            "0".to_string(),
                            "0".to_string(),
                            "0".to_string(),
                            "0".to_string(),
                            "0".to_string(),
                            "0".to_string(),
                        ],
                    },
                    ActionEntry {
                        kind: 112,
                        params: vec![
                            "0".to_string(),
                            "0".to_string(),
                            "0".to_string(),
                            "0".to_string(),
                            "0".to_string(),
                            "0".to_string(),
                            "9".to_string(),
                        ],
                    },
                ],
            })
        );
        assert_eq!(
            actions.get("AC_B").map(|action| action.fields.as_slice()),
            Some(&["11".to_string(), "Americans".to_string(), "5".to_string()][..])
        );
        assert_eq!(
            actions.get("AC_B").map(|action| action.entries.as_slice()),
            Some(
                &[ActionEntry {
                    kind: 11,
                    params: vec!["Americans".to_string(), "5".to_string()],
                }][..]
            )
        );
    }

    #[test]
    fn test_missing_actions_is_empty() {
        let ini = IniFile::from_str("[Map]\nTheater=TEMPERATE\n");
        assert!(parse_actions(&ini).is_empty());
    }
}
