//! Pickle opcode decoder and the L1 scan (port of `savescan.scan_opcodes`).
//!
//! The decoder never builds objects. `Decoder::next` yields one [`Op`] per opcode and errors on
//! a truncated or unknown opcode; the scan and the stub unpickler both consume it.

use std::collections::{BTreeMap, BTreeSet, HashMap};

/// One decoded opcode. Text and byte arguments borrow from the input.
#[derive(Debug)]
pub enum Op<'a> {
    Proto(u8),
    Frame,
    Stop,
    Mark,
    Pop,
    PopMark,
    Dup,
    None,
    Bool(bool),
    Int(i128),
    /// A long or int too large for `i64`.
    BigInt,
    Float,
    /// Text. `py2` marks the Python 2 `str` opcodes (`STRING`, `BINSTRING`, `SHORT_BINSTRING`).
    Str { bytes: &'a [u8], py2: bool },
    /// A protocol 0 `STRING` (already unquoted). Counts as a Python 2 string opcode.
    Py2Quoted(Vec<u8>),
    Bytes,
    Tuple0,
    Tuple1,
    Tuple2,
    Tuple3,
    Tuple,
    EmptyList,
    List,
    EmptyDict,
    Dict,
    EmptySet,
    FrozenSet,
    Append,
    Appends,
    SetItem,
    SetItems,
    AddItems,
    Global(&'a [u8], &'a [u8]),
    StackGlobal,
    Inst(&'a [u8], &'a [u8]),
    Obj,
    Reduce,
    Build,
    NewObj,
    NewObjEx,
    Put(usize),
    Get(usize),
    Memoize,
    /// `PERSID`, `BINPERSID`, `EXT1/2/4`, `NEXT_BUFFER`, `READONLY_BUFFER`: the real load fails or needs hooks.
    Unsupported(&'static str),
}

pub struct Decoder<'a> {
    data: &'a [u8],
    pos: usize,
    /// Highest protocol among the opcodes seen so far.
    pub max_op_proto: u8,
}

fn err<T>(msg: impl Into<String>) -> Result<T, String> {
    Err(msg.into())
}

