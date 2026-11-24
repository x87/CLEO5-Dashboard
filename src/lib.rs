//! In-game dashboard for CLEO 5 on GTA San Andreas: an ImGui sheet drawn over the game by
//! hudhook, showing the script threads, the loaded modules, CLEO's audio streams, its logs
//! and the overlay's own settings.

#[macro_use]
extern crate static_assertions;

mod app;
mod chrome;
mod dashboard_log;
mod debug;
mod highlight;
mod hook_install;
mod hook_probe;
mod icons;
mod logtail;
mod modules;
mod nav;
mod overlay;
mod pages;
mod plugins;
mod sdk;
mod settings;
mod strings;
mod table;
mod theme;
mod threads;
mod updates;
mod utils;

pub use app::Dashboard;
