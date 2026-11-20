//! Keyboard navigation for the menu sheet.
//!
//! Keys are read from the keyboard state rather than ImGui's event queue, like the menu
//! hotkey (see `ToggleKeyState` in `app.rs`), so a tap is seen whatever the game does with
//! the messages. [`Nav::poll`] turns the keyboard into an [`Intent`]; what an intent means is
//! up to the render loop, which is the only place that knows how many rows a page drew.

use hudhook::windows::Win32::System::SystemInformation::GetTickCount;

use crate::utils::async_key_down;

// Virtual-key codes, US layout.
const VK_TAB: i32 = 0x09;
const VK_RETURN: i32 = 0x0D;
/// The space bar, which is the table's own Pause/Resume switch on Scripts.
const VK_SPACE: i32 = 0x20;
const VK_SHIFT: i32 = 0x10;
const VK_ESCAPE: i32 = 0x1B;
const VK_PAGE_UP: i32 = 0x21;
const VK_PAGE_DOWN: i32 = 0x22;
const VK_END: i32 = 0x23;
const VK_HOME: i32 = 0x24;
const VK_LEFT: i32 = 0x25;
const VK_UP: i32 = 0x26;
const VK_RIGHT: i32 = 0x27;
const VK_DOWN: i32 = 0x28;
/// The `/` key, next to the right shift.
const VK_OEM_2: i32 = 0xBF;
/// The modifiers the desktop uses with `Tab`: `Alt+Tab` and `Win+Tab` must not switch pages.
const VK_MENU: i32 = 0x12;
const VK_LWIN: i32 = 0x5B;
const VK_RWIN: i32 = 0x5C;
/// The number row that picks a page's filter chips, in the order they are drawn.
const NUMBER_KEYS: [i32; 5] = [0x31, 0x32, 0x33, 0x34, 0x35];
/// The debugger's two controls, on the function keys a debugger usually has them: `F5`
/// pauses and resumes, `F10` runs one instruction.
const VK_F5: i32 = 0x74;
const VK_F10: i32 = 0x79;
/// `F`, which opens and closes the fields card under the panes.
const VK_F: i32 = 0x46;

/// How long an arrow has to be held before it starts repeating.
const REPEAT_DELAY_MS: u32 = 360;
/// How often a held arrow repeats its move afterwards.
const REPEAT_MS: u32 = 55;

/// What the keys asked the page's table to do with its row cursor.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RowMove {
    /// Down (positive) or up (negative) by that many rows.
    Steps(i32),
    /// By a screenful, which the page counts in the rows its body holds.
    Page(i32),
    First,
    Last,
}

impl RowMove {
    /// Where the cursor lands. The ends stop rather than wrap, and an empty list keeps no
    /// cursor at all.
    pub fn resolve(self, start: Option<usize>, len: usize, page: i32) -> Option<usize> {
        if len == 0 {
            return None;
        }

        let last = len - 1;
        match self {
            RowMove::First => Some(0),
            RowMove::Last => Some(last),
            RowMove::Steps(step) => Some(step_from(start, step, last)),
            RowMove::Page(screens) => Some(step_from(start, screens * page.max(1), last)),
        }
    }

    /// The same move in rows, for pages that scroll instead of picking a row: the log
    /// counts its lines this way.
    pub fn rows(self, page: i32) -> i32 {
        match self {
            RowMove::Steps(step) => step,
            RowMove::Page(screens) => screens * page.max(1),
            // Clamped by the caller's own scroll range, whatever it turns out to be.
            RowMove::First => i32::MIN,
            RowMove::Last => i32::MAX,
        }
    }
}

fn step_from(start: Option<usize>, step: i32, last: usize) -> usize {
    match start {
        // Nothing is picked yet: either arrow lands on the first row rather than the
        // second, so the first press is never one the cursor skipped over.
        None => 0,
        Some(row) => row.saturating_add_signed(step as isize).min(last),
    }
}

/// Where a page's row cursor is after this frame's keys, clamped to the rows it drew. With no
/// movement the cursor only follows a list that shrank under a filter.
pub fn moved_row(
    start: Option<usize>,
    movement: Option<RowMove>,
    len: usize,
    page: i32,
) -> Option<usize> {
    match movement {
        Some(movement) => movement.resolve(start, len, page),
        None if len == 0 => None,
        None => start.map(|row| row.min(len - 1)),
    }
}

/// What the keys asked for this frame, in terms the render loop can act on.
#[derive(Clone, Copy, Default)]
pub struct Intent {
    /// Pages to move by: `-1` for the one on the left, `+1` for the one on the right.
    pub tab: Option<i32>,
    /// Escape: close the sheet.
    pub close: bool,
    /// Rows to move the page's table cursor by.
    pub row: Option<RowMove>,
    /// Enter: open whatever the row cursor is on.
    pub activate: bool,
    /// Space: park or resume the script the row cursor is on.
    pub toggle: bool,
    /// `/`: put the keyboard in the page's filter box.
    pub filter: bool,
    /// A number key: pick that filter chip, `0` being the first one drawn.
    pub chip: Option<usize>,
    /// `F5`: pause or resume the script the focused debugger is on.
    pub pause: bool,
    /// `F10`: run one instruction of it.
    pub step: bool,
    /// `F`: show or hide its fields card.
    pub fields: bool,
}

