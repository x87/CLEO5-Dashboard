//! The colour of one line of log text, shared by the Logs tab and the debugger's trace pane.
//!
//! A line is structure, a message and - while a filter is up - hits. The timestamp
//! `cleo.log` puts in front of every line, the `  |     ` of `cleo_script.log`'s script tree
//! and the `0001234    0001: ` the debugger's trace starts with are structure, so they are
//! dim; a severity word is a warning or an error; a hit is the one thing the user asked to
//! find, so it is the accent; the message is the value colour.
//!
//! The whole thing is a setting the user can turn off ([`crate::settings::highlight`]): an
//! unhighlighted log is a wall of one colour, so an eye that wants that should get it. Hits
//! are painted either way - searching is the point of a filter, and marking what it found
//! is not a style.

use imgui::Ui;

use crate::theme;

/// How many marks one line may be cut into. A one-character needle in a long line would
/// otherwise carve it into hundreds of runs that nobody can tell apart on screen.
const MAX_MARKS: usize = 64;

/// The colours a line can be painted in, resolved once per frame rather than once per line.
pub struct Colors {
    dim: [f32; 4],
    text: [f32; 4],
    accent: [f32; 4],
    warning: [f32; 4],
    error: [f32; 4],
}

impl Colors {
    pub fn theme() -> Self {
        Self {
            dim: theme::dim_color().to_rgba_f32s(),
            text: theme::text_color().to_rgba_f32s(),
            accent: theme::accent_color().to_rgba_f32s(),
            warning: theme::warning_color().to_rgba_f32s(),
            error: theme::error_color().to_rgba_f32s(),
        }
    }
}

/// What a byte range of a line is, which also decides what it wins over.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mark {
    /// Timestamp, tree prefix, opcode field: structure.
    Dim,
    /// A filter hit.
    Hit,
    /// A severity word.
    Warning,
    Error,
}

impl Mark {
    /// Where two marks cover the same byte, the higher one paints it: a severity word is
    /// what the line is about, then the hit that found the line, then the structure either
    /// may sit inside.
    fn priority(self) -> u8 {
        match self {
            Mark::Error | Mark::Warning => 3,
            Mark::Hit => 2,
            Mark::Dim => 1,
        }
    }
}

/// Paints log lines one at a time, reusing its mark buffer between lines.
///
/// A line is marked as byte ranges with a colour, cut into runs at every range boundary and
/// painted run by run, so a hit inside a timestamp still lands on the right bytes. Both
/// buffers are reused for every line of every frame: a log is no place to allocate, and past
/// the first frame the painter does not.
#[derive(Default)]
pub struct Painter {
    marks: Vec<(usize, usize, Mark)>,
    cuts: Vec<usize>,
}

impl Painter {
    /// Draw `line` at the cursor, `needle` marked in the accent colour.
    ///
    /// `syntax` is the setting: off, the line is one run of the message colour and only the
    /// hits stand out.
    pub fn draw(
        &mut self,
        ui: &Ui,
        line: &str,
        needle: Option<&str>,
        colors: &Colors,
        syntax: bool,
    ) {
        self.marks.clear();
        self.cuts.clear();

        // An empty line still has to take a row: `text` is what advances it.
        if line.is_empty() {
            ui.text("");
            return;
        }

        // Structure: a timestamp, a tree prefix or an opcode field. One of the three, never
        // more than one.
        if syntax {
            let mut prefix = timestamp_len(line);
            if prefix == 0 {
                prefix = tree_prefix_len(line);
            }
            if prefix == 0 {
                prefix = debug_prefix_len(line);
            }
            if prefix > 0 {
                self.marks.push((0, prefix, Mark::Dim));
            }
            if let Some((start, end, mark)) = severity_span(line, prefix) {
                self.marks.push((start, end, mark));
            }
            dim_parts(line, prefix, &mut self.marks);
        }

        if let Some(needle) = needle {
            let mut from = 0;
            while let Some(offset) = find_ignore_ascii_case(&line[from..], needle) {
                let start = from + offset;
                self.marks.push((start, start + needle.len(), Mark::Hit));
                from = start + needle.len();
                if self.marks.len() >= MAX_MARKS {
                    break;
                }
            }
        }

        if self.marks.is_empty() {
            ui.text_colored(colors.text, line);
            return;
        }

        self.cuts.push(0);
        self.cuts.push(line.len());
        for index in 0..self.marks.len() {
            let (start, end, _) = self.marks[index];
            self.cuts.push(start);
            self.cuts.push(end);
        }
        self.cuts.sort_unstable();
        self.cuts.dedup();

        let mut first = true;
        for pair in self.cuts.windows(2) {
            let (start, end) = (pair[0], pair[1]);
            if start == end {
                continue;
            }
            if !first {
                // No spacing: the runs are one line of text, not three items in a row.
                ui.same_line_with_spacing(0.0, 0.0);
            }
            ui.text_colored(self.color_of(start, end, colors), &line[start..end]);
            first = false;
        }
    }

