//! The Patches tab shells out to `urp patches <game> ...` (the patch logic lives in the `player` crate, and
//! `apply-test` boots the engine). These functions run on a worker thread.

use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::Value;

/// A parsed `--json` result. Exit code 1 means the command found problems; the JSON is still valid.
pub struct Report {
    pub json: Value,
}

fn last_lines(text: &str) -> String {
    let lines: Vec<&str> = text.lines().rev().take(6).collect();
    lines.into_iter().rev().collect::<Vec<_>>().join("\n")
}

/// `exe patches <game> <sub> --json --data <data>`; `sub` is `list`, `validate` or `apply-test`.
pub fn run(exe: &Path, game: &Path, data: &Path, sub: &str) -> Result<Report, String> {
    let out = Command::new(exe)
        .arg("patches")
        .arg(game)
        .arg(sub)
        .arg("--json")
        .arg("--data")
        .arg(data)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("cannot start {}: {e}", exe.display()))?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let json = serde_json::from_str::<Value>(&stdout).ok().or_else(|| {
        stdout
            .lines()
            .rev()
            .find(|l| l.starts_with('{'))
            .and_then(|l| serde_json::from_str(l).ok())
    });
    match json {
        Some(json) => Ok(Report { json }),
        None => {
            let err = String::from_utf8_lossy(&out.stderr);
            Err(format!(
                "urp patches {sub} failed ({}):\n{}",
                out.status,
                last_lines(err.trim())
            ))
        }
    }
}

/// `exe patches <game> accept <id> --data <data>`; returns the line the command printed.
pub fn accept(exe: &Path, game: &Path, data: &Path, id: &str) -> Result<String, String> {
    let out = Command::new(exe)
        .arg("patches")
        .arg(game)
        .arg("accept")
        .arg(id)
        .arg("--data")
        .arg(data)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("cannot start {}: {e}", exe.display()))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(last_lines(String::from_utf8_lossy(&out.stderr).trim()))
    }
}

/// A JSON scalar as display text.
pub fn show(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => "?".into(),
        other => other.to_string(),
    }
}

/// The patch id `urp patches accept` takes: the patch file name without `.toml`.
pub fn patch_id(path: &str) -> Option<String> {
    Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_is_the_file_stem() {
        assert_eq!(
            patch_id("/d/patches/ab/fix-1.toml").as_deref(),
            Some("fix-1")
        );
    }

    #[test]
    fn scalars_show_without_quotes() {
        assert_eq!(show(&Value::from("a")), "a");
        assert_eq!(show(&Value::from(3)), "3");
        assert_eq!(show(&Value::Null), "?");
    }
}