/// One key and its held state, which is what tells a press from a key that is still down.
struct Key {
    vk: i32,
    down: bool,
    /// When the next repeat is due, while the key is held.
    next_repeat: u32,
}

impl Key {
    const fn new(vk: i32) -> Self {
        Self {
            vk,
            down: false,
            next_repeat: 0,
        }
    }

    /// Take the held state without reporting a press, for a key that is already down when
    /// the sheet opens: opening the sheet with an arrow held must not move the cursor.
    fn prime(&mut self) {
        self.down = async_key_down(self.vk);
        self.next_repeat = 0;
    }

    /// True on the press, and again after every repeat interval while the key is held.
    fn repeat(&mut self, now: u32) -> bool {
        let down = async_key_down(self.vk);
        let was_down = std::mem::replace(&mut self.down, down);

        if !down {
            self.next_repeat = 0;
            return false;
        }
        if !was_down {
            self.next_repeat = now.wrapping_add(REPEAT_DELAY_MS);
            return true;
        }

        // Wrapping subtraction, so the comparison survives the tick counter rolling over
        // (after about seven weeks of uptime).
        if now.wrapping_sub(self.next_repeat) < u32::MAX / 2 {
            self.next_repeat = now.wrapping_add(REPEAT_MS);
            return true;
        }
        false
    }

    /// True on the press only: a held key fires once, for the moves that do not repeat.
    fn hit(&mut self) -> bool {
        let down = async_key_down(self.vk);
        !std::mem::replace(&mut self.down, down) && down
    }
}

/// The keys the sheet listens to, and whether it is up.
pub struct Nav {
    tab: Key,
    left: Key,
    right: Key,
    escape: Key,
    up: Key,
    down: Key,
    home: Key,
    end: Key,
    page_up: Key,
    page_down: Key,
    enter: Key,
    space: Key,
    filter: Key,
    pause: Key,
    step: Key,
    fields: Key,
    /// The number row, one key per chip a page can have.
    numbers: [Key; NUMBER_KEYS.len()],
    /// Whether the sheet was up on the previous frame, so that opening it primes the keys
    /// instead of firing a press for a key that was already down.
    open: bool,
}

impl Default for Nav {
    fn default() -> Self {
        Self::new()
    }
}

impl Nav {
    pub const fn new() -> Self {
        Self {
            tab: Key::new(VK_TAB),
            left: Key::new(VK_LEFT),
            right: Key::new(VK_RIGHT),
            escape: Key::new(VK_ESCAPE),
            up: Key::new(VK_UP),
            down: Key::new(VK_DOWN),
            home: Key::new(VK_HOME),
            end: Key::new(VK_END),
            page_up: Key::new(VK_PAGE_UP),
            page_down: Key::new(VK_PAGE_DOWN),
            enter: Key::new(VK_RETURN),
            space: Key::new(VK_SPACE),
            filter: Key::new(VK_OEM_2),
            pause: Key::new(VK_F5),
            step: Key::new(VK_F10),
            fields: Key::new(VK_F),
            numbers: [
                Key::new(NUMBER_KEYS[0]),
                Key::new(NUMBER_KEYS[1]),
                Key::new(NUMBER_KEYS[2]),
                Key::new(NUMBER_KEYS[3]),
                Key::new(NUMBER_KEYS[4]),
            ],
            open: false,
        }
    }