    /// The colour of `start..end`: the strongest mark covering it, else the message colour.
    fn color_of(&self, start: usize, end: usize, colors: &Colors) -> [f32; 4] {
        let best = self
            .marks
            .iter()
            .filter(|(mark_start, mark_end, _)| *mark_start <= start && end <= *mark_end)
            .max_by_key(|(_, _, mark)| mark.priority());
        match best.map(|(_, _, mark)| *mark) {
            Some(Mark::Dim) => colors.dim,
            Some(Mark::Hit) => colors.accent,
            Some(Mark::Warning) => colors.warning,
            Some(Mark::Error) => colors.error,
            None => colors.text,
        }
    }
}

/// Byte offset of the first `needle` in `haystack`, ignoring ASCII case.
///
/// Both the filter and its highlights come through here: logs are ASCII, and a log is the
/// last place to spend an allocation per line when the scan runs over thousands of them
/// every frame.
pub fn find_ignore_ascii_case(haystack: &str, needle: &str) -> Option<usize> {
    let (haystack, needle) = (haystack.as_bytes(), needle.as_bytes());
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window.eq_ignore_ascii_case(needle))
}

/// Whether `haystack` contains `needle`, ignoring ASCII case.
pub fn contains(haystack: &str, needle: &str) -> bool {
    find_ignore_ascii_case(haystack, needle).is_some()
}

/// Marks the parts of a rendered instruction that are labels rather than data.
///
/// `cleo_script.log` writes `{11847}` for the offset an instruction ran at, and `{time}`,
/// `{address}`, `{size}` for the arguments that follow - the same label/value split the rest
/// of the menu draws, and what makes a line of opcode text scannable at all. A `// comment`
/// trailer is the renderer talking about the line rather than the line itself, so it is dim
/// too.
fn dim_parts(line: &str, from: usize, marks: &mut Vec<(usize, usize, Mark)>) {
    let bytes = line.as_bytes();
    let mut index = from;
    while index < bytes.len() {
        if marks.len() >= MAX_MARKS {
            return;
        }
        match bytes[index] {
            b'{' => match brace_group(&bytes[index..]) {
                Some(len) => {
                    marks.push((index, index + len, Mark::Dim));
                    index += len;
                }
                None => index += 1,
            },
            // A comment, not the `//` inside `http://`: it has to start the text it is in.
            b'/' if bytes[index..].starts_with(b"//")
                && (index == from || bytes[index - 1] == b' ') =>
            {
                marks.push((index, line.len(), Mark::Dim));
                return;
            }
            _ => index += 1,
        }
    }
}

/// Length of the `{...}` group `bytes` starts with, or `None` when that brace opens
/// something else - a group is short, unquoted, and holds no spaces or braces of its own.
fn brace_group(bytes: &[u8]) -> Option<usize> {
    /// Nothing in a rendered instruction is labelled with more than a word.
    const LIMIT: usize = 32;

    if bytes.first() != Some(&b'{') {
        return None;
    }
    let close = bytes.iter().take(LIMIT).skip(1).position(|&byte| byte == b'}')?;
    // The iterator started at index 1, so its offset is one short.
    let close = close + 1;
    let inner = &bytes[1..close];
    if inner.is_empty()
        || inner
            .iter()
            .any(|&byte| byte.is_ascii_whitespace() || matches!(byte, b'{' | b'}' | b'"' | b'\''))
    {
        return None;
    }
    Some(close + 1)
}

