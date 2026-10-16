//! Reading a mod source: a folder or an archive, walked entry by entry. One walker per format shows
//! the same `Entry` to a callback; the callback says whether to write the entry's data to a path.

use std::fs::File;
use std::io::{self, BufReader, Read};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Folder,
    Zip,
    TarGz,
    TarXz,
    TarBz2,
    TarZst,
    Tar,
    SevenZ,
    Rar,
}

/// What the callback wants done with the data of an entry.
pub enum Want {
    Skip,
    /// Write the entry's bytes to this file; its parent folder exists.
    Write(PathBuf),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    File,
    Dir,
    Symlink,
    /// Device, FIFO, sparse file and the like.
    Special,
}

pub struct Entry {
    /// The name as the archive stores it, not yet checked.
    pub path: String,
    /// The size the archive declares.
    pub size: u64,
    pub kind: Kind,
}

/// File name suffixes (lower case) and the format they select.
pub const SUFFIXES: &[(&str, Format)] = &[
    (".zip", Format::Zip),
    (".tar.gz", Format::TarGz),
    (".tgz", Format::TarGz),
    (".tar.xz", Format::TarXz),
    (".txz", Format::TarXz),
    (".tar.zst", Format::TarZst),
    (".tzst", Format::TarZst),
    (".tar.bz2", Format::TarBz2),
    (".tbz2", Format::TarBz2),
    (".tar", Format::Tar),
    (".7z", Format::SevenZ),
    (".rar", Format::Rar),
];

/// The format of an archive file by its name; `None` for an unknown suffix.
pub fn format_of(name: &str) -> Option<(Format, &str)> {
    let lower = name.to_ascii_lowercase();
    SUFFIXES
        .iter()
        .find(|(s, _)| lower.ends_with(s))
        .map(|&(s, f)| (f, &name[..name.len() - s.len()]))
}

/// Copies at most `*budget` bytes of `r` to `dest` and takes them off the budget. A source that holds more
/// than the budget (an archive that lies about its sizes) is an error.
fn copy_limited(r: &mut dyn Read, dest: &Path, budget: &mut u64) -> Result<()> {
    let mut f = File::create(dest).with_context(|| format!("cannot create {}", dest.display()))?;
    let n = io::copy(&mut r.take(budget.saturating_add(1)), &mut f)
        .with_context(|| format!("cannot extract {}", dest.display()))?;
    if n > *budget {
        bail!("the archive unpacks to more than the size limit");
    }
    *budget -= n;
    Ok(())
}

type Visit<'a> = dyn FnMut(&Entry) -> Result<Want> + 'a;

/// Walks `src`. With `list_only` no data is read and every `Want` is ignored (the callback should
/// still answer `Skip`). `budget` is the number of bytes that may be written.
pub fn walk(
    src: &Path,
    format: Format,
    list_only: bool,
    budget: &mut u64,
    cb: &mut Visit,
) -> Result<()> {
    match format {
        Format::Folder => walk_folder(src, src, list_only, budget, cb),
        Format::Zip => walk_zip(src, list_only, budget, cb),
        Format::Tar => walk_tar(BufReader::new(open(src)?), list_only, budget, cb),
        Format::TarGz => walk_tar(
            flate2::read::GzDecoder::new(BufReader::new(open(src)?)),
            list_only,
            budget,
            cb,
        ),
        Format::TarXz => walk_tar(
            xz2::read::XzDecoder::new(BufReader::new(open(src)?)),
            list_only,
            budget,
            cb,
        ),
        Format::TarZst => walk_tar(
            zstd::stream::read::Decoder::new(open(src)?)?,
            list_only,
            budget,
            cb,
        ),
        Format::TarBz2 => walk_tar(
            bzip2::read::MultiBzDecoder::new(open(src)?),
            list_only,
            budget,
            cb,
        ),
        Format::SevenZ => walk_7z(src, list_only, budget, cb),
        Format::Rar => walk_rar(src, list_only, budget, cb),
    }
}

fn open(p: &Path) -> Result<File> {
    File::open(p).with_context(|| format!("cannot open {}", p.display()))
}

