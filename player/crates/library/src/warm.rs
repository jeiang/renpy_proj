//! Warm-up: a headless run of a game that fills its caches (the rpyc cache and the bytecode cache
//! under `cache/<key>/<fingerprint>`) before the first play, so that the first play starts warm.
//!
//! The per-game lock `<data>/locks/<key>.lock` (Unix `flock`) keeps a warm-up and a play from writing
//! the same cache at once:
//! - a warm-up takes it exclusively and writes its pid into the file;
//! - a play or a serve takes it shared. When a warm-up holds it, the play sends SIGTERM to the pid
//!   in the file and waits. A stopped warm-up leaves a cache the next run can use: unusable entries
//!   are recompiled, as on any run.
//!
//! Warm-up needs Unix. On other platforms [`SUPPORTED`] is false, nothing starts a warm-up, and a
//! play takes no lock.

use std::path::{Path, PathBuf};
use std::process::Child;

use anyhow::Result;

use crate::config::Game;
use crate::settings::Settings;

pub use lock::{PlayGuard, WarmGuard};

/// Whether this platform runs warm-ups at all.
pub const SUPPORTED: bool = cfg!(unix);

/// Scheduling niceness of warm-up processes: they yield to foreground work.
#[cfg(unix)]
const NICE: libc::c_int = 10;

/// Lock file of one game.
pub fn lock_path(data: &Path, key: &str) -> PathBuf {
    data.join("locks").join(format!("{key}.lock"))
}

/// Games to warm up after they were added: all of them when warm-up is on, none when it is off or
/// unsupported.
pub fn warm_targets(settings: &Settings, added: &[Game]) -> Vec<PathBuf> {
    if !SUPPORTED || !settings.warm.enabled {
        return Vec::new();
    }
    added.iter().map(|g| g.path.clone()).collect()
}

/// Starts `exe warm <game>...` in its own process group, at low priority, with no stdio. The
/// driver warms the games one after another. Dropping the returned child leaves it running.
#[cfg(unix)]
pub fn spawn(exe: &Path, games: &[PathBuf], data: &Path) -> Result<Child> {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};

    let mut cmd = Command::new(exe);
    cmd.arg("warm")
        .args(games)
        .arg("--data")
        .arg(data)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0);
    // SAFETY: between fork and exec only setpriority runs, which is async-signal-safe. The niceness
    // is inherited by the game warm-ups the driver starts.
    unsafe {
        cmd.pre_exec(|| {
            libc::setpriority(libc::PRIO_PROCESS, 0, NICE);
            Ok(())
        });
    }
    Ok(cmd.spawn()?)
}

#[cfg(not(unix))]
pub fn spawn(_exe: &Path, _games: &[PathBuf], _data: &Path) -> Result<Child> {
    anyhow::bail!("warm-up is not supported on this platform")
}

#[cfg(unix)]
mod lock {
    use std::fs::{File, OpenOptions};
    use std::io::Write;
    use std::os::unix::io::AsRawFd;
    use std::path::Path;
    use std::time::Duration;

    use anyhow::{Context, Result};

    use super::lock_path;

    /// A warm-up's exclusive hold on one game's lock. Dropping it releases the lock.
    #[derive(Debug)]
    pub struct WarmGuard {
        _file: File,
    }

    /// A play's or a serve's shared hold on one game's lock. Several can be held at once.
    #[derive(Debug)]
    pub struct PlayGuard {
        _file: File,
    }