/// Byte length of the `21/11/2026 10:54:02.091` that `cleo.log` puts in front of every
/// line, or 0 when the line does not start with one. The milliseconds are optional.
fn timestamp_len(line: &str) -> usize {
    let bytes = line.as_bytes();
    // `dd/mm/yyyy hh:mm:ss` is nineteen bytes, with the separators at fixed places.
    for (index, separator) in [(2, b'/'), (5, b'/'), (10, b' '), (13, b':'), (16, b':')] {
        if bytes.get(index) != Some(&separator) {
            return 0;
        }
    }
    for index in [0, 1, 3, 4, 6, 7, 8, 9, 11, 12, 14, 15, 17, 18] {
        if !bytes.get(index).is_some_and(u8::is_ascii_digit) {
            return 0;
        }
    }
    let digits = |index: usize| bytes.get(index).is_some_and(u8::is_ascii_digit);
    match bytes.get(19) {
        Some(b'.') if digits(20) && digits(21) && digits(22) => 23,
        _ => 19,
    }
}

/// Byte length of the script tree's `  |     `, ` -|-- `, `  |== ` prefix, or 0 when the
/// line has none.
///
/// The prefix is the leading run of spaces and box characters, and it counts only when the
/// run has a `|`, `-`, `=` or `+` in it - otherwise the plain indentation of an ordinary
/// line would be dimmed along with the message that follows it.
fn tree_prefix_len(line: &str) -> usize {
    let mut end = 0;
    let mut structural = false;
    for (index, byte) in line.bytes().enumerate() {
        match byte {
            b'|' | b'-' | b'=' | b'+' => structural = true,
            b' ' => {}
            _ => return if structural { end } else { 0 },
        }
        end = index + 1;
    }
    if structural {
        end
    } else {
        0
    }
}

/// Byte length of the offset, indentation and opcode fields the debugger's trace puts in
/// front of every line - `0001234    0001: ` - or 0 when the line has none.
///
/// The fields are the log's shape, and dimming them is what leaves the instruction standing
/// out; the arguments that follow keep the message colour, since they are the interesting
/// part of the line.
fn debug_prefix_len(line: &str) -> usize {
    let bytes = line.as_bytes();
    let mut index = 0;
    // The offset, seven digits wide.
    while index < 7 && bytes.get(index).is_some_and(u8::is_ascii_digit) {
        index += 1;
    }
    if index < 7 {
        return 0;
    }
    // Its two spaces, then the indentation of the call chain, then the opcode field.
    if bytes.get(index) != Some(&b' ') || bytes.get(index + 1) != Some(&b' ') {
        return 0;
    }
    index += 2;
    while bytes.get(index) == Some(&b' ') {
        index += 1;
    }
    let opcode = index;
    while index < opcode + 4 && bytes.get(index).is_some_and(u8::is_ascii_hexdigit) {
        index += 1;
    }
    if index != opcode + 4 {
        return 0;
    }
    match (bytes.get(index), bytes.get(index + 1)) {
        (Some(b':'), Some(b' ')) => index + 2,
        _ => 0,
    }
}