fn walk_folder(
    root: &Path,
    dir: &Path,
    list_only: bool,
    budget: &mut u64,
    cb: &mut Visit,
) -> Result<()> {
    let mut items: Vec<_> = std::fs::read_dir(dir)
        .with_context(|| format!("cannot read {}", dir.display()))?
        .collect::<io::Result<_>>()?;
    items.sort_by_key(|e| e.file_name());
    for e in items {
        let path = e.path();
        let meta = std::fs::symlink_metadata(&path)?;
        let t = meta.file_type();
        let rel = path
            .strip_prefix(root)
            .expect("walked below root")
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        let kind = if t.is_symlink() {
            Kind::Symlink
        } else if t.is_dir() {
            Kind::Dir
        } else if t.is_file() {
            Kind::File
        } else {
            Kind::Special
        };
        let want = cb(&Entry {
            path: rel,
            size: if kind == Kind::File { meta.len() } else { 0 },
            kind,
        })?;
        if kind == Kind::Dir {
            walk_folder(root, &path, list_only, budget, cb)?;
        } else if let (Want::Write(dest), false) = (want, list_only) {
            copy_limited(&mut open(&path)?, &dest, budget)?;
        }
    }
    Ok(())
}

fn walk_zip(src: &Path, list_only: bool, budget: &mut u64, cb: &mut Visit) -> Result<()> {
    let mut zip = zip::ZipArchive::new(BufReader::new(open(src)?))
        .with_context(|| format!("not a readable zip: {}", src.display()))?;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).with_context(|| format!("zip entry {i}"))?;
        let symlink = f.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000);
        let kind = if symlink {
            Kind::Symlink
        } else if f.is_dir() {
            Kind::Dir
        } else {
            Kind::File
        };
        let want = cb(&Entry {
            path: f.name().to_string(),
            size: f.size(),
            kind,
        })?;
        if let (Want::Write(dest), false) = (want, list_only) {
            copy_limited(&mut f, &dest, budget)?;
        }
    }
    Ok(())
}

fn walk_tar<R: Read>(r: R, list_only: bool, budget: &mut u64, cb: &mut Visit) -> Result<()> {
    let mut tar = tar::Archive::new(r);
    for e in tar.entries().context("not a readable tar archive")? {
        let mut e = e.context("damaged tar archive")?;
        let ty = e.header().entry_type();
        if ty.is_pax_global_extensions() {
            continue;
        }
        let kind = if ty.is_file() {
            Kind::File
        } else if ty.is_dir() {
            Kind::Dir
        } else if ty.is_symlink() || ty.is_hard_link() {
            Kind::Symlink
        } else {
            Kind::Special
        };
        let path = e.path()?.to_string_lossy().into_owned();
        let want = cb(&Entry {
            path,
            size: e.size(),
            kind,
        })?;
        if let (Want::Write(dest), false) = (want, list_only) {
            copy_limited(&mut e, &dest, budget)?;
        }
    }
    Ok(())
}

fn walk_7z(src: &Path, list_only: bool, budget: &mut u64, cb: &mut Visit) -> Result<()> {
    use sevenz_rust2::{ArchiveEntry, ArchiveReader, Password};
    fn kind_of(e: &ArchiveEntry) -> Kind {
        let attr = e.windows_attributes();
        // Bit 15 flags a unix mode in the high 16 bits (p7zip); 0x400 is a Windows reparse point.
        let unix_symlink = attr & 0x8000 != 0 && (attr >> 16) & 0o170000 == 0o120000;
        if unix_symlink || attr & 0x400 != 0 {
            Kind::Symlink
        } else if e.is_directory() {
            Kind::Dir
        } else {
            Kind::File
        }
    }
    let mut reader = ArchiveReader::open(src, Password::empty())
        .map_err(|e| anyhow!("cannot read 7z archive {}: {e}", src.display()))?;
    if list_only {
        for e in &reader.archive().files {
            if e.is_anti_item() {
                continue;
            }
            cb(&Entry {
                path: e.name().to_string(),
                size: e.size(),
                kind: kind_of(e),
            })?;
        }
        return Ok(());
    }
    let mut failure: Option<anyhow::Error> = None;
    let res = reader.for_each_entries(|e, data| {
        if e.is_anti_item() {
            return Ok(true);
        }
        let step = (|| -> Result<()> {
            let want = cb(&Entry {
                path: e.name().to_string(),
                size: e.size(),
                kind: kind_of(e),
            })?;
            match want {
                Want::Write(dest) => copy_limited(data, &dest, budget),
                // A solid block's next entry starts where this one ends.
                Want::Skip => io::copy(data, &mut io::sink())
                    .map(|_| ())
                    .map_err(Into::into),
            }
        })();
        match step {
            Ok(()) => Ok(true),
            Err(e) => {
                failure = Some(e);
                Ok(false)
            }
        }
    });
    if let Some(e) = failure {
        return Err(e);
    }
    res.map_err(|e| anyhow!("cannot read 7z archive {}: {e}", src.display()))
}

