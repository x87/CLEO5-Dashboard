//! Operand decoding: a type byte in, a value and a width out.
//!
//! The widths are CLEO's own, copied from `CLEO_SkipOpcodeParams`
//! (`source/exports.cpp`), not re-derived: a divergence there would show up as a line
//! rendered from the wrong bytes, which is exactly what a debug view must not do.
//!
//! Decoding never touches the thread's instruction pointer. It walks a *copy* of the
//! cursor over the script's bytes, because the opcode handler that runs next has to see
//! the stream untouched.

/// Operand type bytes, from `CLEO::eDataType`.
pub const DT_END: u8 = 0;
pub const DT_DWORD: u8 = 1;
pub const DT_VAR: u8 = 2;
pub const DT_LVAR: u8 = 3;
pub const DT_BYTE: u8 = 4;
pub const DT_WORD: u8 = 5;
pub const DT_FLOAT: u8 = 6;
pub const DT_VAR_ARRAY: u8 = 7;
pub const DT_LVAR_ARRAY: u8 = 8;
pub const DT_TEXTLABEL: u8 = 9;
pub const DT_VAR_TEXTLABEL: u8 = 10;
pub const DT_LVAR_TEXTLABEL: u8 = 11;
pub const DT_VAR_TEXTLABEL_ARRAY: u8 = 12;
pub const DT_LVAR_TEXTLABEL_ARRAY: u8 = 13;
pub const DT_VARLEN_STRING: u8 = 14;
pub const DT_STRING: u8 = 15;
pub const DT_VAR_STRING: u8 = 16;
pub const DT_LVAR_STRING: u8 = 17;
pub const DT_VAR_STRING_ARRAY: u8 = 18;
pub const DT_LVAR_STRING_ARRAY: u8 = 19;

/// `opcodeParams` holds 32 values, so no instruction can carry more than that.
pub const MAX_VALUES: usize = 32;

/// Array flags byte: the index is a global variable, stored as a byte offset.
const ATF_INDEX_GLOBAL: u8 = 0x80;
/// Array flags byte: mask for the element type.
const ARRAY_TYPE_MASK: u8 = 0x03;
/// Array flags byte: element type `AT_FLOAT`. `AT_INT` is zero.
const AT_FLOAT: u8 = 0x01;

/// Which of the two variable spaces a reference points into.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Scope {
    Global,
    Local,
}

/// How many variable slots the value of a reference occupies.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum VarKind {
    /// One slot: an integer, or a float the script treats as a float.
    Int,
    /// One slot read as a float. Only an array's flags byte says this; a plain reference
    /// carries nothing but four bytes, and what they mean is the command database's say.
    Float,
    /// Two slots: a label of up to 7 characters.
    TextLabel,
    /// Four slots: a string of up to 15 characters.
    String,
}

impl VarKind {
    pub fn slots(self) -> usize {
        match self {
            VarKind::Int | VarKind::Float => 1,
            VarKind::TextLabel => 2,
            VarKind::String => 4,
        }
    }
}

/// One decoded operand.
#[derive(Clone, PartialEq, Debug)]
pub enum Value {
    Int(i32),
    Float(f32),
    /// An immediate string: `DT_TEXTLABEL`, `DT_STRING` or `DT_VARLEN_STRING`.
    Text(String),
    /// A reference to a variable, for the renderer to name and (when it can) resolve.
    Var {
        scope: Scope,
        index: u16,
        kind: VarKind,
    },
    /// A reference to one element of an array.
    Array {
        scope: Scope,
        index: u16,
        /// The variable holding the element index, already divided by four when the
        /// script stored a global's byte offset in it.
        index_var: u16,
        index_global: bool,
        size: u8,
        kind: VarKind,
    },
    /// The `DT_END` that closes a vararg list.
    End,
}

/// Why decoding stopped.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stop {
    /// The instruction asked for exactly as many values as it has.
    Done,
    /// The vararg list's terminator (`DT_END`), consumed with the rest.
    ArgsEnd,
    /// A type byte no `eDataType` defines. Never guessed at.
    Unknown(u8),
    /// The next operand runs past the end of the script.
    Truncated,
}

/// How many values one instruction carries, from the command database.
#[derive(Clone, Copy, Debug)]
pub struct Plan {
    /// Declared input arguments. A vararg command counts its vararg slot as one.
    pub inputs: usize,
    /// Index of the vararg slot within the inputs, if any.
    pub varargs_at: Option<usize>,
    /// Declared output arguments, which follow the inputs in the same stream.
    pub outputs: usize,
}

