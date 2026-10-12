//! The game library: folder scanning, `library.toml`, pre-flight status, launching a game as a child
//! process, and the egui window (`gui`). Contract: player/CONTRACTS.md, "library".

mod config;
mod gui;
pub mod import;
mod key;
mod launch;
pub mod mods;
mod scan;
pub mod settings;
mod status;
pub mod warm;

pub use config::{Game, GameRef, Library, default_data_dir};
pub use gui::run_window;
pub use key::{game_key, resolve_game};
pub use launch::spawn_player;
pub use scan::{detect, scan_folders};
pub use settings::Settings;
pub use status::{ModsInfo, Preflight, mods_info, preflight, preflight_path};
pub use warm::{PlayGuard, WarmGuard};
