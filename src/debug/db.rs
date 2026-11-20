//! Command metadata: `cleo\.config\sa.json`, the same file CLEO core reads.
//!
//! Only what the debug view needs is kept - the command's name, its operator, and the
//! names and types of its input and output arguments. The file is parsed once, on the
//! first debug window, because it is 1.8 MB and nothing on the other tabs looks at it.
//!
//! Commands the file does not list (opcodes registered at run time by another plugin, or
//! a missing database) still decode; they just render without a name.
//!
//! A named type (`WeaponType`, `CarDoor`) is only a number until [`enums`] says what the
//! numbers mean, so each argument of such a type carries the members that file gave it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::Value as Json;

use super::enums::{self, Enum, Enums};
use crate::utils::cleo_dir;

/// Presentation class of one argument, taken from the `type` string in `sa.json`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ArgKind {
    /// `any`: no idea what it holds, so the value is printed the way it is encoded.
    Any,
    Int,
    Float,
    Bool,
    /// A jump target, printed as an address.
    Label,
    /// A text literal or a text variable.
    String,
    Model,
    ScriptId,
    /// An enum or a class; printed as the raw number.
    Other,
}

fn kind_of(text: &str) -> ArgKind {
    match text.to_ascii_lowercase().as_str() {
        "int" => ArgKind::Int,
        "float" => ArgKind::Float,
        "bool" => ArgKind::Bool,
        "label" => ArgKind::Label,
        "string" | "string128" | "gxt_key" | "zone_key" => ArgKind::String,
        "model_any" | "model_char" | "model_object" | "model_vehicle" => ArgKind::Model,
        "script_id" => ArgKind::ScriptId,
        "any" => ArgKind::Any,
        _ => ArgKind::Other,
    }
}

#[derive(Clone, Debug)]
pub struct Arg {
    pub name: String,
    pub kind: ArgKind,
    /// The members of the type `sa.json` names for this argument, when `enums.json` has
    /// them. Shared, not copied: every argument of one type points at the same table.
    ///
    /// `None` for the primitive types, and for a named type whose members are all
    /// unknowns - which is what `null` members mean.
    pub enum_type: Option<Arc<Enum>>,
}

impl Arg {
    /// What `value` is called in this argument's enum, if anything.
    pub fn member(&self, value: i32) -> Option<&str> {
        self.enum_type.as_ref()?.member(i64::from(value))
    }
}

#[derive(Clone, Debug)]
pub struct Command {
    /// Lower case, like the script log prints it.
    pub name: String,
    /// Assignment or comparison operator, empty for a plain call.
    pub oper: String,
    pub inputs: Vec<Arg>,
    /// Index into [`inputs`](Self::inputs) of the `arguments` slot, if the command takes a
    /// variable number of them. Every argument after that is part of the same list.
    pub varargs_at: Option<usize>,
    pub outputs: Vec<Arg>,
}

impl Command {
    pub fn input(&self, index: usize) -> Option<&Arg> {
        if let Some(varargs) = self.varargs_at {
            if index >= varargs {
                return self.inputs.get(varargs);
            }
        }
        self.inputs.get(index)
    }

    pub fn output(&self, index: usize) -> Option<&Arg> {
        self.outputs.get(index)
    }
}

pub struct Db {
    /// Version string from the file's `meta` block, for the header line.
    pub version: String,
    commands: HashMap<u16, Command>,
}

impl Db {
    pub fn get(&self, opcode: u16) -> Option<&Command> {
        self.commands.get(&opcode)
    }

    pub fn len(&self) -> usize {
        self.commands.len()
    }
}

/// `cleo\.config\sa.json`, next to the ini this overlay already writes.
pub fn path() -> PathBuf {
    Path::new(&cleo_dir()).join(".config").join("sa.json")
}

/// Parse the database. A missing or malformed file is not an error worth a warning per
/// frame: the caller ends up with `None` and the trace renders bare opcodes.
pub fn load() -> Option<Db> {
    let path = path();
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) => {
            log::warn!("script debug: cannot read {}: {error}", path.display());
            return None;
        }
    };

    // The enum names are a separate file and optional: without it an argument of a named
    // type shows its number, which is what the panel did before the file was read.
    let enums = enums::load();

    let db = match from_json_with(&text, enums.as_ref()) {
        Ok(db) => db,
        Err(error) => {
            log::warn!("script debug: cannot parse {}: {error}", path.display());
            return None;
        }
    };

    log::info!(
        "script debug: {} commands from {} (database {})",
        db.len(),
        path.display(),
        db.version
    );

    Some(db)
}

