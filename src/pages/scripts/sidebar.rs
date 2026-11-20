//! The Script Info panel beside the Scripts table.

use imgui::Ui;

use crate::icons::Icon;
use crate::plugins;
use crate::sdk::*;
use crate::strings::{self, yes_no};
use crate::theme::{self, Role};
use crate::threads;
use crate::utils::hex_address;

use super::{dump_cleo, State};
use crate::pages::DebugRequest;

/// The panel. Returns the thread the user asked to open in the debugger, if any.
pub fn panel(ui: &Ui, state: &mut State) -> Option<DebugRequest> {
    let Some(thread) = state.selected.as_deref().and_then(threads::find) else {
        // The script has gone away: the panel closes itself rather than describing a row
        // that is not there any more.
        state.selected = None;
        return None;
    };

    let script = thread.ptr;
    // Two names, because they are two things: the file the thread was loaded from, and the
    // name it gave itself with `03A4: name_thread`.
    let file_name = thread.source().to_string();
    let listed = unsafe { (*script).name() };
    let script_name = if listed.is_empty() {
        strings::sidebar::NO_SCRIPT_NAME
    } else {
        listed.as_str()
    };

    let right = ui.content_region_max()[0];
    let close = (ui.text_line_height() + 4.0).round();

    theme::section_head(
        ui,
        strings::sidebar::HEAD,
        Some(Icon::Cube),
        None,
        close + 8.0,
        None,
        theme::dim_color(),
    );

    // The close button shares the header row, on the right where a caret would be. A later
    // item wins the hover, so it stays clickable over the header's own hit area.
    theme::same_line_at(ui, right - close);
    let closed = theme::close_button(ui, "##sidebar_close", close * 0.55);
    theme::hint(ui, strings::sidebar::TIP_CLOSE);
    if closed {
        state.selected = None;
        return None;
    }
    ui.dummy([0.0, 4.0]);

    theme::field(ui, strings::sidebar::FIELD_FILE_NAME, &file_name);
    theme::hint(
        ui,
        if thread.is_cleo() {
            strings::sidebar::TIP_NAME
        } else {
            strings::sidebar::TIP_NAME_NATIVE
        },
    );

    theme::field(ui, strings::sidebar::FIELD_SCRIPT_NAME, script_name);
    theme::hint(ui, strings::sidebar::TIP_SCRIPT_NAME);

    let neighbour = pointer_rows(ui, script);
    address_rows(ui, script, thread.base);
    ui.dummy([0.0, 8.0]);

    debug_mode_row(ui, script, &thread);
    ui.dummy([0.0, 2.0]);

    let debugger = action_row(ui, &thread, state);
    ui.dummy([0.0, 2.0]);

    if debugger {
        return Some((thread.key, thread.name, thread.kind));
    }

    flags_section(ui, script, state);
    neighbour.map(|entry| (entry.key, entry.name, entry.kind))
}

/// The two neighbours in the engine's script list, each with a button that opens it. Returns
/// the one a button was pressed for.
fn pointer_rows(ui: &Ui, script: *mut CRunningScript) -> Option<threads::Entry> {
    let mut clicked = None;
    for (label, pointer) in [
        (strings::debug::FIELD_NEXT, unsafe { (*script).m_pNext } as usize),
        (strings::debug::FIELD_PREVIOUS, unsafe { (*script).m_pPrev } as usize),
    ] {
        let target = threads::at(pointer);
        let action = target
            .as_ref()
            .map(|_| (Icon::External, strings::sidebar::TIP_OPEN_POINTER));
        let (_, opened) = theme::field_action(ui, label, &hex_address(pointer), action);
        if target.is_some() {
            theme::hint(ui, strings::sidebar::TIP_OPEN_POINTER);
        }
        if opened {
            clicked = target;
        }
    }
    clicked
}

fn address_rows(ui: &Ui, script: *mut CRunningScript, base: usize) {
    theme::field(ui, strings::debug::FIELD_BASE_IP, &hex_address(base));
    theme::field(
        ui,
        strings::debug::FIELD_CURRENT_IP,
        &hex_address(unsafe { (*script).m_nIp }),
    );
    theme::field(
        ui,
        strings::debug::FIELD_SP,
        &unsafe { (*script).m_nStackPointer }.to_string(),
    );
    let wake_time = if threads::alive(script) {
        unsafe { (*script).m_nWakeTime }.to_string()
    } else {
        "-".to_string()
    };
    theme::field(ui, strings::debug::FIELD_WAKE_TIME, &wake_time);
}

