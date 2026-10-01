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

/// A running `player serve` child. `urls` fills as the child prints its `Stream URL:` lines.
pub struct Serve {
    pub child: Child,
    pub urls: Vec<String>,
    lines: std::sync::mpsc::Receiver<String>,
}

impl Serve {
    /// Collects the URLs the child has printed so far.
    pub fn poll(&mut self) {
        while let Ok(l) = self.lines.try_recv() {
            self.urls.push(l);
        }
    }

    /// Stops the child and waits for it, so that its port and GPU are free again.
    pub fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Starts `exe serve <game> --data <data>`. The child prints `Stream URL: <url>` on stdout for each LAN address.
pub fn spawn_serve(exe: &Path, game: &Path, data: &Path) -> std::io::Result<Serve> {
    use std::io::{BufRead, BufReader};
    let mut child = Command::new(exe)
        .arg("serve")
        .arg(game)
        .arg("--data")
        .arg(data)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()?;
    let (tx, rx) = std::sync::mpsc::channel();
    if let Some(out) = child.stdout.take() {
        std::thread::spawn(move || {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                if let Some(url) = line.strip_prefix("Stream URL: ") {
                    let _ = tx.send(url.trim().to_string());
                }
            }
        });
    }
    Ok(Serve {
        child,
        urls: Vec::new(),
        lines: rx,
    })
}

impl Drop for Serve {
    fn drop(&mut self) {
        self.stop();
    }
}
