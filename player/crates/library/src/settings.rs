//! `<data>/settings.toml`: player settings that `urp` (and the library window) apply when a command line
//! does not say otherwise. A missing file means the defaults; unknown keys are an error.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

/// Defaults of `urp serve` and the library's Stream button.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StreamDefaults {
    /// HTTP and UDP port; 0 = pick a free one.
    pub port: u16,
    /// Width and height of the streamed picture.
    pub size: (u32, u32),
    pub fps: u32,
    pub kbps: u32,
}

impl Default for StreamDefaults {
    /// What `urp serve` used before there were settings.
    fn default() -> Self {
        StreamDefaults {
            port: 8080,
            size: (1920, 1080),
            fps: 60,
            kbps: 8000,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    /// Draw backend name (`--renderer`); `None` lets the game pick.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub renderer: Option<String>,
    pub stream: StreamDefaults,
}

/// The keys `get` and `set` know.
pub const KEYS: [&str; 5] = [
    "renderer",
    "stream.port",
    "stream.size",
    "stream.fps",
    "stream.kbps",
];

/// `WxH` as in `urp serve --size`.
pub fn parse_size(v: &str) -> Result<(u32, u32)> {
    let (w, h) = v
        .split_once(['x', 'X'])
        .with_context(|| format!("a size is <width>x<height>, got {v}"))?;
    let (w, h): (u32, u32) = (w.trim().parse()?, h.trim().parse()?);
    if w == 0 || h == 0 {
        bail!("a size must not be zero, got {v}");
    }
    Ok((w, h))
}

impl Settings {
    pub fn path(data: &Path) -> PathBuf {
        data.join("settings.toml")
    }

    /// A missing file is the defaults. A file that does not parse is an error.
    pub fn load(data: &Path) -> Result<Settings> {
        let p = Self::path(data);
        match std::fs::read_to_string(&p) {
            Ok(s) => toml::from_str(&s).with_context(|| format!("cannot parse {}", p.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Settings::default()),
            Err(e) => Err(e).with_context(|| format!("cannot read {}", p.display())),
        }
    }

    /// Writes through a temporary file so a crash never leaves half a file.
    pub fn save(&self, data: &Path) -> Result<()> {
        std::fs::create_dir_all(data)
            .with_context(|| format!("cannot create {}", data.display()))?;
        let p = Self::path(data);
        let tmp = p.with_extension("toml.tmp");
        std::fs::write(&tmp, toml::to_string(self)?)
            .with_context(|| format!("cannot write {}", tmp.display()))?;
        std::fs::rename(&tmp, &p).with_context(|| format!("cannot write {}", p.display()))
    }

    /// A key's value as text. An unset renderer is `default`.
    pub fn get(&self, key: &str) -> Result<String> {
        Ok(match key {
            "renderer" => self.renderer.clone().unwrap_or_else(|| "default".into()),
            "stream.port" => self.stream.port.to_string(),
            "stream.size" => format!("{}x{}", self.stream.size.0, self.stream.size.1),
            "stream.fps" => self.stream.fps.to_string(),
            "stream.kbps" => self.stream.kbps.to_string(),
            _ => bail!("unknown setting {key:?} (keys: {})", KEYS.join(", ")),
        })
    }

    /// Sets a key from text. `renderer` takes a name, or `default` (or nothing) to unset it.
    pub fn set(&mut self, key: &str, value: &str) -> Result<()> {
        let number = |what: &str| -> Result<u32> {
            value
                .trim()
                .parse()
                .with_context(|| format!("{what} wants a number, got {value:?}"))
        };
        match key {
            "renderer" => {
                let v = value.trim();
                self.renderer = (!v.is_empty() && v != "default").then(|| v.to_string());
            }
            "stream.port" => {
                self.stream.port = value
                    .trim()
                    .parse()
                    .with_context(|| format!("stream.port wants 0-65535, got {value:?}"))?
            }
            "stream.size" => self.stream.size = parse_size(value)?,
            "stream.fps" | "stream.kbps" => {
                let n = number(key)?;
                if n == 0 {
                    bail!("{key} must not be zero");
                }
                if key == "stream.fps" {
                    self.stream.fps = n;
                } else {
                    self.stream.kbps = n;
                }
            }
            _ => bail!("unknown setting {key:?} (keys: {})", KEYS.join(", ")),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_the_old_serve_defaults() {
        let s = Settings::default();
        assert_eq!(s.renderer, None);
        assert_eq!(
            (s.stream.port, s.stream.size, s.stream.fps, s.stream.kbps),
            (8080, (1920, 1080), 60, 8000)
        );
    }

    #[test]
    fn settings_round_trip_through_the_file() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(Settings::load(d.path()).unwrap(), Settings::default());
        let mut s = Settings::default();
        s.set("renderer", "gl").unwrap();
        s.set("stream.port", "0").unwrap();
        s.set("stream.size", "1280x720").unwrap();
        s.set("stream.fps", "30").unwrap();
        s.set("stream.kbps", "4000").unwrap();
        s.save(d.path()).unwrap();
        let back = Settings::load(d.path()).unwrap();
        assert_eq!(back, s);
        assert_eq!(back.get("stream.size").unwrap(), "1280x720");
        assert_eq!(back.get("renderer").unwrap(), "gl");
        let mut back = back;
        back.set("renderer", "default").unwrap();
        assert_eq!(back.renderer, None);
        // Only changed values need to be in the file.
        std::fs::write(Settings::path(d.path()), "[stream]\nfps = 24\n").unwrap();
        let partial = Settings::load(d.path()).unwrap();
        assert_eq!(partial.stream.fps, 24);
        assert_eq!(partial.stream.port, 8080);
    }

    #[test]
    fn bad_values_and_keys_are_errors() {
        let mut s = Settings::default();
        assert!(s.set("stream.size", "big").is_err());
        assert!(s.set("stream.size", "0x720").is_err());
        assert!(s.set("stream.fps", "0").is_err());
        assert!(s.set("stream.port", "70000").is_err());
        assert!(s.set("nope", "1").is_err());
        assert!(s.get("nope").is_err());
        assert_eq!(s, Settings::default());
    }
}
