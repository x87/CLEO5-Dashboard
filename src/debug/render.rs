//! Line rendering: a decoded instruction in, one line of text out.
//!
//! The shape follows `cleo_script.log` so a line from this panel can be put next to a line
//! from the log and read the same way:
//!
//! ```text
//! 0000033  0001: wait 0
//! 0000035  00D6: if 0
//! 0000037  8118: not actor $PLAYER_CHAR(1) dead
//! 0000039  004D: jump_if_false @0x00401A2B
//! ```
//!
//! The one deliberate difference is how a variable's value is shown: the log writes
//! `$12(500)`, and so does this, because `$12 = 500` next to a `$2 == 5` comparison reads
//! as two assignments. The per-script state grid, which has no operators in it, does use
//! `@3 = 500`.
//!
//! Types come from the command database, and they decide how the four bytes of a slot are
//! read: an argument the database calls `float` is shown as one, where the bits would
//! otherwise print as an integer nobody can use. An array operand is shown as the element
//! it stands on rather than as the encoding it is made of - `$100[2](1.5)`, not
//! `$100(3@,10)` - and a value of a named type gets the name `enums.json` has for it.

use super::db::{Arg, ArgKind, Command};
use super::decode::{Instruction, Scope, Stop, Value, VarKind};

/// Longest text shown for a string value; the rest is replaced by an ellipsis.
pub const TEXT_LIMIT: usize = 24;

/// How a line is laid out. Everything that is not part of the instruction itself.
pub struct Layout {
    /// Offset of the opcode within the script, as the log counts it.
    pub offset: u32,
    /// The opcode, without the not-flag.
    pub opcode: u16,
    /// Whether the `not` flag was set for this instruction.
    pub not_flag: bool,
    /// Indentation levels: one per `gosub`, one per `cleo_call`, one for a condition that
    /// continues a preceding `ANDOR`.
    pub level: usize,
    /// Leave out the `{argument}` comments, keeping everything else.
    pub compact: bool,
    /// Base of the main script block, so a label can be shown as an address. Zero when
    /// it cannot be read.
    pub scm_block: usize,
    /// Print the offset field, the indentation and the opcode - the log's shape. Off for
    /// a listing line, where the caller numbers the line itself and the opcode would only
    /// be noise.
    pub prefix: bool,
    /// Resolve variable references to their current value. Off for a listing: those lines
    /// are static text, and "the value right now" would be a lie printed beside them.
    pub with_values: bool,
    /// How a jump target is printed.
    pub labels: LabelStyle,
}

/// Jump targets are operands of the instruction, so they can be shown either way: as the
/// address CLEO resolves them to, or as the offset the listing itself numbers its lines by.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LabelStyle {
    /// `@0x00401A2B`: the operand added to the main script block.
    Address,
    /// `12`: the operand as it is encoded, which is the offset of the target. Printed
    Offset,
}

impl Layout {
    /// One indentation level, as wide as the log's.
    pub const INDENT: &'static str = "  ";
}

/// Reads the current value behind a variable reference.
///
/// Implemented for a live script in [`super::LiveVars`]; the renderer itself stays free of
/// game memory so it can be tested against fixtures.
pub trait VarSource {
    /// The 4 bytes of an integer variable.
    fn int(&self, scope: Scope, index: u16) -> Option<i32>;
    /// The same 4 bytes, read as a float: they are whichever the script stored.
    fn float(&self, scope: Scope, index: u16) -> Option<f32>;
    /// The text stored in a text variable.
    fn text(&self, scope: Scope, index: u16, kind: VarKind) -> Option<String>;
}

/// A source with no memory behind it: every value renders as unknown. The renderer's
/// integration tests draw with it when they do not care about values.
#[allow(dead_code)]
pub struct NoVars;

impl VarSource for NoVars {
    fn int(&self, _scope: Scope, _index: u16) -> Option<i32> {
        None
    }

    fn float(&self, _scope: Scope, _index: u16) -> Option<f32> {
        None
    }

    fn text(&self, _scope: Scope, _index: u16, _kind: VarKind) -> Option<String> {
        None
    }
}