/// One decoded instruction: its inputs, the outputs that follow them, and the bytes it
/// occupied after the 2-byte opcode.
#[derive(Clone, Debug)]
pub struct Instruction {
    pub inputs: Vec<Value>,
    pub outputs: Vec<Value>,
    /// Bytes consumed, including a vararg terminator when one was read.
    ///
    /// The panel does not need it - it only looks at [`stop`](Self::stop) - but the
    /// decoder's integration tests assert on it.
    #[allow(dead_code)]
    pub consumed: usize,
    pub stop: Stop,
}

impl Instruction {
    /// True when the whole instruction was decoded and nothing was left dangling.
    #[allow(dead_code)] // the decoder's integration tests use it
    pub fn is_complete(&self) -> bool {
        matches!(self.stop, Stop::Done | Stop::ArgsEnd)
    }
}

/// Decode the instruction at `bytes` (which starts right after the opcode).
///
/// `plan` is the command's declared argument layout; without it (an opcode the database
/// does not know) every value up to `DT_END`, an unknown type byte or the end of the
/// script is returned as an input.
pub fn decode(bytes: &[u8], plan: Option<Plan>) -> Instruction {
    let mut cursor = Cursor {
        bytes,
        pos: 0,
        stop: Stop::Done,
    };
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    let mut stop = Stop::Done;

    match plan {
        // An opcode the database does not know: every value up to the terminator is an
        // input, and the terminator itself is part of the instruction.
        None => loop {
            if inputs.len() >= MAX_VALUES {
                break;
            }
            match cursor.peek() {
                // The script ends exactly here: nothing is missing, there is simply
                // nothing left to read, and an unknown arity cannot ask for more.
                None => break,
                Some(DT_END) => {
                    cursor.pos += 1;
                    stop = Stop::ArgsEnd;
                    break;
                }
                _ => match cursor.value() {
                    Some(value) => inputs.push(value),
                    None => {
                        stop = cursor.stop;
                        break;
                    }
                },
            }
        },

        Some(plan) => 'inputs: for index in 0..plan.inputs.min(MAX_VALUES) {
            let varargs = plan.varargs_at == Some(index);
            loop {
                // A vararg list runs to its `DT_END`, which belongs to the instruction.
                if varargs {
                    match cursor.peek() {
                        Some(DT_END) => {
                            cursor.pos += 1;
                            stop = Stop::ArgsEnd;
                            break 'inputs;
                        }
                        None => {
                            stop = Stop::Truncated;
                            break 'inputs;
                        }
                        _ => {}
                    }
                }

                if inputs.len() >= MAX_VALUES {
                    break 'inputs;
                }

                match cursor.value() {
                    // A terminator where a plain argument was declared: the command lies
                    // about its arity, so stop at the byte CLEO would stop at.
                    Some(Value::End) => {
                        stop = Stop::ArgsEnd;
                        break 'inputs;
                    }
                    Some(value) => inputs.push(value),
                    None => {
                        stop = cursor.stop;
                        break 'inputs;
                    }
                }

                if !varargs {
                    break;
                }
            }
        },
    }

    // Outputs sit after the inputs, and are only worth reading when the inputs landed
    // where the database said they would.
    if stop == Stop::Done {
        let wanted = plan.map(|plan| plan.outputs).unwrap_or(0).min(MAX_VALUES);
        for _ in 0..wanted {
            match cursor.value() {
                Some(Value::End) => {
                    stop = Stop::ArgsEnd;
                    break;
                }
                Some(value) => outputs.push(value),
                None => {
                    stop = cursor.stop;
                    break;
                }
            }
        }
    }

    Instruction {
        inputs,
        outputs,
        consumed: cursor.pos,
        stop,
    }
}

struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
    stop: Stop,
}

