//! The script as a static listing: every instruction from `BaseIP` to the end, in stream
//! order, with the one about to run marked.
//!
//! Walking the bytes is the same decode the trace uses, run over the whole block instead of
//! one instruction at a time. It is exactly the order CLEO executes in for straight-line
//! code; a jump only changes which line the marker sits on, not the listing.
//!
//! The listing is built once per script and kept with the panel's state: a script does not
//! change while it runs, so re-walking it every frame would be pure waste.

use super::db::{Command, Db};
use super::decode::{self, Plan};
use super::render::{self, LabelStyle, Layout, NoVars};

/// Upper bound on listing lines, so a giant script cannot eat the frame.
pub const MAX_LINES: usize = 20_000;

/// One instruction in the listing.
pub struct Entry {
    /// Offset of the opcode within the script, which is also what a jump operand holds.
    pub offset: u32,
    /// Bytes this instruction occupies, opcode included.
    pub length: usize,
    /// The line, without an offset field: `not actor_dead $2`.
    pub text: String,
    /// The same instruction with the command name, the argument comments and the
    /// indentation gone: `0001: 1`. What the toolbar's `Compact` switch shows.
    pub compact: String,
}

pub struct Listing {
    pub entries: Vec<Entry>,
    /// Set when the walk had to stop early: a type byte no `eDataType` defines, or a stream
    /// that ran out mid-operand.
    pub stop: Option<decode::Stop>,
    /// Set when [`MAX_LINES`] cut the listing short.
    pub capped: bool,
    /// The longest line, counted in characters. The font is monospaced, so this is enough
    /// to size the pane for the widest line without measuring every string on every frame -
    /// which is what a script printing a long string literal needs.
    pub widest: usize,
}

impl Listing {
    /// Index of the instruction whose bytes contain `offset`, i.e. the line the script is
    /// standing on.
    pub fn index_at(&self, offset: u32) -> Option<usize> {
        let index = self
            .entries
            .partition_point(|entry| entry.offset <= offset);
        index.checked_sub(1).filter(|index| {
            let entry = &self.entries[*index];
            offset < entry.offset + entry.length as u32
        })
    }

    /// Number of instructions the walk decoded.
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

/// Decode the whole script block into a listing.
///
/// `first` is the offset `bytes[0]` has within the script, so the numbers in the gutter stay
/// the offsets a jump operand names. It is zero for everything the engine records a start
/// for, and the thread's standing offset when it does not.
pub fn build(bytes: &[u8], db: Option<&Db>, scm_block: usize, first: u32) -> Listing {
    let mut entries = Vec::new();
    let mut pos = 0usize;
    let mut stop = None;
    let mut capped = false;
    let mut widest = 0usize;

    while pos + 2 <= bytes.len() {
        if entries.len() >= MAX_LINES {
            capped = true;
            break;
        }

        // The negated-condition bit lives in the opcode word itself; CLEO strips it before
        // the handler runs, so the listing has to strip it before looking the command up.
        let raw = u16::from_le_bytes([bytes[pos], bytes[pos + 1]]);
        let not_flag = raw & 0x8000 != 0;
        let opcode = raw & 0x7FFF;

        let command = db.and_then(|db| db.get(opcode));
        let args = &bytes[pos + 2..];
        let instruction = decode::decode(args, plan_for(command));

        let layout = Layout {
            offset: first + pos as u32,
            opcode,
            not_flag,
            // Static text: the call depth of a line is only known while it runs.
            level: 0,
            compact: false,
            scm_block,
            // The caller numbers the lines and prints no opcode.
            prefix: false,
            with_values: false,
            labels: LabelStyle::Offset,
        };
        let text = render::line(command, &instruction, &layout, &NoVars);
        // Compact is the body only: the pane numbers the line in the same column either
        // way, so the offset is not repeated inside it.
        let short_form = render::line(
            command,
            &instruction,
            &Layout {
                compact: true,
                ..layout
            },
            &NoVars,
        );
        let length = 2 + instruction.consumed;

        widest = widest
            .max(text.chars().count())
            .max(short_form.chars().count());
        entries.push(Entry {
            offset: first + pos as u32,
            length,
            text,
            compact: short_form,
        });

        if !instruction.is_complete() {
            stop = Some(instruction.stop);
            break;
        }

        // `consumed` is zero only for an instruction whose whole payload is the opcode.
        pos += length.max(2);
    }

    Listing {
        entries,
        stop,
        capped,
        widest,
    }
}

fn plan_for(command: Option<&Command>) -> Option<Plan> {
    command.map(|command| Plan {
        inputs: command.inputs.len(),
        varargs_at: command.varargs_at,
        outputs: command.outputs.len(),
    })
}
