//! Launching a game: `player <game> --data <data>` as a child process.

use std::path::Path;
use std::process::{Child, Command, Stdio};

/// Starts `exe <game> --data <data>`. The child owns its window; the caller may drop the handle.
pub fn spawn_player(exe: &Path, game: &Path, data: &Path) -> std::io::Result<Child> {
    Command::new(exe)
        .arg(game)
        .arg("--data")
        .arg(data)
        .stdin(Stdio::null())
        .spawn()
}
