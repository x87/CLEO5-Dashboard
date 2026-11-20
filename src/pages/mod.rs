//! The pages of the sheet, one module per tab.

pub mod audio;
pub mod libraries;
pub mod logs;
pub mod scripts;
pub mod settings;

/// A thread a page asked to open in the debugger: its key, its name and its kind.
pub type DebugRequest = (String, String, crate::threads::Kind);

/// What a page reports back to the frame.
#[derive(Default)]
pub struct Report {
    /// Rows the page's table drew, which is what the footer counts.
    pub shown: usize,
    /// A script the page wants opened in the debugger.
    pub open: Option<DebugRequest>,
}