impl<'a> Decoder<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Decoder { data, pos: 0, max_op_proto: 0 }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        if self.data.len() - self.pos < n {
            return err("pickle data was truncated");
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    fn uint(&mut self, n: usize) -> Result<u64, String> {
        let b = self.take(n)?;
        let mut v = 0u64;
        for (i, x) in b.iter().enumerate() {
            v |= (*x as u64) << (8 * i);
        }
        Ok(v)
    }

    fn line(&mut self) -> Result<&'a [u8], String> {
        let rest = &self.data[self.pos..];
        match rest.iter().position(|&c| c == b'\n') {
            Some(i) => {
                self.pos += i + 1;
                Ok(&rest[..i])
            }
            None => err("no newline found when trying to read stream"),
        }
    }

    fn len_bytes(&mut self, n: u64) -> Result<&'a [u8], String> {
        let n = usize::try_from(n).map_err(|_| "length too large".to_string())?;
        self.take(n)
    }

    fn proto(&mut self, p: u8) {
        if p > self.max_op_proto {
            self.max_op_proto = p;
        }
    }

    /// The next opcode, or an error for truncated or unknown input.
    pub fn next(&mut self) -> Result<Op<'a>, String> {
        let code = self.u8()?;
        Ok(match code {
            b'(' => Op::Mark,
            b'.' => Op::Stop,
            b'0' => Op::Pop,
            b'1' => Op::PopMark,
            b'2' => Op::Dup,
            b'F' => {
                self.line()?;
                Op::Float
            }
            b'I' => {
                let l = self.line()?;
                match l {
                    b"00" => Op::Bool(false),
                    b"01" => Op::Bool(true),
                    _ => match std::str::from_utf8(l).ok().and_then(|s| s.trim().parse::<i128>().ok()) {
                        Some(v) => Op::Int(v),
                        None => Op::BigInt,
                    },
                }
            }
            b'J' => {
                self.proto(1);
                Op::Int(self.uint(4)? as u32 as i32 as i128)
            }
            b'K' => {
                self.proto(1);
                Op::Int(self.uint(1)? as i128)
            }
            b'M' => {
                self.proto(1);
                Op::Int(self.uint(2)? as i128)
            }
            b'L' => {
                let l = self.line()?;
                let s = std::str::from_utf8(l).unwrap_or("").trim_end_matches('L');
                match s.parse::<i128>() {
                    Ok(v) => Op::Int(v),
                    Err(_) => Op::BigInt,
                }
            }
            b'N' => Op::None,
            b'P' => {
                self.line()?;
                Op::Unsupported("PERSID")
            }
            b'Q' => {
                self.proto(1);
                Op::Unsupported("BINPERSID")
            }
            b'R' => Op::Reduce,
            b'S' => {
                let l = self.line()?;
                Op::Py2Quoted(unquote_py2(l))
            }
            b'T' => {
                self.proto(1);
                let n = self.uint(4)? as u32 as i32;
                if n < 0 {
                    return err("BINSTRING pickle has negative byte count");
                }
                Op::Str { bytes: self.len_bytes(n as u64)?, py2: true }
            }
            b'U' => {
                self.proto(1);
                let n = self.uint(1)?;
                Op::Str { bytes: self.len_bytes(n)?, py2: true }
            }
            b'V' => {
                let l = self.line()?;
                Op::Str { bytes: l, py2: false }
            }
            b'X' => {
                self.proto(1);
                let n = self.uint(4)?;
                Op::Str { bytes: self.len_bytes(n)?, py2: false }
            }
            b'a' => Op::Append,
            b'b' => Op::Build,
            b'c' => {
                let m = self.line()?;
                let n = self.line()?;
                Op::Global(m, n)
            }
            b'd' => Op::Dict,
            b'}' => {
                self.proto(1);
                Op::EmptyDict
            }
            b'e' => {
                self.proto(1);
                Op::Appends
            }
            b'g' => {
                let l = self.line()?;
                Op::Get(std::str::from_utf8(l).ok().and_then(|s| s.parse().ok()).ok_or("bad GET")?)
            }
            b'h' => {
                self.proto(1);
                Op::Get(self.uint(1)? as usize)
            }
            b'i' => {
                let m = self.line()?;
                let n = self.line()?;
                Op::Inst(m, n)
            }
            b'j' => {
                self.proto(1);
                Op::Get(self.uint(4)? as usize)
            }
            b'l' => Op::List,
            b']' => {
                self.proto(1);
                Op::EmptyList
            }
            b'o' => {
                self.proto(1);
                Op::Obj
            }
            b'p' => {
                let l = self.line()?;
                Op::Put(std::str::from_utf8(l).ok().and_then(|s| s.parse().ok()).ok_or("bad PUT")?)
            }
            b'q' => {
                self.proto(1);
                Op::Put(self.uint(1)? as usize)
            }
            b'r' => {
                self.proto(1);
                Op::Put(self.uint(4)? as usize)
            }
            b's' => Op::SetItem,
            b't' => Op::Tuple,
            b')' => {
                self.proto(1);
                Op::Tuple0
            }
            b'u' => {
                self.proto(1);
                Op::SetItems
            }
            b'G' => {
                self.proto(1);
                self.take(8)?;
                Op::Float
            }
            0x80 => {
                let p = self.u8()?;
                self.proto(2);
                if p > 5 {
                    return err(format!("unsupported pickle protocol: {p}"));
                }
                Op::Proto(p)
            }
            0x81 => {
                self.proto(2);
                Op::NewObj
            }
            0x82 => {
                self.proto(2);
                self.take(1)?;
                Op::Unsupported("EXT1")
            }
            0x83 => {
                self.proto(2);
                self.take(2)?;
                Op::Unsupported("EXT2")
            }
            0x84 => {
                self.proto(2);
                self.take(4)?;
                Op::Unsupported("EXT4")
            }
            0x85 => {
                self.proto(2);
                Op::Tuple1
            }
            0x86 => {
                self.proto(2);
                Op::Tuple2
            }
            0x87 => {
                self.proto(2);
                Op::Tuple3
            }
            0x88 => {
                self.proto(2);
                Op::Bool(true)
            }
            0x89 => {
                self.proto(2);
                Op::Bool(false)
            }
            0x8a => {
                self.proto(2);
                let n = self.uint(1)?;
                self.long(n)?
            }
            0x8b => {
                self.proto(2);
                let n = self.uint(4)?;
                self.long(n)?
            }
            b'B' => {
                self.proto(3);
                let n = self.uint(4)?;
                self.len_bytes(n)?;
                Op::Bytes
            }
            b'C' => {
                self.proto(3);
                let n = self.uint(1)?;
                self.len_bytes(n)?;
                Op::Bytes
            }
            0x8c => {
                self.proto(4);
                let n = self.uint(1)?;
                Op::Str { bytes: self.len_bytes(n)?, py2: false }
            }
            0x8d => {
                self.proto(4);
                let n = self.uint(8)?;
                Op::Str { bytes: self.len_bytes(n)?, py2: false }
            }
            0x8e => {
                self.proto(4);
                let n = self.uint(8)?;
                self.len_bytes(n)?;
                Op::Bytes
            }
            0x8f => {
                self.proto(4);
                Op::EmptySet
            }
            0x90 => {
                self.proto(4);
                Op::AddItems
            }
            0x91 => {
                self.proto(4);
                Op::FrozenSet
            }
            0x92 => {
                self.proto(4);
                Op::NewObjEx
            }
            0x93 => {
                self.proto(4);
                Op::StackGlobal
            }
            0x94 => {
                self.proto(4);
                Op::Memoize
            }
            0x95 => {
                self.proto(4);
                self.take(8)?;
                Op::Frame
            }
            0x96 => {
                self.proto(5);
                let n = self.uint(8)?;
                self.len_bytes(n)?;
                Op::Bytes
            }
            0x97 => {
                self.proto(5);
                Op::Unsupported("NEXT_BUFFER")
            }
            0x98 => {
                self.proto(5);
                Op::Unsupported("READONLY_BUFFER")
            }
            other => return err(format!("invalid load key, {:?}", other as char)),
        })
    }

    fn long(&mut self, n: u64) -> Result<Op<'a>, String> {
        let b = self.len_bytes(n)?;
        if b.is_empty() {
            return Ok(Op::Int(0));
        }
        let neg = b[b.len() - 1] & 0x80 != 0;
        if b.len() > 16 {
            let ext = if neg { 0xff } else { 0 };
            if b[16..].iter().any(|&x| x != ext) || (b[15] & 0x80 != 0) != neg {
                return Ok(Op::BigInt);
            }
        }
        let mut v: u128 = if neg { u128::MAX } else { 0 };
        for (i, x) in b.iter().take(16).enumerate() {
            v &= !(0xffu128 << (8 * i));
            v |= (*x as u128) << (8 * i);
        }
        Ok(Op::Int(v as i128))
    }
}

