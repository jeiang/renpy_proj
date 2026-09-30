//! Python 2 to Python 3 syntax fixer for Ren'Py 7 game code. Contract: player/CONTRACTS.md,
//! "Syntax fixer (`py2fix`)".
//!
//! A token-level rewrite: the source is tokenized the way Python 2 does, then only the tokens of
//! Python 2-only constructs are edited in place. Nothing is added or removed across a newline, so
//! every line keeps its number and tracebacks stay right. If the source cannot be tokenized the
//! result is the source unchanged with no rewrites, and the caller keeps the original `SyntaxError`.
//!
//! Rules (the `rule` of each rewrite): `print`, `exec`, `backtick`, `ne-operator`, `ur-prefix`,
//! `octal`, `long-suffix`, `except-comma`, `raise-comma`, `tuple-param`, `tuple-lambda`, `tabs`.

mod lex;
mod rules;

#[cfg(feature = "python")]
mod pymod;
#[cfg(feature = "python")]
pub use pymod::inittab;

/// One applied rewrite. `line` is 1-based, `col` is the 0-based character column of the first
/// token the rule changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rewrite {
    pub line: u32,
    pub col: u32,
    pub rule: &'static str,
}

/// Rewrites Python 2-only syntax in `source`. Returns the new source and the rewrites applied.
pub fn fix(source: &str) -> (String, Vec<Rewrite>) {
    let mut cur = source.to_string();
    let mut all = Vec::new();
    // A second pass is only needed for nested tuple parameters of lambdas.
    for _ in 0..8 {
        let (out, log) = rules::pass(&cur);
        if log.is_empty() {
            break;
        }
        let again = log.iter().any(|r| r.rule == "tuple-lambda");
        cur = out;
        all.extend(log);
        if !again {
            break;
        }
    }
    (cur, all)
}
