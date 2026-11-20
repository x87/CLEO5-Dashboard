//! The Scripts page: CLEO's scripts and the game's own in one table, with the filters above
//! it and the info panel beside it.

pub mod sidebar;

use std::collections::HashMap;

use imgui::Ui;

use crate::icons::Icon;
use crate::nav::{self, Intent};
use crate::plugins;
use crate::settings::{self, ScriptColumn};
use crate::sdk::{CLEO_GetScriptSize, CLEO_GetScriptVersion, CLEO_SetScriptVersion, CRunningScript, CLEO_VER_3, CLEO_VER_4, CLEO_VER_5};
use crate::strings::{self, pretty_size, pct};
use crate::table::{self, Column};
use crate::theme::{self, Role};
use crate::threads;
use crate::utils::{elide_end, elide_path};

use super::Report;

/// The compat modes the Mode column can pin a script to, newest first. The SDK has finer
/// steps (4.0 / 4.2 / 4.3 / 4.4); they all behave as "CLEO 4" here.
const COMPAT_LEVELS: [(u32, &str); 3] = [
    (CLEO_VER_5, strings::scripts::COMPAT_5),
    (CLEO_VER_4, strings::scripts::COMPAT_4),
    (CLEO_VER_3, strings::scripts::COMPAT_3),
];

/// The entry a script's version belongs to.
fn compat_bucket(version: u32) -> u32 {
    if version >= CLEO_VER_5 {
        CLEO_VER_5
    } else if version >= CLEO_VER_4 {
        CLEO_VER_4
    } else {
        CLEO_VER_3
    }
}

fn compat_label(version: u32) -> &'static str {
    let bucket = compat_bucket(version);
    COMPAT_LEVELS
        .iter()
        .find(|(value, _)| *value == bucket)
        .map(|(_, label)| *label)
        .unwrap_or(strings::scripts::COMPAT_UNKNOWN)
}

/// Everything the Scripts page keeps between frames.
#[derive(Default)]
pub struct State {
    /// Text in the filter box. A buffer rebuilt every frame would always be empty.
    pub filter: String,
    /// Index into [`Chip::ALL`].
    pub chip: usize,
    /// Key of the thread picked in the table: what the sidebar describes.
    pub selected: Option<String>,
    /// Scripts whose compat mode changed while they run: the thread's key mapped to the mode
    /// the script was *loaded* with. The Mode cell says "Reload required" while the picked
    /// mode differs from that one, so switching away and back cancels it.
    pub reload_pending: HashMap<String, u32>,
    /// Whether the sidebar's `Script Flags` section is open.
    pub fields_open: bool,
    /// Columns the table had to drop because the sheet was too narrow.
    pub hidden_columns: Vec<&'static str>,
}

/// How a script's thread sits in the engine: parked at infinity or not. A script standing on
/// a long `wait` is still active.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Active,
    Suspended,
}

impl Status {
    fn of(script: *mut CRunningScript) -> Status {
        if threads::is_suspended(script) {
            Status::Suspended
        } else {
            Status::Active
        }
    }

    fn label(self) -> &'static str {
        match self {
            Status::Active => strings::scripts::STATUS_ACTIVE,
            Status::Suspended => strings::scripts::STATUS_SUSPENDED,
        }
    }

    /// The glyph on the cell is the action a click performs, not the state: the same
    /// convention as the debugger's own Pause / Resume button.
    fn icon(self) -> Icon {
        match self {
            Status::Active => Icon::Pause,
            Status::Suspended => Icon::Play,
        }
    }
}

/// The filter chips above the table.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Chip {
    All,
    Active,
    Suspended,
    Custom,
    Game,
}

impl Chip {
    const ALL: [(Chip, &'static str); 5] = [
        (Chip::All, strings::scripts::CHIP_ALL),
        (Chip::Active, strings::scripts::CHIP_ACTIVE),
        (Chip::Suspended, strings::scripts::CHIP_SUSPENDED),
        (Chip::Custom, strings::scripts::CHIP_CUSTOM),
        (Chip::Game, strings::scripts::CHIP_GAME),
    ];

    fn from_index(index: usize) -> Chip {
        Self::ALL
            .get(index)
            .map(|(chip, _)| *chip)
            .unwrap_or(Chip::All)
    }

    fn tip(self) -> Option<&'static str> {
        match self {
            Chip::Custom => Some(strings::scripts::TIP_CHIP_CUSTOM),
            Chip::Game => Some(strings::scripts::TIP_CHIP_GAME),
            _ => None,
        }
    }