/// Decode a protocol 0 `S` argument (a quoted Python 2 string literal) to bytes.
pub fn unquote_py2(line: &[u8]) -> Vec<u8> {
    let l = line.trim_ascii();
    if l.len() >= 2 && (l[0] == b'\'' || l[0] == b'"') && l[l.len() - 1] == l[0] {
        let inner = &l[1..l.len() - 1];
        let mut out = Vec::with_capacity(inner.len());
        let mut i = 0;
        while i < inner.len() {
            if inner[i] == b'\\' && i + 1 < inner.len() {
                i += 1;
                match inner[i] {
                    b'n' => out.push(b'\n'),
                    b'r' => out.push(b'\r'),
                    b't' => out.push(b'\t'),
                    b'x' if i + 2 < inner.len() => {
                        let h = std::str::from_utf8(&inner[i + 1..i + 3]).ok().and_then(|s| u8::from_str_radix(s, 16).ok());
                        if let Some(h) = h {
                            out.push(h);
                            i += 2;
                        }
                    }
                    c => out.push(c),
                }
            } else {
                out.push(inner[i]);
            }
            i += 1;
        }
        out
    } else {
        l.to_vec()
    }
}

/// Modules only a Python 2 engine writes (`fix_imports` maps them).
const PY2_MODULES: &[&str] = &["__builtin__", "copy_reg", "cPickle", "cStringIO", "StringIO", "Queue", "exceptions", "UserDict"];

