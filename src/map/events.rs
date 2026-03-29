//! Map event parsing.
//!
//! `[Events]` stores a counted list of event-condition chunks per trigger id.
//! We preserve the raw field list and also expose normalized conditions so
//! runtime code can evaluate them without hardcoding a flat row assumption.

use std::collections::HashMap;

use crate::rules::ini_parser::IniFile;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventCondition {
    pub kind: i32,
    pub params: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapEvent {
    pub id: String,
    pub fields: Vec<String>,
    pub conditions: Vec<EventCondition>,
}

pub type EventMap = HashMap<String, MapEvent>;

/// Parse `[Events]` into an id -> event record map.
pub fn parse_events(ini: &IniFile) -> EventMap {
    let Some(section) = ini.section("Events") else {
        return HashMap::new();
    };

    let mut events: EventMap = HashMap::new();
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
        let conditions = parse_event_conditions(&fields);
        events.insert(
            id.clone(),
            MapEvent {
                id,
                fields,
                conditions,
            },
        );
    }

    if !events.is_empty() {
        log::info!("Parsed {} events from [Events]", events.len());
    }
    events
}

fn parse_event_conditions(fields: &[String]) -> Vec<EventCondition> {
    if fields.is_empty() {
        return Vec::new();
    }

    let declared_count = fields[0].trim().parse::<usize>().ok();
    if let Some(count) = declared_count {
        let payload = &fields[1..];
        let chunk_len = 3;
        let max_chunks = payload.len() / chunk_len;
        let chunk_count = count.min(max_chunks);
        if chunk_count > 0 {
            return payload
                .chunks_exact(chunk_len)
                .take(chunk_count)
                .filter_map(|chunk| {
                    let kind = chunk[0].trim().parse::<i32>().ok()?;
                    Some(EventCondition {
                        kind,
                        params: chunk[1..].to_vec(),
                    })
                })
                .collect();
        }
    }

    let kind = fields[0].trim().parse::<i32>().ok();
    kind.map(|kind| {
        vec![EventCondition {
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
    fn test_parse_events() {
        let ini = IniFile::from_str("[Events]\nEV_A=2,47,3,0,27,7,0\nEV_B=2,7\n");
        let events = parse_events(&ini);
        assert_eq!(events.len(), 2);
        assert_eq!(
            events.get("EV_A"),
            Some(&MapEvent {
                id: "EV_A".to_string(),
                fields: vec![
                    "2".to_string(),
                    "47".to_string(),
                    "3".to_string(),
                    "0".to_string(),
                    "27".to_string(),
                    "7".to_string(),
                    "0".to_string()
                ],
                conditions: vec![
                    EventCondition {
                        kind: 47,
                        params: vec!["3".to_string(), "0".to_string()],
                    },
                    EventCondition {
                        kind: 27,
                        params: vec!["7".to_string(), "0".to_string()],
                    },
                ],
            })
        );
        assert_eq!(
            events.get("EV_B").map(|event| event.fields.as_slice()),
            Some(&["2".to_string(), "7".to_string()][..])
        );
        assert_eq!(
            events.get("EV_B").map(|event| event.conditions.as_slice()),
            Some(
                &[EventCondition {
                    kind: 2,
                    params: vec!["7".to_string()],
                }][..]
            )
        );
    }

    #[test]
    fn test_missing_events_is_empty() {
        let ini = IniFile::from_str("[Map]\nTheater=TEMPERATE\n");
        assert!(parse_events(&ini).is_empty());
    }
}
