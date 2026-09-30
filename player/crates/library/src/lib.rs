//! The game library: folder scanning, `library.toml`, pre-flight status, launching a game as a child
//! process, and the egui window (`gui`). Contract: player/CONTRACTS.md, "library".

mod config;
mod gui;
mod key;
mod launch;
mod scan;
mod status;

pub use config::{Game, Library, default_data_dir};
pub use gui::run_window;
pub use key::{game_key, resolve_game};
pub use launch::spawn_player;
pub use scan::{detect, scan_folders};
pub use status::{ModsInfo, Preflight, mods_info, preflight, preflight_path};