/// Result of the L1 scan. Never an error: a malformed pickle sets `error`.
#[derive(Debug, Default, Clone)]
pub struct Scan {
    pub protocol: Option<u8>,
    /// (module, name) -> count. `None` parts are names the scan could not recover.
    pub globals: BTreeMap<(String, String), u32>,
    pub py2_str_ops: u32,
    pub py2_markers: BTreeSet<String>,
    pub max_op_proto: u8,
    pub error: Option<String>,
    pub stopped: bool,
}

const MAX_KEPT_STR: usize = 1024;

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

pub fn scan_opcodes(data: &[u8]) -> Scan {
    let mut rv = Scan::default();
    let mut d = Decoder::new(data);
    let mut memo: HashMap<usize, Option<String>> = HashMap::new();
    // The last few pushed values (strings only matter), for STACK_GLOBAL. FRAME pushes nothing.
    let mut last: Vec<Option<String>> = Vec::with_capacity(8);
    let mut memo_n = 0usize;
    let add = |rv: &mut Scan, m: String, n: String| {
        *rv.globals.entry((m, n)).or_insert(0) += 1;
    };
    loop {
        let op = match d.next() {
            Ok(op) => op,
            Err(e) => {
                rv.error = Some(e);
                break;
            }
        };
        match op {
            Op::Proto(p) => rv.protocol = Some(p),
            Op::Frame => {}
            Op::Str { bytes, py2 } => {
                if py2 {
                    rv.py2_str_ops += 1;
                }
                last.push(if bytes.len() <= MAX_KEPT_STR { Some(text(bytes)) } else { None });
            }
            Op::Py2Quoted(b) => {
                rv.py2_str_ops += 1;
                last.push(if b.len() <= MAX_KEPT_STR { Some(text(&b)) } else { None });
            }
            Op::Global(m, n) => {
                add(&mut rv, text(m), text(n));
                last.push(None);
            }
            Op::Inst(m, n) => {
                add(&mut rv, text(m), text(n));
                last.push(None);
            }
            Op::StackGlobal => {
                let n = last.pop().flatten();
                let m = last.pop().flatten();
                add(&mut rv, m.unwrap_or_default(), n.unwrap_or_default());
                last.push(None);
            }
            Op::Put(i) => {
                memo.insert(i, last.last().cloned().flatten());
            }
            Op::Memoize => {
                memo.insert(memo_n, last.last().cloned().flatten());
                memo_n += 1;
            }
            Op::Get(i) => last.push(memo.get(&i).cloned().flatten()),
            Op::Stop => {
                rv.stopped = true;
                last.push(None);
            }
            _ => last.push(None),
        }
        if last.len() > 4 {
            let drop = last.len() - 4;
            last.drain(..drop);
        }
        if rv.stopped {
            break;
        }
    }
    rv.max_op_proto = d.max_op_proto;
    for (m, n) in rv.globals.keys() {
        if PY2_MODULES.contains(&m.as_str()) {
            rv.py2_markers.insert(m.clone());
        }
        if m == "_codecs" && n == "encode" {
            rv.py2_markers.insert("_codecs.encode".to_string());
        }
    }
    rv
}
