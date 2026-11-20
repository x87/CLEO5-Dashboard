//! The Audio page: the streams `SA.Audio.cleo` holds, with their live stats.

use imgui::Ui;

use crate::nav::{self, RowMove};
use crate::plugins;
use crate::strings::{self, yes_no};
use crate::table::{self, Column};
use crate::theme::Role;
use crate::utils::{elide_path, make_path_relative_to_exe, ptr_to_rust_str};

/// One stream, its cells formatted up front: a column has to be sized from the text it will
/// actually show, or the first frame clips it.
struct Stream {
    name: String,
    kind: &'static str,
    length: String,
    state: &'static str,
    is_3d: &'static str,
    looped: &'static str,
    volume: String,
    speed: String,
}

/// The page body. Returns how many streams it listed.
pub fn page(
    ui: &Ui,
    nav_row: &mut Option<usize>,
    movement: Option<RowMove>,
    page_rows: i32,
) -> usize {
    let Some(audio) = plugins::audio() else {
        return 0;
    };
    let num_streams = unsafe { (audio.number_of_streams)() };
    if num_streams == 0 {
        ui.text_colored(Role::Dim.rgba(1.0), strings::audio::EMPTY);
        return 0;
    }

    let rows = collect(&audio, num_streams);
    let cursor = nav::moved_row(*nav_row, movement, rows.len(), page_rows);
    *nav_row = cursor;

    let spacing = ui.clone_style().item_spacing[0];
    let width = |text: &str| ui.calc_text_size(text)[0];
    let widest = |caption: &str, cells: &mut dyn Iterator<Item = f32>| -> f32 {
        cells.fold(width(caption), |widest, value| widest.max(value)) + spacing * 2.0
    };

    let index_col = (width("000") + spacing * 2.0).max(28.0);
    let length_room = width(" ") + width(strings::audio::SECONDS);
    let type_col = widest(strings::audio::HEADER_TYPE, &mut rows.iter().map(|r| width(r.kind)));
    let length_col = widest(
        strings::audio::HEADER_LENGTH,
        &mut rows.iter().map(|r| width(&r.length) + length_room),
    );
    let state_col = widest(
        strings::audio::HEADER_STATE,
        &mut rows.iter().map(|r| width(r.state)),
    );
    let three_d_col = widest(
        strings::audio::HEADER_3D,
        &mut rows.iter().map(|r| width(r.is_3d)),
    );
    let loop_col = widest(
        strings::audio::HEADER_LOOP,
        &mut rows.iter().map(|r| width(r.looped)),
    );
    let volume_col = widest(
        strings::audio::HEADER_VOLUME,
        &mut rows.iter().map(|r| width(&r.volume)),
    );
    let speed_col = widest(
        strings::audio::HEADER_SPEED,
        &mut rows.iter().map(|r| width(&r.speed)),
    );

    let columns = [
        Column::fixed(strings::table::HEADER_INDEX, index_col),
        Column::stretch(strings::audio::HEADER_FILENAME),
        Column::fixed(strings::audio::HEADER_TYPE, type_col),
        Column::fixed(strings::audio::HEADER_LENGTH, length_col),
        Column::fixed(strings::audio::HEADER_STATE, state_col),
        Column::fixed(strings::audio::HEADER_3D, three_d_col),
        Column::fixed(strings::audio::HEADER_LOOP, loop_col),
        Column::fixed(strings::audio::HEADER_VOLUME, volume_col),
        Column::fixed(strings::audio::HEADER_SPEED, speed_col).trailing(),
    ];
    let captions = [
        (strings::table::HEADER_INDEX, ""),
        (strings::audio::HEADER_FILENAME, ""),
        (strings::audio::HEADER_TYPE, ""),
        (strings::audio::HEADER_LENGTH, ""),
        (strings::audio::HEADER_STATE, ""),
        (strings::audio::HEADER_3D, ""),
        (strings::audio::HEADER_LOOP, ""),
        (strings::audio::HEADER_VOLUME, ""),
        (strings::audio::HEADER_SPEED, ""),
    ];
    let table_height = table::body_height(ui, rows.len(), 0.0, ui.content_region_avail()[1]);

    table::table(ui, "audio_table", &columns, table_height, |ui| {
        table::header(ui, &captions);
        let header_bottom = ui.item_rect_max()[1];

        for (i, row) in rows.iter().enumerate() {
            let row_top = table::body_row(ui, ui.frame_height(), cursor == Some(i));

            ui.table_next_column();
            let number = (i + 1).to_string();
            table::cell_centered(ui, &format!("##_audidx{i}"), &number, Role::Text);

            ui.table_next_column();
            let shown_name = elide_path(ui, &row.name, table::text_room(ui));
            table::cell(
                ui,
                &format!("##_audname{i}"),
                &shown_name,
                Role::Text,
                &row.name,
            );

            ui.table_next_column();
            table::cell(ui, &format!("##_audtype{i}"), row.kind, Role::Text, "");

            ui.table_next_column();
            // The number is the value; the unit beside it is a caption.
            table::cell_parts(
                ui,
                &format!("##_audlen{i}"),
                &[
                    (&row.length, Role::Text),
                    (" ", Role::Text),
                    (strings::audio::SECONDS, Role::Dim),
                ],
                "",
            );

            ui.table_next_column();
            table::cell(ui, &format!("##_audstate{i}"), row.state, Role::Text, "");

            ui.table_next_column();
            table::cell(ui, &format!("##_aud3d{i}"), row.is_3d, Role::Text, "");

            ui.table_next_column();
            table::cell(ui, &format!("##_audloop{i}"), row.looped, Role::Text, "");

            ui.table_next_column();
            table::cell(ui, &format!("##_audvol{i}"), &row.volume, Role::Text, "");

            ui.table_next_column();
            table::cell(ui, &format!("##_audspeed{i}"), &row.speed, Role::Text, "");

            if movement.is_some() && cursor == Some(i) {
                table::reveal_row(ui, row_top, ui.frame_height(), header_bottom);
            }
        }
    });

    rows.len()
}