#[cfg(feature = "rar")]
fn walk_rar(src: &Path, list_only: bool, budget: &mut u64, cb: &mut Visit) -> Result<()> {
    use std::ffi::c_int;
    use std::io::Write;
    use std::ptr;
    use unrar::error::{Code, UnrarError, When};
    use unrar::{Archive, FileHeader};
    use unrar_sys as native;
    fn entry_of(h: &FileHeader) -> Result<Entry> {
        if h.is_encrypted() {
            bail!("the RAR archive is encrypted");
        }
        if h.is_split() {
            bail!("multi-volume RAR archives are not supported");
        }
        // The attribute is a unix mode for archives made on unix; a symlink is S_IFLNK.
        let symlink = h.file_attr & 0o170000 == 0o120000;
        Ok(Entry {
            path: h.filename.to_string_lossy().into_owned(),
            size: h.unpacked_size,
            kind: if symlink {
                Kind::Symlink
            } else if h.is_directory() {
                Kind::Dir
            } else {
                Kind::File
            },
        })
    }
    let err =
        |e: unrar::error::UnrarError| anyhow!("cannot read RAR archive {}: {e}", src.display());
    let archive = Archive::new(src);
    if list_only {
        for h in archive.open_for_listing().map_err(err)? {
            cb(&entry_of(&h.map_err(err)?)?)?;
        }
        return Ok(());
    }
    // The `unrar` crate can only extract a whole entry to a file or to memory, so the data is taken
    // through UnRAR's own callback instead: the entry is tested (decompressed and checked, nothing
    // written by UnRAR) and the callback writes the chunks itself, stopping at `budget + 1` bytes as
    // `copy_limited` does.
    struct Sink {
        file: File,
        /// Bytes that may still be written; 0 means the entry is over the budget.
        left: u64,
        err: Option<io::Error>,
    }
    extern "C" fn on_data(
        msg: native::UINT,
        sink: native::LPARAM,
        data: native::LPARAM,
        len: native::LPARAM,
    ) -> c_int {
        if msg != native::UCM_PROCESSDATA {
            // A password or a next volume: neither is supported, so stop.
            return -1;
        }
        // SAFETY: `sink` is the `Sink` set with this callback below and outlives the call to
        // `RARProcessFile`; UnRAR gives `len` valid bytes at `data`.
        let sink = unsafe { &mut *(sink as *mut Sink) };
        let data = unsafe { std::slice::from_raw_parts(data as *const u8, len as usize) };
        let take = &data[..data
            .len()
            .min(usize::try_from(sink.left).unwrap_or(usize::MAX))];
        if let Err(e) = sink.file.write_all(take) {
            sink.err = Some(e);
            return -1;
        }
        sink.left -= take.len() as u64;
        if sink.left == 0 { -1 } else { 0 }
    }
    struct Handle(*const native::Handle);
    impl Drop for Handle {
        fn drop(&mut self) {
            // SAFETY: the handle came from `RAROpenArchiveEx` and is closed once.
            unsafe { native::RARCloseArchive(self.0) };
        }
    }
    let code_err = |code: c_int, when: When| {
        err(UnrarError::from(
            Code::from(code).unwrap_or(Code::Unknown),
            when,
        ))
    };

    // The archive name as the `unrar` crate passes it.
    #[cfg(any(target_os = "linux", target_os = "netbsd"))]
    let name = std::ffi::CString::new(src.as_os_str().as_encoded_bytes())?;
    #[cfg(not(any(target_os = "linux", target_os = "netbsd")))]
    let name = widestring::WideCString::from_os_str(src)?;
    let mut data = native::OpenArchiveDataEx::new(name.as_ptr().cast(), native::RAR_OM_EXTRACT);
    // UnRAR writes its results through these pointers although the bindings declare them `*const`,
    // so they are made from mutable places.
    // SAFETY: `data` and `name` live across the call.
    let h = unsafe { native::RAROpenArchiveEx((&raw mut data).cast_const()) };
    let _close = (!h.is_null()).then(|| Handle(h));
    if h.is_null() || data.open_result != 0 {
        return Err(code_err(data.open_result as c_int, When::Open));
    }
    loop {
        let mut header = native::HeaderDataEx::default();
        // SAFETY: `h` is open and `header` lives across the call.
        match unsafe { native::RARReadHeaderEx(h, (&raw mut header).cast_const()) } {
            native::ERAR_SUCCESS => {}
            native::ERAR_END_ARCHIVE => return Ok(()),
            code => return Err(code_err(code, When::Read)),
        }
        let dest = match cb(&entry_of(&FileHeader::from(header))?)? {
            Want::Skip => {
                // SAFETY: `h` is open and no callback is set.
                let code = unsafe {
                    native::RARProcessFile(h, native::RAR_SKIP, ptr::null(), ptr::null())
                };
                if code != native::ERAR_SUCCESS {
                    return Err(code_err(code, When::Process));
                }
                continue;
            }
            Want::Write(dest) => dest,
        };
        let file =
            File::create(&dest).with_context(|| format!("cannot create {}", dest.display()))?;
        let limit = budget.saturating_add(1);
        let mut sink = Sink {
            file,
            left: limit,
            err: None,
        };
        // SAFETY: `sink` outlives the processing call, and the callback is cleared before `sink`
        // goes out of scope.
        let code = unsafe {
            native::RARSetCallback(h, Some(on_data), &mut sink as *mut Sink as native::LPARAM);
            let code = native::RARProcessFile(h, native::RAR_TEST, ptr::null(), ptr::null());
            native::RARSetCallback(h, None, 0);
            code
        };
        if let Some(e) = sink.err {
            return Err(e).with_context(|| format!("cannot extract {}", dest.display()));
        }
        if sink.left == 0 {
            bail!("the archive unpacks to more than the size limit");
        }
        if code != native::ERAR_SUCCESS {
            return Err(code_err(code, When::Process));
        }
        *budget -= limit - sink.left;
    }
}