/// The switch that makes DebugUtils log this script's opcodes. Without the plugin the flag
/// goes nowhere, so the row is not drawn at all; the flag is CLEO's, per script it loaded, so
/// a game thread has none either.
fn debug_mode_row(ui: &Ui, script: *mut CRunningScript, thread: &threads::Entry) {
    if !plugins::debug_utils() || !thread.is_cleo() {
        return;
    }

    let mut debug_mode = unsafe { CLEO_GetScriptDebugMode(script) };
    ui.align_text_to_frame_padding();
    ui.text_colored(Role::Dim.rgba(1.0), strings::sidebar::DEBUG_MODE);
    theme::hint(ui, strings::sidebar::TIP_DEBUG_MODE);

    let switch_width = ((ui.text_line_height() + 1.0) * 1.7).round();
    theme::same_line_at(ui, ui.content_region_max()[0] - switch_width);
    if theme::switch(ui, "##sidebar_debug", &mut debug_mode) {
        unsafe { CLEO_SetScriptDebugMode(script, debug_mode) };
    }
}

/// The row of buttons that act on the script. Returns whether the debugger was asked for.
fn action_row(ui: &Ui, thread: &threads::Entry, state: &mut State) -> bool {
    let mut debugger = false;
    if theme::flat_button(ui, format!("{}##sidebar_debugger", strings::sidebar::DEBUGGER)) {
        debugger = true;
    }
    theme::hint(ui, strings::sidebar::TIP_OPEN_DEBUGGER);

    if !thread.is_cleo() {
        return debugger;
    }

    let script = thread.ptr;
    ui.same_line();
    if theme::flat_button(ui, format!("{}##sidebar_dump", strings::sidebar::DUMP)) {
        dump_cleo(thread);
    }
    theme::hint(ui, strings::sidebar::TIP_DUMP);

    ui.same_line();
    if theme::flat_button(ui, format!("{}##sidebar_reload", strings::sidebar::RELOAD)) {
        unsafe {
            // The new instance inherits the parent's compatibility mode, so reloading is what
            // actually applies a compat change.
            let name = CLEO_GetScriptFilename(script);
            CLEO_CreateCustomScript(script, name, 0);
            CLEO_TerminateScript(script);
        }
        state.reload_pending.remove(&thread.key);
    }
    theme::hint(ui, strings::sidebar::TIP_RELOAD);

    debugger
}

fn flags_section(ui: &Ui, script: *mut CRunningScript, state: &mut State) {
    // The rows of chrome between the sheet and the footer come out of the item spacing as
    // well; the extra dummy keeps the footer off the last child.
    ui.dummy([0.0, 10.0]);
    let toggled = theme::section_head(
        ui,
        strings::sidebar::FIELDS,
        Some(Icon::Flag),
        Some(state.fields_open),
        0.0,
        Some(strings::keys::FIELDS),
        theme::dim_color(),
    );
    if toggled {
        state.fields_open = !state.fields_open;
    }
    if !state.fields_open {
        return;
    }

    ui.dummy([0.0, 4.0]);
    for (label, tip, on) in [
        (
            strings::debug::FLAG_ACTIVE,
            strings::debug::TIP_FLAG_ACTIVE,
            unsafe { (*script).m_bIsActive },
        ),
        (
            strings::debug::FLAG_MISSION,
            strings::debug::TIP_FLAG_MISSION,
            unsafe { (*script).m_bIsMission },
        ),
        (
            strings::debug::FLAG_CUSTOM,
            strings::debug::TIP_FLAG_CUSTOM,
            unsafe { (*script).is_custom() },
        ),
        (
            strings::debug::FLAG_WASTED_CHECK,
            strings::debug::TIP_FLAG_WASTED_CHECK,
            unsafe { (*script).m_bWastedBustedCheck },
        ),
        (
            strings::debug::FLAG_WASTED,
            strings::debug::TIP_FLAG_WASTED,
            unsafe { (*script).m_bWastedOrBusted },
        ),
    ] {
        theme::flag_row(ui, label, on, yes_no(on));
        theme::hint(ui, tip);
    }

    ui.dummy([0.0, 4.0]);
    ui.separator();
    ui.dummy([0.0, 4.0]);

    // Not flags: the values behind the debugger's own field panel, in the same shape.
    let logical = unsafe { (*script).m_nAndOrState };
    let not_flag = unsafe { (*script).m_bNotFlag };
    let scm_function = unsafe { (*script).scm_function() };
    theme::flag_row(
        ui,
        strings::debug::INFO_LOGICAL_OP,
        logical != 0,
        &format!("0x{logical:02X}"),
    );
    theme::flag_row(
        ui,
        strings::debug::INFO_NOT_FLAG,
        not_flag,
        yes_no(not_flag),
    );
    theme::flag_row(
        ui,
        strings::debug::INFO_SCM_FUNCTION,
        scm_function != 0,
        &scm_function.to_string(),
    );
}