impl Cursor<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    /// `len` bytes, or `None` with [`Cursor::stop`] set to `Truncated`.
    fn take(&mut self, len: usize) -> Option<&[u8]> {
        let end = self.pos.checked_add(len)?;
        if end > self.bytes.len() {
            self.stop = Stop::Truncated;
            return None;
        }

        let field = &self.bytes[self.pos..end];
        self.pos = end;
        Some(field)
    }

    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|field| field[0])
    }

    /// Little-endian 16-bit operand.
    fn u16(&mut self) -> Option<u16> {
        self.take(2).map(|field| u16::from_le_bytes([field[0], field[1]]))
    }

    fn u32(&mut self) -> Option<u32> {
        self.take(4).map(|field| u32::from_le_bytes([field[0], field[1], field[2], field[3]]))
    }

    /// Fixed-size text, cut at the first NUL.
    fn text(&mut self, len: usize) -> Option<String> {
        let field = self.take(len)?;
        let used = field
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(field.len());
        Some(String::from_utf8_lossy(&field[..used]).into_owned())
    }

    /// One operand, or `None` with [`Cursor::stop`] set.
    fn value(&mut self) -> Option<Value> {
        let type_byte = self.u8()?;

        macro_rules! or_stop {
            ($expr:expr) => {
                match $expr {
                    Some(value) => value,
                    None => {
                        if self.stop == Stop::Done {
                            self.stop = Stop::Truncated;
                        }
                        return None;
                    }
                }
            };
        }

        let value = match type_byte {
            DT_END => Value::End,
            DT_DWORD => Value::Int(or_stop!(self.u32()) as i32),
            DT_BYTE => Value::Int(or_stop!(self.u8()) as i8 as i32),
            DT_WORD => Value::Int(or_stop!(self.u16()) as i16 as i32),
            DT_FLOAT => Value::Float(f32::from_bits(or_stop!(self.u32()))),

            DT_VAR => Value::Var {
                scope: Scope::Global,
                index: or_stop!(self.u16()),
                kind: VarKind::Int,
            },
            DT_LVAR => Value::Var {
                scope: Scope::Local,
                index: or_stop!(self.u16()),
                kind: VarKind::Int,
            },
            DT_VAR_TEXTLABEL => Value::Var {
                scope: Scope::Global,
                index: or_stop!(self.u16()),
                kind: VarKind::TextLabel,
            },
            DT_LVAR_TEXTLABEL => Value::Var {
                scope: Scope::Local,
                index: or_stop!(self.u16()),
                kind: VarKind::TextLabel,
            },
            DT_VAR_STRING => Value::Var {
                scope: Scope::Global,
                index: or_stop!(self.u16()),
                kind: VarKind::String,
            },
            DT_LVAR_STRING => Value::Var {
                scope: Scope::Local,
                index: or_stop!(self.u16()),
                kind: VarKind::String,
            },

            DT_TEXTLABEL => Value::Text(or_stop!(self.text(8))),
            DT_STRING => Value::Text(or_stop!(self.text(16))),
            DT_VARLEN_STRING => {
                // The length byte is *unsigned*; CLEO's skipper relies on that too.
                let length = or_stop!(self.u8()) as usize;
                Value::Text(or_stop!(self.text(length)))
            }

            DT_VAR_ARRAY
            | DT_LVAR_ARRAY
            | DT_VAR_TEXTLABEL_ARRAY
            | DT_LVAR_TEXTLABEL_ARRAY
            | DT_VAR_STRING_ARRAY
            | DT_LVAR_STRING_ARRAY => {
                // Six bytes: variable index, index variable, size, flags.
                let index = or_stop!(self.u16());
                let mut index_var = or_stop!(self.u16());
                let size = or_stop!(self.u8());
                let flags = or_stop!(self.u8());

                let index_global = flags & ATF_INDEX_GLOBAL != 0;
                if index_global {
                    // Stored as a byte offset into the global space.
                    index_var /= 4;
                }

                let kind = match flags & ARRAY_TYPE_MASK {
                    // `AT_INT` and `AT_FLOAT` are one slot each, but which of the two the
                    // slot holds is exactly what the flags byte is there to say.
                    AT_FLOAT => VarKind::Float,
                    2 => VarKind::TextLabel,
                    3 => VarKind::String,
                    _ => VarKind::Int,
                };

                Value::Array {
                    scope: if matches!(
                        type_byte,
                        DT_VAR_ARRAY | DT_VAR_TEXTLABEL_ARRAY | DT_VAR_STRING_ARRAY
                    ) {
                        Scope::Global
                    } else {
                        Scope::Local
                    },
                    index,
                    index_var,
                    index_global,
                    size,
                    kind,
                }
            }

            other => {
                self.stop = Stop::Unknown(other);
                return None;
            }
        };

        Some(value)
    }
}
