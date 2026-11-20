//! The Libraries page: every module the process has loaded, with the ones CLEO loaded for a
//! script marked and counted.

use hudhook::windows::Win32::Foundation::HMODULE;
use imgui::Ui;

use crate::modules;
use crate::nav::{self, Intent};
use crate::plugins;
use crate::strings;
use crate::table::{self, Column};
use crate::theme::{self, Role};
use crate::utils::{elide_path, hex_address};

use super::Report;

/// The filter chips above the table. A module is what its extension says it is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Chip {
    All,
    Cleo,
    Dll,
    Asi,
}

impl Chip {
    const ALL: [(Chip, &'static str); 4] = [
        (Chip::All, strings::libraries::CHIP_ALL),
        (Chip::Cleo, strings::libraries::CHIP_CLEO),
        (Chip::Dll, strings::libraries::CHIP_DLL),
        (Chip::Asi, strings::libraries::CHIP_ASI),
    ];

    fn from_index(index: usize) -> Chip {
        Self::ALL
            .get(index)
            .map(|(chip, _)| *chip)
            .unwrap_or(Chip::All)
    }

    fn tip(self) -> Option<&'static str> {
        match self {
            Chip::All => None,
            Chip::Cleo => Some(strings::libraries::TIP_CHIP_CLEO),
            Chip::Dll => Some(strings::libraries::TIP_CHIP_DLL),
            Chip::Asi => Some(strings::libraries::TIP_CHIP_ASI),
        }
    }

    fn keeps(self, module: &modules::Module) -> bool {
        match self {
            Chip::All => true,
            Chip::Cleo => module.kind == modules::Kind::Cleo,
            Chip::Dll => module.kind == modules::Kind::Dll,
            Chip::Asi => module.kind == modules::Kind::Asi,
        }
    }
}

#[derive(Default)]
pub struct State {
    /// Index into [`Chip::ALL`].
    pub chip: usize,
    /// Text in the filter box, matched against the module path.
    pub filter: String,
    /// Whether modules Windows itself loads are listed too. Off by default: `System32` is
    /// most of the process and none of what a modder is looking for.
    pub system: bool,
}

pub fn page(
    ui: &Ui,
    state: &mut State,
    nav_row: &mut Option<usize>,
    intent: Intent,
    page_rows: i32,
) -> Report {
    if let Some(chip) = intent.chip {
        state.chip = chip.min(Chip::ALL.len() - 1);
    }
    toolbar(ui, state, intent.filter);

    let needle = state.filter.trim().to_lowercase();
    let chip = Chip::from_index(state.chip);
    let show_system = state.system;

    // A snapshot of the module list, re-taken once a second; the counts come straight from
    // the plugin, so a listed module never shows a stale number.
    let loaded_modules = modules::loaded();
    let loaded = cleo_libraries();

    let rows: Vec<&modules::Module> = loaded_modules
        .iter()
        .filter(|module| chip.keeps(module))
        .filter(|module| show_system || !module.system)
        .filter(|module| needle.is_empty() || module.path_lower.contains(needle.as_str()))
        .collect();

    let cursor = nav::moved_row(*nav_row, intent.row, rows.len(), page_rows);
    *nav_row = cursor;
    let report = Report {
        shown: rows.len(),
        ..Default::default()
    };

    theme::bare_child(ui, "libraries_tab_inner", || {
        // A failed walk leaves the list empty; that is not the same as a chip that matches
        // nothing, so say which one it is.
        if loaded_modules.is_empty() {
            ui.text_colored(Role::Dim.rgba(1.0), strings::libraries::UNAVAILABLE);
            return;
        }
        if rows.is_empty() {
            ui.text_colored(Role::Dim.rgba(1.0), strings::libraries::EMPTY);
            return;
        }

        table_body(
            ui,
            &rows,
            &loaded,
            ui.content_region_avail()[1],
            cursor,
            intent.row.is_some(),
        );
    });

    report
}

