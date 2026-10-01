//! A Python 2 tokenizer. It keeps byte spans into the source so the rewriter can edit in place.
//!
//! It knows what the rewriter needs: names, numbers (old octal, `L` suffix), strings with every
//! Python 2 prefix (`ur`), operators (`<>`, backtick), comments, line continuations and bracket
//! nesting. A logical line ends at a `Newline` token that is emitted only outside brackets.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Name,
    Num,
    Str,
    Op,
    Newline,
}

#[derive(Clone, Copy, Debug)]
pub struct Tok {
    pub kind: Kind,
    /// Byte span in the source.
    pub s: usize,
    pub e: usize,
    /// 1-based line of the first byte.
    pub line: u32,
    /// 0-based column in characters.
    pub col: u32,
    /// First token of a logical line.
    pub first: bool,
}

/// The source cannot be tokenized (unterminated string, stray backslash).
#[derive(Debug)]
pub struct LexError;

const OPS3: [&[u8]; 5] = [b"**=", b"//=", b">>=", b"<<=", b"..."];
const OPS2: [&[u8]; 19] = [
    b"**", b"//", b">>", b"<<", b"<=", b">=", b"==", b"!=", b"<>", b"+=", b"-=", b"*=", b"/=",
    b"%=", b"&=", b"|=", b"^=", b"->", b":=",
];

fn is_ident_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_' || c >= 0x80
}

fn is_ident(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c >= 0x80
}

fn is_prefix(word: &[u8]) -> bool {
    !word.is_empty()
        && word.len() <= 2
        && word
            .iter()
            .all(|c| matches!(c.to_ascii_lowercase(), b'r' | b'u' | b'b'))
}

/// End of the string that starts at `i` (`i` is at the opening quote).
fn string_end(b: &[u8], i: usize) -> Result<usize, LexError> {
    let q = b[i];
    let triple = b.get(i + 1) == Some(&q) && b.get(i + 2) == Some(&q);
    let mut j = i + if triple { 3 } else { 1 };
    while j < b.len() {
        match b[j] {
            b'\\' => j += 2,
            c if c == q => {
                if !triple {
                    return Ok(j + 1);
                }
                if b.get(j + 1) == Some(&q) && b.get(j + 2) == Some(&q) {
                    return Ok(j + 3);
                }
                j += 1;
            }
            b'\n' if !triple => return Err(LexError),
            _ => j += 1,
        }
    }
    Err(LexError)
}

fn number_end(b: &[u8], i: usize) -> usize {
    let n = b.len();
    let at = |j: usize| if j < n { b[j] } else { 0 };
    let mut j = i;
    let mut integer = true;
    if at(j) == b'0' && matches!(at(j + 1), b'x' | b'X' | b'o' | b'O' | b'b' | b'B') {
        j += 2;
        while at(j).is_ascii_hexdigit() || at(j) == b'_' {
            j += 1;
        }
    } else {
        while at(j).is_ascii_digit() || at(j) == b'_' {
            j += 1;
        }
        if at(j) == b'.' {
            integer = false;
            j += 1;
            while at(j).is_ascii_digit() || at(j) == b'_' {
                j += 1;
            }
        }
        if matches!(at(j), b'e' | b'E') {
            let k = if matches!(at(j + 1), b'+' | b'-') {
                j + 2
            } else {
                j + 1
            };
            if at(k).is_ascii_digit() {
                integer = false;
                j = k;
                while at(j).is_ascii_digit() || at(j) == b'_' {
                    j += 1;
                }
            }
        }
        if matches!(at(j), b'j' | b'J') {
            integer = false;
            j += 1;
        }
    }
    if integer && matches!(at(j), b'l' | b'L') {
        j += 1;
    }
    j
}

pub fn lex(src: &str) -> Result<Vec<Tok>, LexError> {
    let b = src.as_bytes();
    let n = b.len();
    let mut toks: Vec<Tok> = Vec::new();
    let mut i = 0;
    let mut line = 1u32;
    let mut line_start = 0;
    let mut depth = 0i32;
    let mut in_line = false;

    // Advances the line counters over the bytes of a token that may contain newlines.
    macro_rules! skip_lines {
        ($from:expr, $to:expr) => {
            for k in $from..$to {
                if b[k] == b'\n' {
                    line += 1;
                    line_start = k + 1;
                }
            }
        };
    }

    while i < n {
        let c = b[i];
        match c {
            b'\n' => {
                if depth == 0 && in_line {
                    toks.push(Tok {
                        kind: Kind::Newline,
                        s: i,
                        e: i + 1,
                        line,
                        col: col_of(src, line_start, i),
                        first: false,
                    });
                    in_line = false;
                }
                i += 1;
                line += 1;
                line_start = i;
            }
            b' ' | b'\t' | b'\r' | 0x0c => i += 1,
            b'\\' => {
                let mut j = i + 1;
                if b.get(j) == Some(&b'\r') {
                    j += 1;
                }
                if b.get(j) != Some(&b'\n') {
                    return Err(LexError);
                }
                i = j + 1;
                line += 1;
                line_start = i;
            }
            b'#' => {
                while i < n && b[i] != b'\n' {
                    i += 1;
                }
            }
            _ => {
                let s = i;
                let kind;
                if c == b'"' || c == b'\'' {
                    i = string_end(b, i)?;
                    kind = Kind::Str;
                } else if c.is_ascii_digit()
                    || (c == b'.' && i + 1 < n && b[i + 1].is_ascii_digit())
                {
                    i = number_end(b, i);
                    kind = Kind::Num;
                } else if is_ident_start(c) {
                    while i < n && is_ident(b[i]) {
                        i += 1;
                    }
                    if i < n && (b[i] == b'"' || b[i] == b'\'') && is_prefix(&b[s..i]) {
                        i = string_end(b, i)?;
                        kind = Kind::Str;
                    } else {
                        kind = Kind::Name;
                    }
                } else {
                    kind = Kind::Op;
                    let rest = &b[i..];
                    let len = if OPS3.iter().any(|o| rest.starts_with(o)) {
                        3
                    } else if OPS2.iter().any(|o| rest.starts_with(o)) {
                        2
                    } else {
                        1
                    };
                    i += len;
                    match c {
                        b'(' | b'[' | b'{' => depth += 1,
                        b')' | b']' | b'}' => depth = (depth - 1).max(0),
                        _ => {}
                    }
                }
                toks.push(Tok {
                    kind,
                    s,
                    e: i,
                    line,
                    col: col_of(src, line_start, s),
                    first: !in_line,
                });
                in_line = true;
                skip_lines!(s, i);
            }
        }
    }
    if in_line {
        toks.push(Tok {
            kind: Kind::Newline,
            s: n,
            e: n,
            line,
            col: 0,
            first: false,
        });
    }
    Ok(toks)
}

fn col_of(src: &str, line_start: usize, at: usize) -> u32 {
    let seg = &src.as_bytes()[line_start..at];
    if seg.is_ascii() {
        seg.len() as u32
    } else {
        src[line_start..at].chars().count() as u32
    }
}