/// Render one instruction.
pub fn line(
    command: Option<&Command>,
    instruction: &Instruction,
    layout: &Layout,
    vars: &dyn VarSource,
) -> String {
    let mut text = String::with_capacity(96);

    // Offset field, then the indented body: the log's shape, with the indentation
    // after the offset so a deep call chain does not push the offset off the panel.
    if layout.prefix {
        text.push_str(&format!("{:07}  ", layout.offset));
        for _ in 0..layout.level {
            text.push_str(Layout::INDENT);
        }
        if layout.not_flag {
            text.push_str("not ");
        }
        text.push_str(&format!("{:04X}: ", shown_opcode(layout)));
    } else if layout.not_flag {
        // A listing line has no opcode field to carry the flag, so the word is the
        // only thing that says the condition is negated.
        text.push_str("not ");
    }

    match command {
        Some(command) => render_command(command, instruction, layout, vars, &mut text),
        None => {
            text.push_str(&format!("0x{:04X} (unknown)", layout.opcode));
            for value in &instruction.inputs {
                text.push(' ');
                text.push_str(&render_value(value, None, layout, vars));
            }
        }
    }

    // A stream that stopped early is reported, never guessed past.
    match instruction.stop {
        Stop::Unknown(_) | Stop::Truncated => text.push_str(" ??"),
        Stop::Done | Stop::ArgsEnd => {}
    }

    text
}

fn shown_opcode(layout: &Layout) -> u16 {
    if layout.not_flag {
        layout.opcode | 0x8000
    } else {
        layout.opcode
    }
}

fn render_command(
    command: &Command,
    instruction: &Instruction,
    layout: &Layout,
    vars: &dyn VarSource,
    text: &mut String,
) {
    // Output references first, as `x, y = command ...`, the way the log writes them.
    if !instruction.outputs.is_empty() {
        for (index, value) in instruction.outputs.iter().enumerate() {
            if index > 0 {
                text.push_str(", ");
            }
            text.push_str(&render_value(
                value,
                command.output(index),
                layout,
                vars,
            ));
        }
        text.push_str(" = ");
    }

    if command.oper.is_empty() {
        text.push_str(&command.name);
        for (index, value) in instruction.inputs.iter().enumerate() {
            text.push(' ');
            text.push_str(&render_value(value, command.input(index), layout, vars));
        }
        return;
    }

    // An operator command writes itself as an expression: `0@ = 1`, `$2 == 5`.
    match instruction.inputs.len() {
        0 => text.push_str(&command.name),
        1 => {
            text.push_str(&command.oper);
            text.push(' ');
            text.push_str(&render_value(&instruction.inputs[0], command.input(0), layout, vars));
        }
        _ => {
            text.push_str(&render_value(&instruction.inputs[0], command.input(0), layout, vars));
            text.push(' ');
            text.push_str(&command.oper);
            for (index, value) in instruction.inputs.iter().enumerate().skip(1) {
                text.push(' ');
                text.push_str(&render_value(value, command.input(index), layout, vars));
            }
        }
    }
}

fn render_value(
    value: &Value,
    arg: Option<&Arg>,
    layout: &Layout,
    vars: &dyn VarSource,
) -> String {
    let kind = arg.map(|arg| arg.kind).unwrap_or(ArgKind::Any);

    let rendered = render_typed(value, arg, layout, vars, kind);

    // The argument's name from the command database, in the log's braces. `Compact` is
    // exactly this comment and nothing else: the offsets, the opcode field, the command
    // name and the indentation are the same either way.
    match arg {
        Some(arg) if !layout.compact && !arg.name.is_empty() => {
            format!("{{{}}} {rendered}", arg.name)
        }
        _ => rendered,
    }
}