    /// Whether the chip covers this half of the queue at all.
    fn wants(self, thread: &threads::Entry) -> bool {
        match self {
            Chip::All | Chip::Active | Chip::Suspended => true,
            Chip::Custom => thread.is_cleo(),
            Chip::Game => !thread.is_cleo(),
        }
    }

    fn keeps(self, status: Status) -> bool {
        match self {
            Chip::All | Chip::Custom | Chip::Game => true,
            Chip::Active => status == Status::Active,
            Chip::Suspended => status == Status::Suspended,
        }
    }
}

/// One row of the Scripts table, resolved once so the widths, the cells and the sidebar all
/// read the same values.
struct Row {
    /// 1-based position in the table, which is what the `#` column shows.
    ordinal: usize,
    thread: threads::Entry,
    status: Status,
    /// CLEO keeps the compat mode per script it loaded, and one shared mode for every thread
    /// the game started.
    version: u32,
    /// Bytes of bytecode from `BaseIP`, approximated for a game thread.
    size: Option<usize>,
    traced: bool,
    paused: bool,
}

impl Row {
    fn is_cleo(&self) -> bool {
        self.thread.is_cleo()
    }

    fn locked(&self) -> bool {
        !self.is_cleo()
            && unsafe { (*self.thread.ptr).m_bIsMission }
            && !settings::allow_native_control()
    }
}

/// The page's body. Returns a request to open a script in the debugger, if the keys or the
/// sidebar asked for one.
pub fn page(
    ui: &Ui,
    state: &mut State,
    intent: Intent,
    page_rows: i32,
    width: f32,
) -> Report {
    if let Some(chip) = intent.chip {
        state.chip = chip.min(Chip::ALL.len() - 1);
    }
    toolbar(ui, state, intent.filter);

    let needle = state.filter.trim().to_lowercase();
    let chip = Chip::from_index(state.chip);

    // The whole queue, once: a filter hides a script from the table, it does not stop it, so
    // a selection or a pending reload has to survive being filtered out.
    let entries = threads::all();
    let live = |key: &str| entries.iter().any(|entry| entry.key == key);
    state.reload_pending.retain(|key, _| live(key));
    if state.selected.as_deref().is_some_and(|key| !live(key)) {
        state.selected = None;
    }

    let rows = rows(entries, chip, &needle);
    let mut report = Report {
        shown: rows.len(),
        ..Default::default()
    };

    // The selection is the page's row cursor, so the keys and the pointer always mean the
    // same script in the table and in the sidebar.
    let start = state
        .selected
        .as_deref()
        .and_then(|key| rows.iter().position(|row| row.thread.key == key));
    let cursor = nav::moved_row(start, intent.row, rows.len(), page_rows);
    if cursor != start {
        state.selected = cursor.map(|index| rows[index].thread.key.clone());
    }

    if let Some(row) = cursor.map(|index| &rows[index]) {
        if intent.activate {
            let thread = &row.thread;
            report.open = Some((thread.key.clone(), thread.name.clone(), thread.kind));
        }
        if intent.toggle {
            threads::toggle_suspended(row.thread.ptr);
        }
    }

    theme::bare_child(ui, "script_tab_inner", || {
        state.hidden_columns = table_body(
            ui,
            &rows,
            ui.content_region_avail()[1],
            width,
            &mut state.selected,
            &mut state.reload_pending,
            // Only a key scrolls the table: the mouse has the wheel.
            cursor.filter(|_| intent.row.is_some()),
        );
    });

    report
}

/// How many filter chips the page has, for the footer's key hint.
pub const CHIP_COUNT: usize = Chip::ALL.len();

