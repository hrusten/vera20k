//! Map trigger parsing.
//!
//! `[Triggers]` defines the raw trigger records that later connect tags,
//! events, and actions into actual scenario behavior. We preserve the raw
//! fields and also expose the key structured values needed by runtime logic.
//! Format per ModEnc for RA2/YR maps:
//! `ID=HOUSE,<TRIGGER>,NAME,A,B,C,D,E`

use std::collections::HashMap;

use crate::rules::ini_parser::IniFile;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TriggerDifficulty {
    pub easy: bool,
    pub medium: bool,
    pub hard: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapTrigger {
    pub id: String,
    pub fields: Vec<String>,
    pub owner: Option<String>,
    pub linked_trigger_id: Option<String>,
    pub name: Option<String>,
    pub enabled: bool,
    pub difficulty: TriggerDifficulty,
    pub repeating: bool,
}

pub type TriggerMap = HashMap<String, MapTrigger>;

/// Parse `[Triggers]` into an id -> raw trigger record map.
pub fn parse_triggers(ini: &IniFile) -> TriggerMap {
    let Some(section) = ini.section("Triggers") else {
        return HashMap::new();
    };

    let mut triggers: TriggerMap = HashMap::new();
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
        triggers.insert(
            id.clone(),
            MapTrigger {
                id,
                owner: parse_owner(&fields),
                linked_trigger_id: parse_linked_trigger(&fields),
                name: parse_name(&fields),
                enabled: parse_bool_slot(&fields, 3, true),
                difficulty: TriggerDifficulty {
                    easy: parse_bool_slot(&fields, 4, true),
                    medium: parse_bool_slot(&fields, 5, true),
                    hard: parse_bool_slot(&fields, 6, true),
                },
                repeating: parse_repeat_mode(&fields),
                fields,
            },
        );
    }

    if !triggers.is_empty() {
        log::info!("Parsed {} triggers from [Triggers]", triggers.len());
    }
    triggers
}

fn parse_owner(fields: &[String]) -> Option<String> {
    let owner = fields.first()?.trim();
    (!owner.is_empty()).then(|| owner.to_string())
}

fn parse_linked_trigger(fields: &[String]) -> Option<String> {
    let value = fields.get(1)?.trim();
    if value.is_empty() || value.eq_ignore_ascii_case("<none>") {
        None
    } else {
        Some(value.to_ascii_uppercase())
    }
}

fn parse_name(fields: &[String]) -> Option<String> {
    let name = fields.get(2)?.trim();
    (!name.is_empty()).then(|| name.to_string())
}

fn parse_bool_slot(fields: &[String], index: usize, default: bool) -> bool {
    fields
        .get(index)
        .map(|value| value.trim() == "1")
        .unwrap_or(default)
}

fn parse_repeat_mode(fields: &[String]) -> bool {
    fields
        .get(7)
        .map(|value| value.trim() == "2")
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_triggers() {
        let ini = IniFile::from_str(
            "[Triggers]\nTR_A=Neutral,TR_NEXT,Alpha Trigger,1,1,0,1,0\nTR_B=House,<none>,Beta Trigger\n",
        );
        let triggers = parse_triggers(&ini);
        assert_eq!(triggers.len(), 2);
        assert_eq!(
            triggers.get("TR_A"),
            Some(&MapTrigger {
                id: "TR_A".to_string(),
                owner: Some("Neutral".to_string()),
                linked_trigger_id: Some("TR_NEXT".to_string()),
                name: Some("Alpha Trigger".to_string()),
                enabled: true,
                difficulty: TriggerDifficulty {
                    easy: true,
                    medium: false,
                    hard: true,
                },
                repeating: false,
                fields: vec![
                    "Neutral".to_string(),
                    "TR_NEXT".to_string(),
                    "Alpha Trigger".to_string(),
                    "1".to_string(),
                    "1".to_string(),
                    "0".to_string(),
                    "1".to_string(),
                    "0".to_string()
                ],
            })
        );
        assert_eq!(
            triggers
                .get("TR_B")
                .map(|trigger| trigger.fields.as_slice()),
            Some(
                &[
                    "House".to_string(),
                    "<none>".to_string(),
                    "Beta Trigger".to_string(),
                ][..]
            )
        );
        assert_eq!(
            triggers
                .get("TR_B")
                .map(|trigger| trigger.linked_trigger_id.as_deref()),
            Some(None)
        );
    }

    #[test]
    fn test_missing_triggers_is_empty() {
        let ini = IniFile::from_str("[Map]\nTheater=TEMPERATE\n");
        assert!(parse_triggers(&ini).is_empty());
    }
}