#[cfg(not(feature = "rar"))]
fn walk_rar(_: &Path, _: bool, _: &mut u64, _: &mut Visit) -> Result<()> {
    bail!("this build of urp has no RAR support (it was built without the `rar` feature)")
}

#[cfg(all(test, feature = "rar"))]
mod tests {
    use super::*;

    /// `testdata/rar_bomb.rar` holds one entry, `bomb.bin`, of `BOMB_SIZE` zero bytes in about 200 bytes
    /// (made by `testdata/make_rar_bomb.py`, because the RAR creator is proprietary).
    const BOMB: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/mods/testdata/rar_bomb.rar"
    );
    const BOMB_SIZE: u64 = 1 + 2050 * 1024;

    /// Walks the fixture, writing its one file to `dest` in a temp folder, and returns the walk result
    /// with the size of the file that reached the disk.
    fn walk_bomb(mut budget: u64) -> (Result<()>, Option<u64>) {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("bomb.bin");
        let res = walk(Path::new(BOMB), Format::Rar, false, &mut budget, &mut |e| {
            Ok(if e.kind == Kind::File {
                Want::Write(dest.clone())
            } else {
                Want::Skip
            })
        });
        (res, std::fs::metadata(&dest).ok().map(|m| m.len()))
    }

    #[test]
    fn rar_fixture_unpacks_in_full_within_budget() {
        let (res, written) = walk_bomb(BOMB_SIZE);
        res.unwrap();
        assert_eq!(written, Some(BOMB_SIZE));
    }

    /// Covers the finding "walk_rar extracts a whole entry before checking the unpack budget": a RAR
    /// that inflates far past the budget must stop at `budget + 1` bytes on disk, as `copy_limited` does
    /// for the other formats.
    #[test]
    fn rar_entry_over_budget_is_not_written_in_full() {
        let budget = 4096;
        let (res, written) = walk_bomb(budget);
        let err = res.expect_err("an entry over the budget must be refused");
        assert!(
            err.to_string().contains("size limit"),
            "unexpected error: {err:#}"
        );
        let written = written.unwrap_or(0);
        assert!(
            written <= budget + 1,
            "{written} bytes reached the disk for a budget of {budget}"
        );
    }
}