    /// Read the keyboard and say what the sheet should do this frame.
    ///
    /// While a text field has the keyboard (`typing`), only Tab and Escape keep their
    /// meaning: Tab is the one key a field never uses, and Escape is how a game menu closes.
    ///
    /// `menu_key` is skipped everywhere, even when navigation would use it, since one key
    /// cannot both move a cursor and close the thing the cursor is in.
    pub fn poll(&mut self, open: bool, typing: bool, menu_key: i32) -> Intent {
        let now = unsafe { GetTickCount() };

        if open && !self.open {
            // Keys already down when the sheet came up are not presses of it.
            let keys = [
                &mut self.tab,
                &mut self.left,
                &mut self.right,
                &mut self.escape,
                &mut self.up,
                &mut self.down,
                &mut self.home,
                &mut self.end,
                &mut self.page_up,
                &mut self.page_down,
                &mut self.enter,
                &mut self.space,
                &mut self.filter,
                &mut self.pause,
                &mut self.step,
                &mut self.fields,
            ];
            for key in keys {
                key.prime();
            }
            for key in self.numbers.iter_mut() {
                key.prime();
            }
        }
        self.open = open;

        // Every key is read every frame: a key skipped while a field has the keyboard would
        // look freshly pressed the moment the field went away.
        let shift = async_key_down(VK_SHIFT);
        // `Alt+Tab` is the task switcher and `Win+Tab` is the task view: the sheet sees the
        // Tab press but must not act on it, or switching away also switches its page.
        let foreign_tab =
            async_key_down(VK_MENU) || async_key_down(VK_LWIN) || async_key_down(VK_RWIN);
        // `hit` runs either way: the edge has to be consumed for the next frame.
        let tab = self.tab.hit() && !foreign_tab;
        let left = self.left.hit();
        let right = self.right.hit();
        let escape = self.escape.hit();
        let enter = self.enter.hit();
        let space = self.space.hit();
        let filter = self.filter.hit();
        let home = self.home.hit();
        let end = self.end.hit();
        let up = self.up.repeat(now);
        let down = self.down.repeat(now);
        let page_up = self.page_up.repeat(now);
        let page_down = self.page_down.repeat(now);
        // The debugger's two: `F5` pauses and resumes, `F10` runs one instruction.
        let f5 = self.pause.hit();
        let f10 = self.step.hit();
        let f = self.fields.hit();
        // One key per chip: they fire once per press, like the other pickers.
        let mut numbers = [false; NUMBER_KEYS.len()];
        for (index, key) in self.numbers.iter_mut().enumerate() {
            numbers[index] = key.hit();
        }

        let mut intent = Intent::default();
        if !open {
            return intent;
        }

        let usable = |key: &Key| key.vk != menu_key;

        // `Tab` is the one key a text field never uses, so the pages keep switching while
        // one has the keyboard. `Left`/`Right` are the caret's there, and only move pages
        // when nothing is being typed into.
        let mut step = 0;
        if tab && usable(&self.tab) {
            step += if shift { -1 } else { 1 };
        }
        if !typing {
            if left && usable(&self.left) {
                step -= 1;
            }
            if right && usable(&self.right) {
                step += 1;
            }
        }
        intent.tab = (step != 0).then(|| step.clamp(-1, 1));
        intent.close = escape && usable(&self.escape);

        // Everything else stands down for a text field; see the doc comment.
        if typing {
            return intent;
        }

        intent.activate = enter && usable(&self.enter);
        intent.toggle = space && usable(&self.space);
        intent.filter = filter && usable(&self.filter);
        intent.pause = f5 && usable(&self.pause);
        intent.step = f10 && usable(&self.step);
        intent.fields = f && usable(&self.fields);
        for (index, pressed) in numbers.iter().enumerate() {
            // A number that is the menu hotkey closes the sheet instead of filtering it.
            if *pressed && self.numbers[index].vk != menu_key {
                intent.chip = Some(index);
                break;
            }
        }

        // The last one read wins when a pair is pressed at once, which is also the order
        // they are listed in: a page jump beats a single row.
        let mut row = None;
        if up && usable(&self.up) {
            row = Some(RowMove::Steps(-1));
        }
        if down && usable(&self.down) {
            row = Some(RowMove::Steps(1));
        }
        if page_up && usable(&self.page_up) {
            row = Some(RowMove::Page(-1));
        }
        if page_down && usable(&self.page_down) {
            row = Some(RowMove::Page(1));
        }
        if home && usable(&self.home) {
            row = Some(RowMove::First);
        }
        if end && usable(&self.end) {
            row = Some(RowMove::Last);
        }
        intent.row = row;

        intent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_step_from_nothing_lands_on_the_first_row() {
        assert_eq!(RowMove::Steps(1).resolve(None, 5, 3), Some(0));
        assert_eq!(RowMove::Steps(-1).resolve(None, 5, 3), Some(0));
    }

    #[test]
    fn the_ends_stop_instead_of_wrapping() {
        assert_eq!(RowMove::Steps(1).resolve(Some(4), 5, 3), Some(4));
        assert_eq!(RowMove::Steps(-1).resolve(Some(0), 5, 3), Some(0));
    }

    #[test]
    fn a_page_is_as_tall_as_the_body() {
        assert_eq!(RowMove::Page(1).resolve(Some(2), 40, 10), Some(12));
        assert_eq!(RowMove::Page(-1).resolve(Some(12), 40, 10), Some(2));
        // A page shorter than a row still moves one.
        assert_eq!(RowMove::Page(1).resolve(Some(0), 40, 0), Some(1));
    }

    #[test]
    fn an_empty_page_keeps_no_cursor() {
        assert_eq!(RowMove::First.resolve(Some(3), 0, 10), None);
        assert_eq!(RowMove::Last.resolve(None, 0, 10), None);
        assert_eq!(moved_row(Some(3), None, 0, 10), None);
    }

    #[test]
    fn the_cursor_follows_a_list_that_shrank() {
        assert_eq!(moved_row(Some(9), None, 4, 10), Some(3));
        assert_eq!(moved_row(None, None, 4, 10), None);
    }

    #[test]
    fn home_and_end_are_the_first_and_last_row() {
        assert_eq!(RowMove::First.resolve(Some(4), 5, 3), Some(0));
        assert_eq!(RowMove::Last.resolve(Some(0), 5, 3), Some(4));
    }

    #[test]
    fn a_row_move_in_rows_is_signed() {
        assert_eq!(RowMove::Steps(-1).rows(10), -1);
        assert_eq!(RowMove::Page(2).rows(10), 20);
        assert!(RowMove::First.rows(10) < -1_000_000);
        assert!(RowMove::Last.rows(10) > 1_000_000);
    }
}