fn render_typed(
    value: &Value,
    arg: Option<&Arg>,
    layout: &Layout,
    vars: &dyn VarSource,
    kind: ArgKind,
) -> String {
    match value {
        Value::Int(number) if kind == ArgKind::Label => label(*number, layout),

        Value::Int(number) => {
            if kind == ArgKind::ScriptId {
                return format!("0x{:X}", *number as u32);
            }
            // The number stays where the log puts it; a name from `enums.json` is the only
            // thing added. `22 (Colt45)`.
            match arg.and_then(|arg| arg.member(*number)) {
                Some(member) => format!("{number} ({member})"),
                None => number.to_string(),
            }
        }

        Value::Float(number) => format_float(*number),

        Value::Text(text) => quote(text),

        Value::End => "args_end".to_string(),

        Value::Var {
            scope,
            index,
            kind: stored,
        } => {
            let name = var_name(*scope, *index, *stored);
            if !layout.with_values {
                return name;
            }
            match var_value(vars, *scope, *index, *stored, kind) {
                Some(value) => format!("{name}({value})"),
                None => format!("{name}(?)"),
            }
        }

        // The element the operand stands on and what that element holds. The encoding -
        // the index variable's name and the array's size - is left out: it says how the
        // operand is built, not what the script is doing with it.
        Value::Array {
            scope,
            index,
            index_var,
            index_global,
            kind: stored,
            ..
        } => {
            let element = element_kind(*stored, kind);
            let name = var_name(*scope, *index, element);
            if !layout.with_values {
                return format!("{name}[?]");
            }

            let index_scope = if *index_global {
                Scope::Global
            } else {
                Scope::Local
            };
            // Without the index variable's value there is no element to show, only the
            // name of the array it would come from.
            let Some(at) = vars.int(index_scope, *index_var) else {
                return format!("{name}[?]");
            };

            match element_value(vars, *scope, *index, at, element) {
                Some(value) => format!("{name}[{at}]({value})"),
                None => format!("{name}[{at}](?)"),
            }
        }
    }
}

/// A jump target, either the way `CLEO_ThreadJumpAtLabelPtr` resolves it or as the offset
/// the operand already is.
fn label(offset: i32, layout: &Layout) -> String {
    match (layout.labels, offset, layout.scm_block) {
        (LabelStyle::Offset, offset, _) => format!("{offset}"),
        (LabelStyle::Address, offset, base) if offset >= 0 && base != 0 => {
            format!("@0x{:08X}", base as u64 + offset as u64)
        }
        (LabelStyle::Address, offset, _) => format!("@LABEL_{offset}"),
    }
}

/// `$12`, `3@`, `s$4`, `2@s`, `v$7`, `1@v`: a global is a sigil and a name, a local is the
/// index first and the sigil after it, which is the order the builder reads. The type
/// prefix of a global becomes a suffix on a local.
fn var_name(scope: Scope, index: u16, kind: VarKind) -> String {
    let prefix = match kind {
        VarKind::Int | VarKind::Float => "",
        VarKind::TextLabel => "s",
        VarKind::String => "v",
    };

    match scope {
        Scope::Global => format!("{prefix}${index}"),
        Scope::Local => format!("{index}@{prefix}"),
    }
}

/// The value behind a reference, read the way the declared type says to read it.
///
/// The four bytes in a slot are an integer or a float, and the stream does not say which:
/// `sa.json` does, so a `float` argument is shown as one instead of as the bits.
fn var_value(
    vars: &dyn VarSource,
    scope: Scope,
    index: u16,
    stored: VarKind,
    declared: ArgKind,
) -> Option<String> {
    match stored {
        VarKind::Int | VarKind::Float => {
            if declared == ArgKind::ScriptId {
                return vars
                    .int(scope, index)
                    .map(|number| format!("0x{:X}", number as u32));
            }

            if stored == VarKind::Float || declared == ArgKind::Float {
                vars.float(scope, index).map(format_float)
            } else {
                vars.int(scope, index).map(|number| number.to_string())
            }
        }
        VarKind::TextLabel | VarKind::String => {
            vars.text(scope, index, stored).map(|text| quote(&text))
        }
    }
}

/// What one element of an array holds.
///
/// The flags byte says it, unless the command database calls the argument a float: a
/// builder that marks a float array `AT_INT` is not rare, and of the two witnesses the
/// declared type is the one written by hand. Text is decided by the stream alone - the
/// element size follows from it - so a `string` argument never turns one into a number.
fn element_kind(stored: VarKind, declared: ArgKind) -> VarKind {
    match stored {
        VarKind::TextLabel | VarKind::String => stored,
        VarKind::Int | VarKind::Float => {
            if stored == VarKind::Float || declared == ArgKind::Float {
                VarKind::Float
            } else {
                VarKind::Int
            }
        }
    }
}

