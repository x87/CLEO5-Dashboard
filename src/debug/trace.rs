//! The per-script ring of executed instructions.
//!
//! Lines are appended by the opcode callback, which runs on the game thread once per
//! instruction, so everything here is allocation-light and bounded: the ring keeps the
//! newest `cap` lines and counts the ones it dropped instead of growing without limit. A
//! gap is shown as `... N earlier lines`, never invented.

use std::collections::VecDeque;

/// Default number of lines kept per script.
pub const DEFAULT_CAPACITY: usize = 2000;
/// Bounds of the line cap the panel offers.
pub const MIN_CAPACITY: usize = 100;
pub const MAX_CAPACITY: usize = 20_000;

/// One executed instruction.
#[derive(Clone)]
pub struct Line {
    /// Renderer output with names and indentation.
    pub text: String,
    /// The same line in compact form, kept so the toggle does not have to re-render.
    pub compact: String,
}

pub struct Ring {
    lines: VecDeque<Line>,
    capacity: usize,
    /// Lines dropped off the front since the last [`clear`](Self::clear).
    dropped: usize,
}

impl Ring {
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.clamp(MIN_CAPACITY, MAX_CAPACITY);
        Self {
            lines: VecDeque::with_capacity(capacity.min(256)),
            capacity,
            dropped: 0,
        }
    }

    pub fn push(&mut self, line: Line) {
        while self.lines.len() >= self.capacity {
            self.lines.pop_front();
            self.dropped += 1;
        }
        self.lines.push_back(line);
    }

    /// Change the cap, dropping the oldest lines to fit.
    ///
    /// Nothing in the panel calls this: the cap is [`crate::debug::DEFAULT_LINE_CAP`],
    /// taken when the ring is made. Kept because it is the only way to exercise the
    /// drop-oldest path, which the tests do.
    #[allow(dead_code)]
    pub fn set_capacity(&mut self, capacity: usize) {
        self.capacity = capacity.clamp(MIN_CAPACITY, MAX_CAPACITY);
        while self.lines.len() > self.capacity {
            self.lines.pop_front();
            self.dropped += 1;
        }
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    /// The most lines this ring keeps: what the panel counts against.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// How many lines fell off the front, i.e. how many ran before the oldest one kept.
    pub fn dropped(&self) -> usize {
        self.dropped
    }

    pub fn iter(&self) -> std::collections::vec_deque::Iter<'_, Line> {
        self.lines.iter()
    }

    pub fn clear(&mut self) {
        self.lines.clear();
        self.dropped = 0;
    }
}