    fn open(data: &Path, key: &str) -> Result<File> {
        let p = lock_path(data, key);
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("cannot create {}", dir.display()))?;
        }
        OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&p)
            .with_context(|| format!("cannot open {}", p.display()))
    }

    /// Takes `op` without blocking. `Ok(false)` when another holder has the lock.
    fn try_lock(file: &File, op: libc::c_int) -> Result<bool> {
        loop {
            // SAFETY: flock reads only the descriptor, which `file` keeps open.
            if unsafe { libc::flock(file.as_raw_fd(), op | libc::LOCK_NB) } == 0 {
                return Ok(true);
            }
            let e = std::io::Error::last_os_error();
            match e.kind() {
                std::io::ErrorKind::WouldBlock => return Ok(false),
                std::io::ErrorKind::Interrupted => continue,
                _ => return Err(e.into()),
            }
        }
    }

    /// The pid a warm-up wrote into the lock file, if the file holds a usable one.
    fn holder_pid(path: &Path) -> Option<libc::pid_t> {
        std::fs::read_to_string(path)
            .ok()?
            .trim()
            .parse::<libc::pid_t>()
            .ok()
            .filter(|&pid| pid > 0)
    }

    impl WarmGuard {
        /// Takes the game's lock for a warm-up. `None` when a play, a serve or another warm-up holds
        /// it: the caller then skips this game.
        pub fn try_acquire(data: &Path, key: &str) -> Result<Option<WarmGuard>> {
            let mut file = open(data, key)?;
            if !try_lock(&file, libc::LOCK_EX)? {
                return Ok(None);
            }
            file.set_len(0)?;
            file.write_all(std::process::id().to_string().as_bytes())?;
            Ok(Some(WarmGuard { _file: file }))
        }
    }

    impl PlayGuard {
        /// Takes the game's lock shared, for a play or a serve. A warm-up that holds it is stopped
        /// first: SIGTERM goes to the pid it recorded, and the call waits until the lock is free.
        pub fn acquire(data: &Path, key: &str) -> Result<PlayGuard> {
            let path = lock_path(data, key);
            let file = open(data, key)?;
            while !try_lock(&file, libc::LOCK_SH)? {
                if let Some(pid) = holder_pid(&path) {
                    // SAFETY: a plain signal to the warm-up that recorded this pid. holder_pid
                    // rejects pid 0 and negative values, which would address process groups.
                    unsafe {
                        libc::kill(pid, libc::SIGTERM);
                    }
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Ok(PlayGuard { _file: file })
        }
    }
}

#[cfg(not(unix))]
mod lock {
    use std::path::Path;

    use anyhow::{Result, bail};

    /// Warm-up is unsupported here, so there is never a warm-up lock to take.
    #[derive(Debug)]
    pub struct WarmGuard;

    /// Plays take no lock on this platform.
    #[derive(Debug)]
    pub struct PlayGuard;

    impl WarmGuard {
        pub fn try_acquire(_data: &Path, _key: &str) -> Result<Option<WarmGuard>> {
            bail!("warm-up is not supported on this platform")
        }
    }

    impl PlayGuard {
        pub fn acquire(_data: &Path, _key: &str) -> Result<PlayGuard> {
            Ok(PlayGuard)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(path: &str) -> Game {
        Game {
            name: "g".into(),
            path: PathBuf::from(path),
            key: "g-1".into(),
            engine: None,
            renpy7: false,
            icon: None,
        }
    }

    #[test]
    fn disabled_setting_warms_nothing() {
        let added = [game("/games/a"), game("/games/b")];
        let mut s = Settings::default();
        s.set("warm.enabled", "false").unwrap();
        assert!(warm_targets(&s, &added).is_empty());
        s.set("warm.enabled", "true").unwrap();
        let on = warm_targets(&s, &added);
        assert_eq!(
            on,
            vec![PathBuf::from("/games/a"), PathBuf::from("/games/b")]
        );
    }

    #[cfg(unix)]
    mod unix {
        use std::os::unix::process::ExitStatusExt;
        use std::process::Command;
        use std::time::Duration;

        use super::super::*;
        static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

        #[test]
        fn two_warm_ups_exclude_each_other() {
            let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
            let d = tempfile::tempdir().unwrap();
            let first = WarmGuard::try_acquire(d.path(), "k").unwrap();
            assert!(first.is_some());
            assert!(WarmGuard::try_acquire(d.path(), "k").unwrap().is_none());
            drop(first);
            assert!(WarmGuard::try_acquire(d.path(), "k").unwrap().is_some());
        }

        #[test]
        fn a_play_blocks_warm_up_and_plays_share() {
            let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
            let d = tempfile::tempdir().unwrap();
            let p1 = PlayGuard::acquire(d.path(), "k").unwrap();
            let p2 = PlayGuard::acquire(d.path(), "k").unwrap();
            assert!(WarmGuard::try_acquire(d.path(), "k").unwrap().is_none());
            drop((p1, p2));
            assert!(WarmGuard::try_acquire(d.path(), "k").unwrap().is_some());
        }

        #[test]
        fn play_stops_a_warm_up_and_leaves_a_free_lock() {
            let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
            let d = tempfile::tempdir().unwrap();
            let key = "k";
            let warm = WarmGuard::try_acquire(d.path(), key).unwrap().unwrap();
            // A stand-in warm-up: a process that wrote a partial cache file and is still running.
            // The test process holds the lock for it and records its pid, as a real warm-up does.
            let partial = d.path().join("partial.rpyc");
            let mut child = Command::new("sh")
                .arg("-c")
                .arg(format!(
                    "printf partial > '{}'; exec sleep 30",
                    partial.display()
                ))
                .spawn()
                .unwrap();
            std::thread::sleep(Duration::from_millis(100));
            std::fs::write(lock_path(d.path(), key), child.id().to_string()).unwrap();

            let release = std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(200));
                drop(warm);
            });
            let play = PlayGuard::acquire(d.path(), key).unwrap();
            release.join().unwrap();

            let status = child.wait().unwrap();
            assert_eq!(status.signal(), Some(libc::SIGTERM));
            // The partial cache file stays on disk, as after a killed warm-up.
            assert_eq!(std::fs::read_to_string(&partial).unwrap(), "partial");
            drop(play);
            // The stopped warm-up's lock is free, so the next warm-up can run.
            assert!(WarmGuard::try_acquire(d.path(), key).unwrap().is_some());
        }
    }
}