/// Build the command table from the text of a `sa.json`, without enum names.
#[allow(dead_code)] // the database's integration tests parse a fixture this way
pub fn from_json(text: &str) -> Result<Db, serde_json::Error> {
    from_json_with(text, None)
}

/// Build the command table from the text of a `sa.json`, naming the values of the types
/// `enums` knows.
pub fn from_json_with(text: &str, enums: Option<&Enums>) -> Result<Db, serde_json::Error> {
    let root: Json = serde_json::from_str(text)?;

    let mut db = parse(&root, enums);
    db.version = root
        .get("meta")
        .and_then(|meta| meta.get("version"))
        .and_then(Json::as_str)
        .unwrap_or("?")
        .to_string();

    Ok(db)
}

/// Build the command table from a parsed `sa.json`.
fn parse(root: &Json, enums: Option<&Enums>) -> Db {
    let mut commands = HashMap::new();

    let extensions = root
        .get("extensions")
        .and_then(Json::as_array)
        .map(|list| list.as_slice())
        .unwrap_or_default();

    for extension in extensions {
        let Some(list) = extension.get("commands").and_then(Json::as_array) else {
            continue;
        };

        for command in list {
            // An unsupported command is not compiled by the builder, so anything that
            // reaches the game with that opcode is not this command.
            if command
                .get("attrs")
                .and_then(|attrs| attrs.get("is_unsupported"))
                .and_then(Json::as_bool)
                .unwrap_or(false)
            {
                continue;
            }

            let Some(id) = command
                .get("id")
                .and_then(Json::as_str)
                .and_then(|id| u16::from_str_radix(id, 16).ok())
            else {
                continue;
            };
            // The not-flag lives in bit 15; a command never owns that bit.
            if id > 0x7FFF {
                continue;
            }
            let Some(name) = command.get("name").and_then(Json::as_str) else {
                continue;
            };

            // Two extensions can register the same opcode (the C++ loader keeps the first
            // and appends the second's name); the first one wins here too.
            if commands.contains_key(&id) {
                continue;
            }

            // Sanny's data names the operator by the operation itself - `+`, `^`, `<<` - and
            // the assignment it stands for is the `=` Sanny's own writer puts after it: the
            // script says `1@ += 16`, not `1@ + 16`. Two kinds of command carry the bare
            // sign:
            //
            // * an in-place command, whose first input is the destination and which has no
            //   output (`0008`, `0B19`): the operator *is* an assignment, so `=` is added;
            // * a command with an output (`0B12`), which writes `2@ = 1@ ^ 3@`: there the
            //   sign is the operation alone, and adding `=` would produce nonsense.
            //
            // A condition (`0018`, the `>` family, which the file marks in
            // `attrs.is_condition`) is the third case: its sign is likewise the whole
            // operator, `0@ > 16`, never an assignment.
            let mut oper = command
                .get("operator")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_string();
            let condition = command
                .get("attrs")
                .and_then(|attrs| attrs.get("is_condition"))
                .and_then(Json::as_bool)
                .unwrap_or(false);
            let (inputs, varargs_at) = arguments(command.get("input"), enums);
            let (outputs, _) = arguments(command.get("output"), enums);
            if !condition && !oper.is_empty() && outputs.is_empty() && !oper.contains('=') {
                oper.push('=');
            }

            commands.insert(
                id,
                Command {
                    name: name.to_ascii_lowercase(),
                    oper,
                    inputs,
                    varargs_at,
                    outputs,
                },
            );
        }
    }

    Db {
        version: String::new(),
        commands,
    }
}

/// The `type` the vararg slot carries in `sa.json`.
const VARARGS: &str = "arguments";