/// The row above the table: the filter box, the status chips and the column picker.
fn toolbar(ui: &Ui, state: &mut State, focus: bool) {
    let room = ui.content_region_avail()[0];
    let origin = ui.cursor_screen_pos();
    let columns_label = strings::scripts::COLUMNS;
    let columns_width = theme::dropdown_width(ui, columns_label);
    let search_width = (room * 0.30).clamp(200.0, 340.0);

    table::search_box(
        ui,
        "##_script_filter",
        strings::scripts::FILTER_PLACEHOLDER,
        &mut state.filter,
        focus,
        search_width,
    );
    theme::chips(ui, &Chip::ALL, Chip::tip, &mut state.chip);

    if let Some(offset) = table::right_slot(ui, origin[0], room, columns_width) {
        ui.same_line_with_spacing(offset, 0.0);
        theme::dropdown(ui, "script_columns", columns_label, "", || {
            for column in ScriptColumn::ALL {
                let shown = settings::script_column(column);
                if ui.menu_item_config(column.label()).selected(shown).build() {
                    settings::set_script_column(column, !shown);
                }
            }
        });
    }
}

fn rows(entries: Vec<threads::Entry>, chip: Chip, needle: &str) -> Vec<Row> {
    // The queue's base addresses, collected on the first game row that needs measuring.
    let mut bases: Option<Vec<usize>> = None;
    let mut rows = Vec::new();

    for thread in entries {
        // The cheap checks first: a row the table will not draw must not pay for a compat
        // lookup, a size measurement or a debugger lookup.
        if !chip.wants(&thread) {
            continue;
        }
        let script = thread.ptr;
        let status = Status::of(script);
        if !chip.keeps(status) || !thread.matches(needle) {
            continue;
        }

        let size = if thread.is_cleo() {
            threads::region_between(script, &[])
        } else {
            let bases = bases.get_or_insert_with(threads::bases);
            threads::region_between(script, bases)
        };
        let (traced, paused) = crate::debug::flags(&thread.key);

        rows.push(Row {
            ordinal: rows.len() + 1,
            version: unsafe { CLEO_GetScriptVersion(script) },
            status,
            size,
            thread,
            traced,
            paused,
        });
    }

    rows
}

