//! Enum member names: `cleo\.config\enums.json`, the table the builder writes next to
//! `sa.json`.
//!
//! `sa.json` names the type of an argument - `WeaponType`, `CarDoor`, `HudObject` - but not
//! what the numbers of that type mean. This file is that table, and it is the only place
//! the panel gets a name for a value other than the value itself.
//!
//! Not every member is usable, and dropping the rest is the point of parsing it at all: the
//! symbol table keeps entries whose number it never learned (`"Torso": null`), and entries
//! whose value is a string (`FileMode`'s `rt`, `AnimGroup`'s `man`). Only the numbered ones
//! are kept, so a type that is all-`null` - `Town`, `BodyPart` - simply never resolves and
//! the panel goes on showing the bare number. Where two members share a number the first
//! one in the file wins; that is the order the builder's own lookup uses.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::Value as Json;

use crate::utils::cleo_dir;

/// One type's members: number → name.
#[derive(Debug, Default)]
pub struct Enum {
    members: HashMap<i64, String>,
}

impl Enum {
    /// The name `value` has in this type, if it has one.
    pub fn member(&self, value: i64) -> Option<&str> {
        self.members.get(&value).map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.members.len()
    }
}

/// The whole table: type name → its members.
#[derive(Debug, Default)]
pub struct Enums {
    types: HashMap<String, Arc<Enum>>,
}

impl Enums {
    /// One type, shared rather than copied: every argument of that type holds this.
    pub fn get(&self, type_name: &str) -> Option<&Arc<Enum>> {
        self.types.get(type_name)
    }

    pub fn len(&self) -> usize {
        self.types.len()
    }

    /// Every numbered member over every type, for the one log line.
    pub fn members(&self) -> usize {
        self.types.values().map(|table| table.len()).sum()
    }
}

/// `cleo\.config\enums.json`, beside the `sa.json` it complements.
pub fn path() -> PathBuf {
    Path::new(&cleo_dir()).join(".config").join("enums.json")
}

/// Parse the table. A missing file is not worth a warning: the panel then shows numbers
/// where it would have shown names, which is what it did before the file existed.
pub fn load() -> Option<Enums> {
    let path = path();
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) => {
            log::info!("script debug: no enum names ({}): {error}", path.display());
            return None;
        }
    };

    match from_json(&text) {
        Ok(enums) => {
            log::info!(
                "script debug: {} enums, {} members from {}",
                enums.len(),
                enums.members(),
                path.display()
            );
            Some(enums)
        }
        Err(error) => {
            log::warn!("script debug: cannot parse {}: {error}", path.display());
            None
        }
    }
}

/// Build the table from the text of an `enums.json`.
pub fn from_json(text: &str) -> Result<Enums, serde_json::Error> {
    let root: Json = serde_json::from_str(text)?;
    let mut types = HashMap::new();

    let Some(groups) = root.as_object() else {
        return Ok(Enums { types });
    };

    for (type_name, members) in groups {
        let Some(members) = members.as_object() else {
            continue;
        };

        let mut table = HashMap::with_capacity(members.len());
        for (name, value) in members {
            // `null` is a member the symbol table has no number for, and a string is a
            // value no integer operand can hold. Both are skipped, not guessed at.
            let Some(value) = value.as_i64() else {
                continue;
            };
            // Object order is file order (`preserve_order`), so the first entry stays.
            table.entry(value).or_insert_with(|| name.clone());
        }

        if !table.is_empty() {
            types.insert(type_name.clone(), Arc::new(Enum { members: table }));
        }
    }

    Ok(Enums { types })
}