/// The first severity word after `from`, as `(start, end, mark)`.
///
/// Matched as a word, case-insensitively, so `ERROR:`, `[Warning]` and the `SHOW_ERROR` tag
/// all land, while `errorless` does not. The tag's prefix comes along with it, since the
/// whole `SHOW_ERROR` is the thing on the line.
fn severity_span(line: &str, from: usize) -> Option<(usize, usize, Mark)> {
    let bytes = line.as_bytes();
    let mut index = from.min(bytes.len());
    while index < bytes.len() {
        if !bytes[index].is_ascii_alphabetic() {
            index += 1;
            continue;
        }
        let start = index;
        while index < bytes.len() && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_') {
            index += 1;
        }
        let word = &line[start..index];
        let tail = word.rsplit('_').next().unwrap_or(word);
        if tail.eq_ignore_ascii_case("error") || tail.eq_ignore_ascii_case("fatal") {
            return Some((start, index, Mark::Error));
        }
        if tail.eq_ignore_ascii_case("warning") {
            return Some((start, index, Mark::Warning));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamps_are_the_prefix_of_a_cleo_log_line() {
        assert_eq!(timestamp_len("21/11/2026 10:54:02.091 Log started."), 23);
        assert_eq!(timestamp_len("21/11/2026 10:54:02 Log started."), 19);
        // A line of the script log starts with a tree, not a date.
        assert_eq!(
            timestamp_len(" -|-- Script 33: 'zz_type' from cleo/zz_types_01.cs"),
            0
        );
        // Something that only looks like a date: one bad byte and it is not one.
        assert_eq!(timestamp_len("21/11/2026 10:54:0x.091"), 0);
        assert_eq!(timestamp_len("21/11/26 10:54:02.091"), 0);
        assert_eq!(timestamp_len(""), 0);
    }

    #[test]
    fn tree_prefixes_are_the_box_run_a_script_log_line_starts_with() {
        assert_eq!(tree_prefix_len("  |     {11847}           wait {time} 1"), 8);
        assert_eq!(tree_prefix_len(" -|-- Executed in 0 ms"), 6);
        assert_eq!(tree_prefix_len("--------------------------------"), 32);
        // Plain prose with no box character in its indentation is not structure.
        assert_eq!(tree_prefix_len("    Commands database version 1.68"), 0);
        assert_eq!(tree_prefix_len("Started on game of version: SA 1.0 us"), 0);
    }

    #[test]
    fn trace_prefixes_are_the_offset_indentation_and_opcode() {
        assert_eq!(debug_prefix_len("0001234    0001: wait {time} 1"), 17);
        // A deeper call chain is more indentation, and still one prefix.
        assert_eq!(debug_prefix_len("0001234        0001: wait {time} 1"), 21);
        // A listing line has no offset field: the pane numbers it itself.
        assert_eq!(debug_prefix_len("    0001: wait {time} 1"), 0);
        assert_eq!(debug_prefix_len("21/11/2026 10:54:02.091 Trace"), 0);
        assert_eq!(debug_prefix_len(""), 0);
    }

    #[test]
    fn severity_words_are_found_as_words() {
        // What the painter does: the scan starts after the structural prefix.
        fn word_of(line: &str) -> Option<(&str, Mark)> {
            let from = timestamp_len(line).max(tree_prefix_len(line).max(debug_prefix_len(line)));
            severity_span(line, from).map(|(start, end, mark)| (&line[start..end], mark))
        }
        fn tag_of(line: &str) -> Option<&str> {
            word_of(line).map(|(word, _)| word)
        }

        // The timestamp is skipped, the message after it is not.
        assert_eq!(
            tag_of("21/11/2026 10:54:02.091 ERROR: could not load"),
            Some("ERROR")
        );
        assert_eq!(tag_of(" ERROR: could not load"), Some("ERROR"));
        assert_eq!(tag_of(" [Warning] a script crashed"), Some("Warning"));
        // The tag's prefix is part of the word: the whole `SHOW_ERROR` is the thing there.
        assert_eq!(tag_of(" SHOW_ERROR: no such file"), Some("SHOW_ERROR"));
        // A word that merely contains the tag is not one.
        assert_eq!(tag_of(" errorless run"), None);
        assert_eq!(tag_of(" no severity here"), None);

        assert!(matches!(word_of(" ERROR: x").unwrap().1, Mark::Error));
        assert!(matches!(word_of(" FATAL: x").unwrap().1, Mark::Error));
        assert!(matches!(word_of(" warning: x").unwrap().1, Mark::Warning));
    }

    #[test]
    fn labels_and_comments_are_the_dimmable_parts_of_an_instruction() {
        fn dims(line: &str) -> Vec<&str> {
            let mut marks = Vec::new();
            dim_parts(line, tree_prefix_len(line), &mut marks);
            marks
                .into_iter()
                .map(|(start, end, _)| &line[start..end])
                .collect()
        }

        assert_eq!(
            dims("  |       {360}       goto_if_false @LABEL_374"),
            ["{360}"]
        );
        assert_eq!(
            dims("  |       {159}       wait {time} 0"),
            ["{159}", "{time}"]
        );
        assert_eq!(
            dims("  |       {374}       17@(0) = 0 // -> 0"),
            ["{374}", "// -> 0"]
        );
        // A brace in a string literal is not a label, and neither is a `//` inside a URL.
        assert_eq!(dims("  |       print {text} '{a b}'"), ["{text}"]);
        assert_eq!(dims("  |       print {text} 'C://x'"), ["{text}"]);
        assert!(dims("  |-- Executed in 0 ms").is_empty());
    }

    #[test]
    fn the_filter_finds_text_whatever_its_case() {
        let line = "21/11/2026 10:54:02.091 CLEO initialization: Phase 1";
        assert_eq!(find_ignore_ascii_case(line, "cleo INIT"), Some(24));
        assert_eq!(find_ignore_ascii_case(line, "phase 1"), Some(45));
        assert_eq!(find_ignore_ascii_case(line, "phase 2"), None);
        assert_eq!(find_ignore_ascii_case(line, ""), None);
        assert_eq!(find_ignore_ascii_case("", "a"), None);
        assert!(contains(line, "Initialization"));
    }
}