/// The Scripts table. Returns the columns it had to drop because the sheet was too narrow.
fn table_body(
    ui: &Ui,
    rows: &[Row],
    height: f32,
    table_width: f32,
    selected: &mut Option<String>,
    reload_pending: &mut HashMap<String, u32>,
    scroll_to: Option<usize>,
) -> Vec<&'static str> {
    let style = ui.clone_style();
    let cell_pad = table::cell_pad(ui);
    let width = |text: &str| ui.calc_text_size(text)[0];
    let shown = settings::script_column;

    // The allocation limits: what the memory cell is a fraction of, and what it is coloured
    // against.
    let mut max_count = 0;
    let mut max_size = 0;
    if let Some(memory) = plugins::memory() {
        unsafe {
            (memory.allocation_limits)(&mut max_count, &mut max_size);
        }
    }

    let index_col = (width("000") + cell_pad).max(28.0);
    // The status cell is an icon button and its label; as wide as the wider of the two
    // states plus the box.
    let status_col =
        width(strings::scripts::STATUS_SUSPENDED) + (ui.text_line_height() * 0.9).round() + 10.0 + cell_pad;
    let mode_col = {
        let dropdown = theme::dropdown_width(ui, strings::scripts::COMPAT_UNKNOWN);
        let note = if reload_pending.is_empty() {
            0.0
        } else {
            width(strings::scripts::RELOAD_REQUIRED)
        };
        dropdown.max(note) + cell_pad
    };
    // `~` marks a measured game thread, so the widest cell is one character wider than the
    // size itself.
    let size_col = width("~888.88 MB").max(width(strings::scripts::HEADER_SIZE)) + cell_pad;
    // Sized from the widest value it can print, so a limit adjuster widens the column instead
    // of clipping the numbers.
    let memory_col = width(&strings::scripts::memory_cell(max_count, max_count, "888.88 MB"))
        .max(width(strings::scripts::HEADER_MEMORY))
        .max(120.0 * settings::get().ui_scale)
        + cell_pad;

    // The optional columns in table order. The mode is last: it is a control, not a reading.
    let mut optional: Vec<(ScriptColumn, f32, f32)> = Vec::new();
    for (column, wanted, floor) in [
        (ScriptColumn::Status, status_col, status_col),
        (ScriptColumn::Size, size_col, size_col),
        (ScriptColumn::Memory, memory_col, memory_col),
        (ScriptColumn::Mode, mode_col, mode_col),
    ] {
        if shown(column) {
            optional.push((column, wanted, floor.min(wanted)));
        }
    }

    // ImGui keeps a fixed column at its width and clips the rest, so the widths are fitted
    // here: the path elides into whatever the numbers leave, and the least useful columns
    // step out when even that does not fit.
    let room = (table_width - 12.0).max(320.0);
    let path_room = 120.0 * settings::get().ui_scale;
    let mut hidden: Vec<&'static str> = Vec::new();
    for victim in [
        ScriptColumn::Size,
        ScriptColumn::Mode,
        ScriptColumn::Memory,
        ScriptColumn::Status,
    ] {
        let wanted: f32 =
            index_col + optional.iter().map(|(_, width, _)| width).sum::<f32>() + path_room;
        if wanted <= room || optional.len() <= 1 {
            break;
        }
        if let Some(position) = optional.iter().position(|(column, _, _)| *column == victim) {
            hidden.push(optional[position].0.label());
            optional.remove(position);
        }
    }

    // What still overflows comes off the columns that have slack, in proportion to it.
    let overflow =
        index_col + optional.iter().map(|(_, width, _)| width).sum::<f32>() + path_room - room;
    if overflow > 0.0 {
        let slacks: Vec<f32> = optional
            .iter()
            .map(|(_, width, floor)| (width - floor).max(0.0))
            .collect();
        let total: f32 = slacks.iter().sum();
        if total > 0.0 {
            let take = overflow.min(total);
            for ((_, width, _), slack) in optional.iter_mut().zip(&slacks) {
                *width -= take * (slack / total);
            }
        }
    }

    let taken: f32 = index_col + optional.iter().map(|(_, width, _)| width).sum::<f32>();
    let path_col = (room - taken).max(80.0);

    let mut columns = vec![
        Column::fixed(strings::table::HEADER_INDEX, index_col),
        Column::fixed(strings::scripts::HEADER_SCRIPT, path_col),
    ];
    for (column, column_width, _) in &optional {
        columns.push(Column::fixed(column.label(), *column_width));
    }
    if let Some(last) = columns.last_mut() {
        last.mark_trailing();
    }

    let notes = rows
        .iter()
        .filter(|row| row.is_cleo() && reload_pending.contains_key(&row.thread.key))
        .count() as f32;
    let extra = notes * (ui.text_line_height() + style.item_spacing[1]);
    let table_height = table::body_height(ui, rows.len(), extra, height);

    // ImGui keeps column widths per table id and only reads the initial width when it creates
    // the table, so the id carries the layout the widths were fitted for.
    let table_id = format!("scripts_table_{}_{}", room as i32, columns.len());

    // The cells follow the fitted list, not the settings: a dropped column keeps its setting
    // on, and a cell for it would land in the next column over.
    let visible = |column: ScriptColumn| optional.iter().any(|(kept, _, _)| *kept == column);

    let captions: Vec<(&str, &str)> = std::iter::once((strings::table::HEADER_INDEX, ""))
        .chain(std::iter::once((
            strings::scripts::HEADER_SCRIPT,
            if rows.is_empty() {
                strings::scripts::TIP_SELECT
            } else {
                ""
            },
        )))
        .chain(optional.iter().map(|(column, _, _)| {
            (
                column.label(),
                if *column == ScriptColumn::Status {
                    strings::scripts::TIP_STATUS
                } else {
                    ""
                },
            )
        }))
        .collect();

    table::table(ui, &table_id, &columns, table_height, |ui| {
        table::header(ui, &captions);

        // Where the frozen header ends, which is as high as the scrolling rows may go.
        let header_bottom = ui.item_rect_max()[1];

        for (index, row) in rows.iter().enumerate() {
            let has_note = reload_pending.contains_key(&row.thread.key);
            let row_height = ui.frame_height()
                + if has_note {
                    ui.text_line_height() + style.item_spacing[1]
                } else {
                    0.0
                };
            let row_top = table::body_row(
                ui,
                row_height,
                selected.as_deref() == Some(row.thread.key.as_str()),
            );

            ui.table_next_column();
            let number = row.ordinal.to_string();
            let number_clicked =
                table::cell_centered_fill(ui, &format!("##_idx{}", row.ordinal), &number, Role::Text);

            ui.table_next_column();
            let path_clicked = path_cell(ui, row);

            if number_clicked || path_clicked {
                *selected = Some(row.thread.key.clone());
            }

            if visible(ScriptColumn::Status) {
                ui.table_next_column();
                status_cell(ui, row);
            }
            if visible(ScriptColumn::Size) {
                ui.table_next_column();
                size_cell(ui, row);
            }
            if visible(ScriptColumn::Memory) {
                ui.table_next_column();
                memory_cell(ui, row, max_count, max_size);
            }
            if visible(ScriptColumn::Mode) {
                ui.table_next_column();
                mode_cell(ui, row, reload_pending);
            }

            if scroll_to == Some(index) {
                table::reveal_row(ui, row_top, row_height, header_bottom);
            }
        }
    });

    hidden
}

