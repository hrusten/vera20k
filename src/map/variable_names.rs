//! Map local variable definitions from `[VariableNames]`.
//!
//! The map file can define local boolean variables with optional initial set
//! state. Trigger events can test these locals, and trigger actions can mutate
//! them during runtime.

use std::collections::HashMap;

use crate::rules::ini_parser::IniFile;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalVariable {
    pub index: u32,
    pub name: String,
    pub initially_set: bool,
}

pub type LocalVariableMap = HashMap<u32, LocalVariable>;

pub fn parse_local_variables(ini: &IniFile) -> LocalVariableMap {
    let Some(section) = ini.section("VariableNames") else {
        return HashMap::new();
    };

    let mut locals = LocalVariableMap::new();
    for key in section.keys() {
        let Some(raw_value) = section.get(key) else {
            continue;
        };
        let Ok(index) = key.trim().parse::<u32>() else {
            continue;
        };
        let mut parts = raw_value.split(',').map(|part| part.trim());
        let name = parts.next().unwrap_or("").to_string();
        let initially_set = parts.next().map(|value| value == "1").unwrap_or(false);
        locals.insert(
            index,
            LocalVariable {
                index,
                name,
                initially_set,
            },
        );
    }

    if !locals.is_empty() {
        log::info!(
            "Parsed {} local variables from [VariableNames]",
            locals.len()
        );
    }
    locals
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_local_variables() {
        let ini = IniFile::from_str(
            "[VariableNames]\n0=BridgeFixed,1\n7=SpyEntered,0\njunk=IgnoreMe,1\n",
        );
        let vars = parse_local_variables(&ini);
        assert_eq!(vars.len(), 2);
        assert_eq!(
            vars.get(&0),
            Some(&LocalVariable {
                index: 0,
                name: "BridgeFixed".to_string(),
                initially_set: true,
            })
        );
        assert_eq!(
            vars.get(&7),
            Some(&LocalVariable {
                index: 7,
                name: "SpyEntered".to_string(),
                initially_set: false,
            })
        );
    }

    #[test]
    fn test_missing_variable_names_is_empty() {
        let ini = IniFile::from_str("[Map]\nTheater=TEMPERATE\n");
        assert!(parse_local_variables(&ini).is_empty());
    }
}