/// How many filter chips the page has, for the footer's key hint.
pub const CHIP_COUNT: usize = Chip::ALL.len();
fn toolbar(ui: &Ui, state: &mut State, focus: bool) {
    let room = ui.content_region_avail()[0];
    let origin = ui.cursor_screen_pos();
    let search_width = (room * 0.30).clamp(200.0, 340.0);

    table::search_box(
        ui,
        "##_library_filter",
        strings::libraries::FILTER_PLACEHOLDER,
        &mut state.filter,
        focus,
        search_width,
    );
    theme::chips(ui, &Chip::ALL, Chip::tip, &mut state.chip);

    // The switch sits on the right edge of the row, if the chips left room for it.
    let label = strings::libraries::SHOW_SYSTEM;
    let gap = ui.clone_style().item_spacing[0];
    let pill = ((ui.text_line_height() + 1.0).round() * 1.7).round();
    let block = ui.calc_text_size(label)[0] + gap + pill;

    if let Some(offset) = table::right_slot(ui, origin[0], room, block) {
        ui.same_line_with_spacing(offset, 0.0);
        ui.align_text_to_frame_padding();
        ui.text_colored(Role::Dim.rgba(1.0), label);
        ui.same_line();
        theme::switch(ui, "##_library_system", &mut state.system);
        theme::hint(ui, strings::libraries::TIP_SHOW_SYSTEM);
    }
}

/// The libraries CLEO loaded for a script, by loaded-image base, with how many times a script
/// asked for each. Empty when `SA.MemoryOperations.cleo` is not loaded.
fn cleo_libraries() -> std::collections::HashMap<usize, usize> {
    let mut libraries = std::collections::HashMap::new();
    let Some(memory) = plugins::memory() else {
        return libraries;
    };

    let num_libs = unsafe { (memory.number_of_libraries)() };
    for i in 0..num_libs {
        let mut handle = HMODULE(std::ptr::null_mut());
        let mut count = 0;
        if unsafe { (memory.library_by_index)(i, &mut handle, &mut count) } {
            libraries.insert(handle.0 as usize, count);
        }
    }

    libraries
}

fn table_body(
    ui: &Ui,
    rows: &[&modules::Module],
    loaded: &std::collections::HashMap<usize, usize>,
    height: f32,
    cursor: Option<usize>,
    moved: bool,
) {
    let spacing = ui.clone_style().item_spacing[0];
    let width = |text: &str| ui.calc_text_size(text)[0];

    let index_col = (width("000") + spacing * 2.0).max(28.0);
    let count_col = rows
        .iter()
        .fold(width(strings::libraries::HEADER_LOADS), |widest, module| {
            match loaded.get(&module.base) {
                Some(count) => widest.max(width(&count.to_string())),
                None => widest.max(width(strings::libraries::NO_LOADS)),
            }
        })
        + spacing * 2.0;
    let base_col = rows
        .iter()
        .fold(width(strings::libraries::HEADER_BASE), |widest, module| {
            widest.max(width(&hex_address(module.base)))
        })
        + spacing * 2.0;

    let columns = [
        Column::fixed(strings::table::HEADER_INDEX, index_col),
        Column::stretch(strings::libraries::HEADER_MODULE),
        Column::fixed(strings::libraries::HEADER_LOADS, count_col),
        Column::fixed(strings::libraries::HEADER_BASE, base_col).trailing(),
    ];
    let captions = [
        (strings::table::HEADER_INDEX, ""),
        (strings::libraries::HEADER_MODULE, ""),
        (strings::libraries::HEADER_LOADS, ""),
        (strings::libraries::HEADER_BASE, ""),
    ];
    let table_height = table::body_height(ui, rows.len(), 0.0, height);

    table::table(ui, "libraries_table", &columns, table_height, |ui| {
        table::header(ui, &captions);
        let header_bottom = ui.item_rect_max()[1];

        for (i, module) in rows.iter().enumerate() {
            let row_top = table::body_row(ui, ui.frame_height(), cursor == Some(i));

            ui.table_next_column();
            let number = (i + 1).to_string();
            table::cell_centered(ui, &format!("##_libidx{i}"), &number, Role::Text);

            ui.table_next_column();
            let shown = elide_path(ui, &module.path, table::text_room(ui));
            // The modules CLEO loaded for a script are the ones with a count, so they are
            // also the ones the eye is looking for in a table this long.
            let role = if loaded.contains_key(&module.base) {
                Role::Success
            } else {
                Role::Text
            };
            table::cell(ui, &format!("##_lib{i}"), &shown, role, &module.path);

            ui.table_next_column();
            let count = match loaded.get(&module.base) {
                Some(count) => count.to_string(),
                None => strings::libraries::NO_LOADS.to_string(),
            };
            table::cell(ui, &format!("##_libcount{i}"), &count, Role::Text, "");

            ui.table_next_column();
            table::cell(
                ui,
                &format!("##_libbase{i}"),
                &hex_address(module.base),
                Role::Text,
                "",
            );

            if moved && cursor == Some(i) {
                table::reveal_row(ui, row_top, ui.frame_height(), header_bottom);
            }
        }
    });
}