fn collect(audio: &plugins::AudioApi, num_streams: usize) -> Vec<Stream> {
    let mut rows = Vec::with_capacity(num_streams);
    for i in 0..num_streams {
        let mut filename = [0i8; 260];
        let name = if unsafe { (audio.filename_by_index)(i, filename.as_mut_ptr(), filename.len()) } {
            make_path_relative_to_exe(ptr_to_rust_str(filename.as_ptr() as _))
        } else {
            String::new()
        };

        rows.push(Stream {
            name,
            kind: match unsafe { (audio.type_by_index)(i) } {
                0 => strings::audio::TYPE_NONE,
                1 => strings::audio::TYPE_SFX,
                2 => strings::audio::TYPE_MUSIC,
                3 => strings::audio::TYPE_UI,
                _ => strings::audio::TYPE_UNKNOWN,
            },
            length: format!("{:.2}", unsafe { (audio.length_by_index)(i) }),
            state: match unsafe { (audio.state_by_index)(i) } {
                -1 => strings::audio::STATE_STOPPED,
                0 => strings::audio::STATE_INACTIVE,
                1 => strings::audio::STATE_PLAYING,
                2 => strings::audio::STATE_PAUSED,
                _ => strings::audio::STATE_UNKNOWN,
            },
            is_3d: yes_no(unsafe { (audio.is_3d_by_index)(i) }),
            looped: yes_no(unsafe { (audio.is_looped_by_index)(i) }),
            volume: format!("{:.2}", unsafe { (audio.volume_by_index)(i) }),
            speed: format!("{:.2}", unsafe { (audio.speed_by_index)(i) }),
        });
    }
    rows
}
