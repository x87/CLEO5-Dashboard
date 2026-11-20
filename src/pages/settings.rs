//! The Settings page: the rows of [`settings::SCHEMA`], grouped by section.

use imgui::Ui;

use crate::settings;
use crate::theme;

/// The height the page needs: a caption over each section, one frame-height control per
/// setting, and the gaps between them.
pub fn body_height(ui: &Ui) -> f32 {
    let style = ui.clone_style();
    let spacing = style.item_spacing[1];

    let sections = settings::sections();
    let captions = sections.len() as f32;
    let rows = settings::SCHEMA.len() as f32;
    let items = rows * ui.frame_height() + captions * (ui.text_line_height() + 4.0);
    let gaps = (rows + captions - 1.0) * spacing;

    items + gaps + style.window_padding[1] * 2.0 + 12.0
}

pub fn page(ui: &Ui) {
    theme::padded_child(ui, "settings_tab_inner", || {
        let mut current = settings::get();
        let mut changed = false;
        let mut section = None;

        for def in settings::SCHEMA {
            if section != Some(def.section) {
                section = Some(def.section);
                theme::section(ui, def.section.label());
            }
            changed |= settings::row(ui, &mut current, def);
        }

        if changed {
            settings::set(current);
        }
    });
}