/// The name cell: the folder as a caption, the file name as the value, and a badge while the
/// script is traced or paused. Both it and the number select the row.
fn path_cell(ui: &Ui, row: &Row) -> bool {
    let badge = if row.paused {
        Some((strings::debug::PAUSED, Role::Warning))
    } else if row.traced {
        Some((strings::debug::BADGE_TRACING, Role::Dim))
    } else {
        None
    };

    let room = table::text_room(ui);
    let badge_room = badge
        .map(|(text, _)| ui.calc_text_size(text)[0] + 12.0)
        .unwrap_or(0.0);
    let (folder, name) = row.thread.label_parts();
    let name_room = (room - badge_room).max(40.0);
    let shown_name = elide_end(ui, &name, name_room);
    let folder_room = name_room - ui.calc_text_size(&shown_name)[0];
    let shown_folder = if folder_room > 8.0 {
        elide_path(ui, &folder, folder_room)
    } else {
        String::new()
    };

    let mut parts: Vec<(&str, Role)> = Vec::new();
    if !shown_folder.is_empty() {
        parts.push((&shown_folder, Role::Dim));
    }
    parts.push((&shown_name, Role::Text));
    if let Some((text, role)) = badge {
        parts.push(("  ", Role::Dim));
        parts.push((text, role));
    }

    table::cell_parts_fill(ui, &format!("##_path{}", row.ordinal), &parts, &row.thread.label)
}

/// The status switch. A parked script has its wake time pushed to infinity, so clearing it
/// lets the script run again next frame.
fn status_cell(ui: &Ui, row: &Row) {
    let script = row.thread.ptr;
    let parked = row.status == Status::Suspended;
    let locked = row.locked();
    let disabled = ui.begin_disabled(locked);

    let glyph = (ui.text_line_height() * 0.9).round();
    let clicked = theme::icon_button(
        ui,
        &format!("##_status{}", row.ordinal),
        row.status.icon(),
        glyph,
        if locked {
            theme::dim_color()
        } else {
            theme::accent_color()
        },
    );
    // The hint belongs to the switch, so it is asked for before the label is drawn.
    theme::hint(
        ui,
        if locked {
            strings::scripts::TIP_SUSPEND_LOCKED
        } else if parked {
            strings::scripts::TIP_RESUME
        } else {
            strings::scripts::TIP_PAUSE
        },
    );
    drop(disabled);

    if clicked {
        threads::toggle_suspended(script);
    }

    // The word is text, not part of the switch: the glyph is the whole target.
    ui.same_line();
    ui.align_text_to_frame_padding();
    ui.text(row.status.label());
}

fn size_cell(ui: &Ui, row: &Row) {
    // A game thread's region is measured, not recorded: `~` says which of the two the cell
    // shows. A thread the engine recorded no start for has no measurement at all.
    let (text, tooltip) = match row.size {
        Some(bytes) if row.is_cleo() => (pretty_size(bytes), strings::scripts::size_tooltip(bytes)),
        Some(bytes) => (
            format!("~{}", pretty_size(bytes)),
            strings::scripts::native_size_tooltip(bytes),
        ),
        None if row.is_cleo() => (strings::scripts::SIZE_UNKNOWN.to_string(), String::new()),
        None => (
            strings::scripts::SIZE_UNKNOWN.to_string(),
            strings::scripts::TIP_SIZE_UNKNOWN_NATIVE.to_string(),
        ),
    };

    table::cell(
        ui,
        &format!("##_size{}", row.ordinal),
        &text,
        Role::Text,
        &tooltip,
    );
}

