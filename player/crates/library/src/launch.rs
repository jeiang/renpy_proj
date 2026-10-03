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

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// The Stream button calls `spawn_serve(player, game path, data)`. The gate runs
    /// `player serve <game> --data <data> ...`. Check the exact argv of the child and that the
    /// printed URLs reach the library window.
    #[test]
    fn spawn_serve_runs_the_serve_subcommand_and_collects_urls() {
        let dir = std::env::temp_dir().join(format!("stream-spawn-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("fake-player");
        let argv = dir.join("argv");
        std::fs::write(
            &exe,
            format!(
                "#!/bin/sh\nfor a in \"$@\"; do echo \"$a\"; done > '{}'\necho 'Stream URL: http://192.0.2.1:8080/'\necho 'other line'\nsleep 30\n",
                argv.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();

        let mut serve = spawn_serve(&exe, Path::new("/g/game"), Path::new("/d/data")).unwrap();
        for _ in 0..200 {
            serve.poll();
            if !serve.urls.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        assert_eq!(serve.urls, vec!["http://192.0.2.1:8080/".to_string()]);
        let args: Vec<String> = std::fs::read_to_string(&argv)
            .unwrap()
            .lines()
            .map(String::from)
            .collect();
        assert_eq!(args, ["serve", "/g/game", "--data", "/d/data"]);
        serve.stop();
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