/// The arguments of one side of a command, and the index of its vararg slot.
///
/// The vararg slot is the argument whose *type* is `arguments`; that is not a value type,
/// so it is stored as `Any` and never gets a name to print.
fn arguments(list: Option<&Json>, enums: Option<&Enums>) -> (Vec<Arg>, Option<usize>) {
    let Some(list) = list.and_then(Json::as_array) else {
        return (Vec::new(), None);
    };

    let mut args = Vec::with_capacity(list.len());
    let mut varargs_at = None;

    for arg in list {
        let type_name = arg.get("type").and_then(Json::as_str).unwrap_or("any");

        if type_name.eq_ignore_ascii_case(VARARGS) {
            varargs_at = Some(args.len());
            args.push(Arg {
                name: String::new(),
                kind: ArgKind::Any,
                enum_type: None,
            });
            continue;
        }

        args.push(Arg {
            name: arg
                .get("name")
                .and_then(Json::as_str)
                .unwrap_or_default()
                .to_string(),
            kind: kind_of(type_name),
            enum_type: enums.and_then(|enums| enums.get(type_name)).cloned(),
        });
    }

    (args, varargs_at)
}

#[cfg(test)]
mod tests {
    use super::from_json;

    /// A command table with the three shapes that carry an operator: one that assigns into
    /// its first argument, one whose result goes to an output, and one that is a condition.
    fn table() -> super::Db {
        let json = r#"{
            "meta": { "version": "test" },
            "extensions": [{
                "name": "default",
                "commands": [
                    { "id": "0008", "name": "ADD_VAL_TO_INT_VAR", "operator": "+",
                      "input": [{ "type": "int" }, { "type": "int" }] },
                    { "id": "0058", "name": "ADD_VAL_TO_INT_VAR_TO_INT_VAR", "operator": "+",
                      "input": [{ "type": "int" }, { "type": "int" }],
                      "output": [{ "type": "int" }] },
                    { "id": "0B19", "name": "BIT_XOR_COMPOUND", "operator": "^",
                      "input": [{ "type": "int" }, { "type": "int" }] },
                    { "id": "0018", "name": "IS_INT_VAR_GREATER_THAN_NUMBER", "operator": ">",
                      "attrs": { "is_condition": true },
                      "input": [{ "type": "int" }, { "type": "int" }] },
                    { "id": "0004", "name": "SET_VAR_INT", "operator": "=",
                      "input": [{ "type": "int" }, { "type": "int" }] }
                ]
            }]
        }"#;
        from_json(json).expect("fixture parses")
    }

    #[test]
    fn an_in_place_operator_is_an_assignment() {
        // `1@ += 16`: the first input is the destination, so the sign carries the `=`.
        assert_eq!(table().get(0x0008).unwrap().oper, "+=");
        assert_eq!(table().get(0x0B19).unwrap().oper, "^=");
        // Already an assignment: nothing is added twice.
        assert_eq!(table().get(0x0004).unwrap().oper, "=");
    }

    #[test]
    fn an_operator_with_an_output_stays_an_expression() {
        // `0@ = 1@ + 2@`: the operator is the operation, the output is the assignment.
        assert_eq!(table().get(0x0058).unwrap().oper, "+");
    }

    #[test]
    fn a_condition_keeps_its_sign() {
        // `0@ > 16`, never `0@ >= 16`.
        assert_eq!(table().get(0x0018).unwrap().oper, ">");
    }

    /// A command with no operator at all - `goto`, `gosub`, `wait` - writes its own name and
    /// arguments. Handing it a `=` turns the line into `= @label` and loses the name, which
    /// is how a jump came to look like an assignment missing its output.
    #[test]
    fn a_command_without_an_operator_keeps_none() {
        let json = r#"{
            "extensions": [{ "name": "default", "commands": [
                { "id": "0002", "name": "GOTO", "input": [{ "type": "label" }] },
                { "id": "004D", "name": "GOTO_IF_FALSE", "input": [{ "type": "label" }] },
                { "id": "0008", "name": "ADD_VAL_TO_INT_VAR", "operator": "+",
                  "input": [{ "type": "int" }, { "type": "int" }] }
            ] }]
        }"#;
        let db = from_json(json).expect("fixture parses");
        assert_eq!(db.get(0x0002).unwrap().oper, "");
        assert_eq!(db.get(0x004D).unwrap().oper, "");
        // Still normalised next door, so the empty case is the only one left alone.
        assert_eq!(db.get(0x0008).unwrap().oper, "+=");
    }
}
