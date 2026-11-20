//! The `Check Updates` button: ask GitHub for the latest CLEO release and hold the answer
//! for the title bar.

mod error;mod github;
mod version;

use std::sync::atomic::{AtomicU8, Ordering};

use crate::sdk::CLEO_GetVersionStr;
use crate::strings;
use crate::utils::ptr_to_rust_str;


/// The repository the check looks at.
const OWNER: &str = "cleolibrary";
const REPO: &str = "CLEO5";
const API_BASE: &str = "https://api.github.com";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Idle,
    Checking,
    Available,
    UpToDate,
}

impl Status {
    fn from_int(value: u8) -> Self {
        match value {
            1 => Status::Checking,
            2 => Status::Available,
            3 => Status::UpToDate,
            _ => Status::Idle,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Status::Idle => "",
            Status::Checking => strings::footer::CHECKING,
            Status::Available => strings::footer::UPDATE_AVAILABLE,
            Status::UpToDate => strings::footer::UP_TO_DATE,
        }
    }
}

static STATUS: AtomicU8 = AtomicU8::new(Status::Idle as u8);

pub fn status() -> Status {
    Status::from_int(STATUS.load(Ordering::Relaxed))
}

fn set_status(status: Status) {
    STATUS.store(status as u8, Ordering::Relaxed);
}

/// Runs on the render thread and blocks until GitHub answers, so the next frame's title bar
/// can show the result. Failures leave the button's label clean rather than stuck on
/// `Checking...`.
pub fn check() {
    set_status(Status::Checking);
    let url = format!("{API_BASE}/repos/{OWNER}/{REPO}/releases/latest");

    let Ok(release) = github::fetch_latest_release(&url) else {
        set_status(Status::Idle);
        return;
    };

    let current = ptr_to_rust_str(unsafe { CLEO_GetVersionStr() });
    let newer = version::is_newer(current, &release.version()).unwrap_or(false);
    set_status(if newer {
        Status::Available
    } else {
        Status::UpToDate
    });
}