fn memory_cell(ui: &Ui, row: &Row, max_count: usize, max_size: usize) {
    if !row.is_cleo() {
        // The game's own threads allocate through the engine's pools, which CLEO does not
        // track.
        table::cell(
            ui,
            &format!("##_mem{}", row.ordinal),
            strings::scripts::MEMORY_NONE,
            Role::Text,
            strings::scripts::TIP_MEMORY_NATIVE,
        );
        return;
    }

    let mut count = 0;
    let mut bytes = 0;
    if let Some(memory) = plugins::memory() {
        unsafe {
            (memory.allocation_info)(row.thread.ptr, &mut count, &mut bytes);
        }
    }

    if count == 0 {
        table::cell(
            ui,
            &format!("##_mem{}", row.ordinal),
            strings::scripts::MEMORY_NONE,
            Role::Text,
            strings::scripts::TIP_MEMORY_NONE,
        );
        return;
    }

    let usage = pct(count, max_count).max(pct(bytes, max_size));
    let text = strings::scripts::memory_cell(count, max_count, &pretty_size(bytes));
    let text = elide_end(ui, &text, table::text_room(ui));

    table::cell(
        ui,
        &format!("##_mem{}", row.ordinal),
        &text,
        if usage < 50.0 {
            Role::Text
        } else if usage < 90.0 {
            Role::Warning
        } else {
            Role::Error
        },
        &strings::scripts::memory_tooltip(count, max_count, bytes, max_size),
    );
}

/// The compat mode, in the last column: the one cell that changes what the script does rather
/// than reporting on it.
fn mode_cell(ui: &Ui, row: &Row, reload_pending: &mut HashMap<String, u32>) {
    let version = row.version;
    let script = row.thread.ptr;

    if version < CLEO_VER_3 {
        table::cell(
            ui,
            &format!("##_compat{}", row.ordinal),
            strings::scripts::COMPAT_UNKNOWN,
            Role::Error,
            "",
        );
        return;
    }

    let tip = if row.is_cleo() {
        strings::scripts::TIP_COMPAT
    } else {
        strings::scripts::TIP_COMPAT_NATIVE
    };
    theme::dropdown(
        ui,
        &format!("script_mode_{}", row.ordinal),
        compat_label(version),
        tip,
        || {
            for (value, label) in COMPAT_LEVELS {
                if ui
                    .selectable_config(label)
                    .selected(compat_bucket(version) == value)
                    .build()
                {
                    // Remember the mode the script was loaded with, so switching away and
                    // back cancels the note. A native row has no such note: its mode is one
                    // setting for all of them, applied on the next use.
                    let key = row.thread.key.clone();
                    let loaded = if row.is_cleo() {
                        *reload_pending.entry(key.clone()).or_insert(version)
                    } else {
                        version
                    };

                    unsafe { CLEO_SetScriptVersion(script, value) };

                    if compat_bucket(value) == compat_bucket(loaded) {
                        reload_pending.remove(&key);
                    }
                }
            }
        },
    );

    if row.is_cleo() && reload_pending.contains_key(&row.thread.key) {
        ui.text_colored(Role::Warning.rgba(1.0), strings::scripts::RELOAD_REQUIRED);
    }
}

/// Write a CLEO script's bytecode to `<name>.dump` beside it. Only CLEO's own scripts can be
/// dumped: they are files, so the copy lands next to the original.
pub(super) fn dump_cleo(thread: &threads::Entry) {
    let script = thread.ptr;
    let size = unsafe { CLEO_GetScriptSize(script) };
    if size == 0 {
        return;
    }

    let base = threads::code_base(script) as *const u8;
    if base.is_null() {
        return;
    }

    let mut bytes = vec![0u8; size];
    unsafe { std::ptr::copy_nonoverlapping(base, bytes.as_mut_ptr(), size) };
    let _ = std::fs::write(format!("{}.dump", thread.name), bytes);
}