/// One element's value, read at `base + index * element size` slots.
fn element_value(
    vars: &dyn VarSource,
    scope: Scope,
    base: u16,
    at: i32,
    kind: VarKind,
) -> Option<String> {
    // A negative index, one that runs off the end of the space, or one large enough to
    // wrap says nothing about the element: it is reported as unknown rather than read
    // somewhere else.
    let at = usize::try_from(at).ok()?;
    let slot = at
        .checked_mul(kind.slots())?
        .checked_add(usize::from(base))?;
    let slot = u16::try_from(slot).ok()?;

    match kind {
        VarKind::Int => vars.int(scope, slot).map(|number| number.to_string()),
        VarKind::Float => vars.float(scope, slot).map(format_float),
        VarKind::TextLabel | VarKind::String => {
            vars.text(scope, slot, kind).map(|text| quote(&text))
        }
    }
}

fn quote(text: &str) -> String {
    format!("\"{}\"", ellipsis(text, TEXT_LIMIT))
}

/// Cut `text` to `limit` characters, marking the cut.
pub fn ellipsis(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }

    let mut cut: String = text.chars().take(limit.saturating_sub(1)).collect();
    cut.push('\u{2026}');
    cut
}

/// `1.5`, `1.0`, `0.25`: never in exponent form, always recognisable as a float.
pub fn format_float(number: f32) -> String {
    if !number.is_finite() {
        return if number.is_nan() {
            "NAN".to_string()
        } else if number > 0.0 {
            "INF".to_string()
        } else {
            "-INF".to_string()
        };
    }

    let mut text = format!("{number:.4}");
    while text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.push('0');
    }
    text
}

#[cfg(test)]
mod tests {
    use super::{line, LabelStyle, Layout, NoVars};
    use super::super::db::{Arg, ArgKind, Command};
    use super::super::decode::{Instruction, Scope, Stop, Value, VarKind};

    fn arg(kind: ArgKind) -> Arg {
        Arg {
            name: String::new(),
            kind,
            enum_type: None,
        }
    }

    fn command(name: &str, oper: &str, inputs: Vec<Arg>) -> Command {
        Command {
            name: name.to_string(),
            oper: oper.to_string(),
            inputs,
            varargs_at: None,
            outputs: Vec::new(),
        }
    }

    fn listing(opcode: u16) -> Layout {
        Layout {
            offset: 0,
            opcode,
            not_flag: false,
            level: 0,
            compact: true,
            scm_block: 0,
            prefix: false,
            with_values: false,
            labels: LabelStyle::Offset,
        }
    }

    fn local(index: u16) -> Value {
        Value::Var {
            scope: Scope::Local,
            index,
            kind: VarKind::Int,
        }
    }

    fn instruction(inputs: Vec<Value>) -> Instruction {
        Instruction {
            inputs,
            outputs: Vec::new(),
            consumed: 0,
            stop: Stop::Done,
        }
    }

    /// The operator is printed once, and only once: `12@ *= 8`, not `12@ **= 8`.
    #[test]
    fn an_in_place_operator_is_printed_once() {
        let command = command(
            "mult_int_lvar_by_val",
            "*=",
            vec![arg(ArgKind::Int), arg(ArgKind::Int)],
        );
        let text = line(
            Some(&command),
            &instruction(vec![local(12), Value::Int(8)]),
            &listing(0x0012),
            &NoVars,
        );
        assert_eq!(text, "12@ *= 8");
    }

    /// A command whose operator the database leaves empty writes its own name: the `=`
    /// that a jump line once carried made it read as an assignment with no output.
    #[test]
    fn a_command_without_an_operator_keeps_its_name() {
        let command = command("goto_if_false", "", vec![arg(ArgKind::Label)]);
        let text = line(
            Some(&command),
            &instruction(vec![Value::Int(-189)]),
            &listing(0x004D),
            &NoVars,
        );
        assert_eq!(text, "goto_if_false -189");
    }

    /// An operator with an output is an expression: the output is the assignment.
    #[test]
    fn an_output_carries_the_assignment() {
        let mut command = command(
            "bit_xor",
            "^",
            vec![arg(ArgKind::Int), arg(ArgKind::Int)],
        );
        command.outputs = vec![arg(ArgKind::Int)];
        let mut instruction = instruction(vec![local(1), local(3)]);
        instruction.outputs = vec![local(2)];
        let text = line(Some(&command), &instruction, &listing(0x0B12), &NoVars);
        assert_eq!(text, "2@ = 1@ ^ 3@");
    }
}
