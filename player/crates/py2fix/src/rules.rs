//! The rewrite rules. Each handler adds in-place edits (byte ranges replaced by text) and one log
//! entry; none of them adds or removes a newline.

use crate::Rewrite;
use crate::lex::{Kind, Tok, lex};

struct Edit {
    s: usize,
    e: usize,
    text: String,
}

struct Ctx<'a> {
    src: &'a str,
    t: Vec<Tok>,
    edits: Vec<Edit>,
    log: Vec<Rewrite>,
}

/// One rewrite pass over `src`.
pub fn pass(src: &str) -> (String, Vec<Rewrite>) {
    let Ok(t) = lex(src) else {
        return (src.to_string(), Vec::new());
    };
    let mut c = Ctx { src, t, edits: Vec::new(), log: Vec::new() };
    c.run();
    c.finish()
}

const COMPOUND: [&str; 10] = ["if", "for", "while", "try", "with", "def", "class", "else", "elif", "async"];
const NOT_EXPR_START: [&str; 10] = ["if", "else", "in", "is", "and", "or", "for", "as", "from", "import"];

impl<'a> Ctx<'a> {
    fn txt(&self, i: usize) -> &'a str {
        let src: &'a str = self.src;
        &src[self.t[i].s..self.t[i].e]
    }

    fn is_op(&self, i: usize, s: &str) -> bool {
        i < self.t.len() && self.t[i].kind == Kind::Op && self.txt(i) == s
    }

    fn is_name(&self, i: usize, s: &str) -> bool {
        i < self.t.len() && self.t[i].kind == Kind::Name && self.txt(i) == s
    }

    fn ins(&mut self, at: usize, text: &str) {
        self.edits.push(Edit { s: at, e: at, text: text.to_string() });
    }

    fn rep(&mut self, s: usize, e: usize, text: &str) {
        self.edits.push(Edit { s, e, text: text.to_string() });
    }

    fn note(&mut self, i: usize, rule: &'static str) {
        let t = self.t[i];
        self.log.push(Rewrite { line: t.line, col: t.col, rule });
    }

    fn opener(&self, i: usize) -> bool {
        self.t[i].kind == Kind::Op && matches!(self.txt(i), "(" | "[" | "{")
    }

    fn closer(&self, i: usize) -> bool {
        self.t[i].kind == Kind::Op && matches!(self.txt(i), ")" | "]" | "}")
    }

    /// Index of the bracket that closes the opener at `i`.
    fn close_of(&self, i: usize) -> Option<usize> {
        let mut d = 0i32;
        for k in i..self.t.len() {
            if self.opener(k) {
                d += 1;
            } else if self.closer(k) {
                d -= 1;
                if d == 0 {
                    return Some(k);
                }
            }
        }
        None
    }

    /// Index of the token that ends the simple statement which contains `from`: a `Newline`, a
    /// `;` outside brackets, or the token count.
    fn stmt_end(&self, from: usize) -> usize {
        let mut d = 0i32;
        for k in from..self.t.len() {
            match self.t[k].kind {
                Kind::Newline => return k,
                Kind::Op => {
                    if self.opener(k) {
                        d += 1;
                    } else if self.closer(k) {
                        d -= 1;
                    } else if d <= 0 && self.txt(k) == ";" {
                        return k;
                    }
                }
                _ => {}
            }
        }
        self.t.len()
    }

    /// Indices in `from..to` of tokens that satisfy `pred` and sit outside any bracket opened
    /// inside the range.
    fn top_level(&self, from: usize, to: usize, pred: impl Fn(&Self, usize) -> bool) -> Vec<usize> {
        let mut d = 0i32;
        let mut out = Vec::new();
        for k in from..to {
            if self.opener(k) {
                d += 1;
            } else if self.closer(k) {
                d -= 1;
            } else if d == 0 && pred(self, k) {
                out.push(k);
            }
        }
        out
    }

    fn commas(&self, from: usize, to: usize) -> Vec<usize> {
        self.top_level(from, to, |c, k| c.is_op(k, ","))
    }

    /// Parameter ranges `(first token, one past last token)` of a comma list.
    fn split(&self, from: usize, to: usize) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        let mut a = from;
        for c in self.commas(from, to) {
            if c > a {
                out.push((a, c));
            }
            a = c + 1;
        }
        if to > a {
            out.push((a, to));
        }
        out
    }

    /// Whether the token after `print` or `exec` begins an expression, so the name is a statement
    /// keyword there and not a variable.
    fn begins_expr(&self, f: usize) -> bool {
        match self.t[f].kind {
            Kind::Num | Kind::Str => true,
            Kind::Name => !NOT_EXPR_START.contains(&self.txt(f)),
            Kind::Op => matches!(self.txt(f), "(" | "[" | "{" | "-" | "+" | "~" | "`" | ">>" | "..."),
            Kind::Newline => false,
        }
    }

    /// Turns the gap between a keyword and its first argument into `(`.
    fn open_call(&mut self, kw: usize, arg: usize) {
        let (s, e) = (self.t[kw].e, self.t[arg].s);
        let gap = &self.src[s..e];
        if gap.bytes().any(|b| matches!(b, b'\n' | b'\\' | b'#')) {
            self.ins(s, "(");
        } else {
            self.rep(s, e, "(");
        }
    }

    fn run(&mut self) {
        self.tabs();
        let n = self.t.len();
        let mut d = 0i32;
        let mut start = true;
        let mut tick: Option<usize> = None;
        for i in 0..n {
            match self.t[i].kind {
                Kind::Newline => {
                    start = true;
                    continue;
                }
                Kind::Op => match self.txt(i) {
                    "(" | "[" | "{" => d += 1,
                    ")" | "]" | "}" => d -= 1,
                    ";" | ":" if d <= 0 => {
                        start = true;
                        continue;
                    }
                    "<>" => {
                        let (s, e) = (self.t[i].s, self.t[i].e);
                        self.rep(s, e, "!=");
                        self.note(i, "ne-operator");
                    }
                    "`" => {
                        let (s, e) = (self.t[i].s, self.t[i].e);
                        match tick.take() {
                            None => {
                                self.rep(s, e, "repr(");
                                self.note(i, "backtick");
                                tick = Some(i);
                            }
                            Some(_) => self.rep(s, e, ")"),
                        }
                    }
                    _ => {}
                },
                Kind::Num => self.number(i),
                Kind::Str => self.string(i),
                Kind::Name => {
                    let word = self.txt(i);
                    if start {
                        match word {
                            "print" => self.print(i),
                            "exec" => self.exec(i),
                            "raise" => self.raise(i),
                            "except" => self.except(i),
                            "def" => self.def(i),
                            _ => {}
                        }
                    }
                    if word == "lambda" {
                        self.lambda(i);
                    }
                }
            }
            start = false;
        }
    }

    fn finish(mut self) -> (String, Vec<Rewrite>) {
        self.edits.sort_by_key(|e| (e.s, e.e));
        let mut out = String::with_capacity(self.src.len() + 64);
        let mut cur = 0;
        for e in &self.edits {
            if e.s < cur {
                continue;
            }
            out.push_str(&self.src[cur..e.s]);
            out.push_str(&e.text);
            cur = e.e;
        }
        out.push_str(&self.src[cur..]);
        (out, self.log)
    }

    fn number(&mut self, i: usize) {
        let (s, e) = (self.t[i].s, self.t[i].e);
        let mut body = self.txt(i);
        if body.ends_with(['l', 'L']) {
            self.rep(e - 1, e, "");
            self.note(i, "long-suffix");
            body = &body[..body.len() - 1];
        }
        let b = body.as_bytes();
        if b.len() > 1 && b[0] == b'0' && b.iter().all(|c| (b'0'..=b'7').contains(c)) && b.iter().any(|&c| c != b'0') {
            self.ins(s + 1, "o");
            self.note(i, "octal");
        }
    }

    fn string(&mut self, i: usize) {
        let s = self.t[i].s;
        let text = self.txt(i);
        let prefix_len = text.find(['"', '\'']).unwrap_or(0);
        if prefix_len == 2 && text[..2].eq_ignore_ascii_case("ur") {
            self.rep(s, s + 1, "");
            self.note(i, "ur-prefix");
        }
    }

    fn print(&mut self, i: usize) {
        let end = self.stmt_end(i + 1);
        let f = i + 1;
        if f == end {
            let at = self.t[i].e;
            self.ins(at, "()");
            self.note(i, "print");
            return;
        }
        if !self.begins_expr(f) {
            return;
        }
        if self.is_op(f, "(") && self.close_of(f) == Some(end - 1) {
            return;
        }
        let chevron = self.is_op(f, ">>");
        let mut args = f;
        let mut file = None;
        if chevron {
            let comma = self.commas(f + 1, end).first().copied();
            let fe = comma.unwrap_or(end);
            if fe == f + 1 {
                return;
            }
            file = Some(self.src[self.t[f + 1].s..self.t[fe - 1].e].to_string());
            args = comma.map_or(end, |c| c + 1);
        }
        let trailing = end > args && self.commas(args, end).last() == Some(&(end - 1));
        let argend = if trailing { end - 1 } else { end };
        let mut tail: Vec<String> = Vec::new();
        if trailing {
            tail.push("end=\" \"".to_string());
        }
        if let Some(fl) = file {
            tail.push(format!("file={fl}"));
        }
        let tail = tail.join(", ");
        let last_end = self.t[end - 1].e;
        if args >= argend {
            // `print >>f` and `print >>f,`: nothing to print but the options.
            let from = self.t[i].e;
            self.rep(from, last_end, &format!("({tail})"));
        } else {
            if chevron {
                let (from, to) = (self.t[i].e, self.t[args].s);
                self.rep(from, to, "(");
            } else {
                self.open_call(i, args);
            }
            if trailing {
                let (s, e) = (self.t[end - 1].s, self.t[end - 1].e);
                self.rep(s, e, "");
            }
            let close = if tail.is_empty() { ")".to_string() } else { format!(", {tail})") };
            self.ins(last_end, &close);
        }
        self.note(i, "print");
    }

    fn exec(&mut self, i: usize) {
        let end = self.stmt_end(i + 1);
        let f = i + 1;
        if f == end || !self.begins_expr(f) || self.is_op(f, ">>") {
            return;
        }
        if self.is_op(f, "(") && self.close_of(f) == Some(end - 1) {
            return;
        }
        let inn = self.top_level(f, end, |c, k| c.is_name(k, "in")).first().copied();
        self.open_call(i, f);
        if let Some(k) = inn {
            let (s, e) = (self.t[k].s, self.t[k].e);
            self.rep(s, e, ",");
        }
        let at = self.t[end - 1].e;
        self.ins(at, ")");
        self.note(i, "exec");
    }

    fn raise(&mut self, i: usize) {
        let end = self.stmt_end(i + 1);
        let cm = self.commas(i + 1, end);
        if cm.is_empty() || cm.len() > 2 || cm[0] == i + 1 {
            return;
        }
        let c1 = cm[0];
        let v_end = if cm.len() == 2 { cm[1] } else { end };
        if v_end <= c1 + 1 || (cm.len() == 2 && end <= cm[1] + 1) {
            return;
        }
        let (s, e) = (self.t[c1].s, self.t[c1].e);
        self.rep(s, e, "(");
        if self.is_op(c1 + 1, "(")
            && self.close_of(c1 + 1) == Some(v_end - 1)
            && !self.commas(c1 + 2, v_end - 1).is_empty()
        {
            let at = self.t[c1 + 1].s;
            self.ins(at, "*");
        }
        let at = self.t[v_end - 1].e;
        self.ins(at, ")");
        if cm.len() == 2 {
            let (s, e) = (self.t[cm[1]].s, self.t[cm[1]].e);
            self.rep(s, e, ".with_traceback(");
            let at = self.t[end - 1].e;
            self.ins(at, ")");
        }
        self.note(i, "raise-comma");
    }

    fn except(&mut self, i: usize) {
        let mut d = 0i32;
        let mut colon = None;
        for k in i + 1..self.t.len() {
            if self.t[k].kind == Kind::Newline {
                break;
            }
            if self.opener(k) {
                d += 1;
            } else if self.closer(k) {
                d -= 1;
            } else if d == 0 && self.is_op(k, ":") {
                colon = Some(k);
                break;
            }
        }
        let Some(colon) = colon else { return };
        let cm = self.commas(i + 1, colon);
        if cm.len() != 1 {
            return;
        }
        let c = cm[0];
        if c + 2 == colon && self.t[c + 1].kind == Kind::Name {
            let (s, e) = (self.t[c].s, self.t[c + 1].s);
            self.rep(s, e, " as ");
            self.note(i, "except-comma");
        }
    }

    /// The parameters that are a whole parenthesized group, like Python 2's `(a, b)`.
    fn tuple_params(&self, params: &[(usize, usize)]) -> Vec<(usize, usize)> {
        params
            .iter()
            .copied()
            .filter(|&(a, b)| self.is_op(a, "(") && self.close_of(a) == Some(b - 1))
            .collect()
    }

    fn def(&mut self, i: usize) {
        if !(i + 2 < self.t.len() && self.t[i + 1].kind == Kind::Name && self.is_op(i + 2, "(")) {
            return;
        }
        let open = i + 2;
        let Some(close) = self.close_of(open) else { return };
        let tuples = self.tuple_params(&self.split(open + 1, close));
        if tuples.is_empty() || !self.is_op(close + 1, ":") {
            return;
        }
        let colon = close + 1;
        let at = if self.t[colon + 1].kind == Kind::Newline {
            let b = colon + 2;
            if b >= self.t.len()
                || self.is_op(b, "@")
                || (self.t[b].kind == Kind::Name && COMPOUND.contains(&self.txt(b)))
            {
                return;
            }
            self.t[b].s
        } else {
            self.t[colon + 1].s
        };
        let mut prefix = String::new();
        for (k, &(a, b)) in tuples.iter().enumerate() {
            let name = format!("__py2p{k}");
            let (s, e) = (self.t[a].s, self.t[b - 1].e);
            prefix.push_str(&format!("{} = {name}; ", &self.src[s..e]));
            self.rep(s, e, &name);
        }
        self.ins(at, &prefix);
        self.note(i, "tuple-param");
    }

    /// One past the last token of the body of the lambda whose colon is at `colon`.
    fn lambda_end(&self, colon: usize) -> usize {
        let mut d = 0i32;
        for k in colon + 1..self.t.len() {
            match self.t[k].kind {
                Kind::Newline => return k,
                Kind::Name if d == 0 && self.txt(k) == "for" => return k,
                Kind::Op => {
                    if self.opener(k) {
                        d += 1;
                    } else if self.closer(k) {
                        if d == 0 {
                            return k;
                        }
                        d -= 1;
                    } else if d == 0 && matches!(self.txt(k), "," | ";" | ":") {
                        return k;
                    }
                }
                _ => {}
            }
        }
        self.t.len()
    }

    fn lambda(&mut self, i: usize) {
        let mut d = 0i32;
        let mut colon = None;
        for k in i + 1..self.t.len() {
            if self.t[k].kind == Kind::Newline {
                break;
            }
            if self.opener(k) {
                d += 1;
            } else if self.closer(k) {
                d -= 1;
            } else if d == 0 && self.is_op(k, ":") {
                colon = Some(k);
                break;
            }
        }
        let Some(colon) = colon else { return };
        let tuples = self.tuple_params(&self.split(i + 1, colon));
        let body_end = self.lambda_end(colon);
        if tuples.is_empty() || body_end <= colon + 1 {
            return;
        }
        let (mut pre, mut post) = (String::new(), String::new());
        for (k, &(a, b)) in tuples.iter().enumerate() {
            let name = format!("__py2l{k}");
            let (s, e) = (self.t[a].s, self.t[b - 1].e);
            let inner = if b - 1 > a + 1 { self.src[self.t[a + 1].s..self.t[b - 2].e].to_string() } else { String::new() };
            let single = b - 1 == a + 2 && self.t[a + 1].kind == Kind::Name;
            if single {
                self.rep(s, e, &inner);
                continue;
            }
            self.rep(s, e, &name);
            pre.push_str(&format!(" (lambda {inner}:"));
            post = format!(")(*{name})") + &post;
        }
        if !pre.is_empty() {
            let (c_end, l_end) = (self.t[colon].e, self.t[body_end - 1].e);
            self.ins(c_end, &pre);
            self.ins(l_end, &post);
        }
        self.note(i, "tuple-lambda");
    }

    /// Python 2 reads a tab as up to the next multiple of 8 columns; Python 3 rejects a file that
    /// mixes both ways of indenting. Such a file gets spaces only, at the same columns.
    fn tabs(&mut self) {
        let mut leads: Vec<(usize, usize, usize)> = Vec::new();
        let (mut any_tab, mut any_space) = (false, false);
        for i in 0..self.t.len() {
            if !self.t[i].first {
                continue;
            }
            let at = self.t[i].s;
            let ls = self.src[..at].rfind('\n').map_or(0, |p| p + 1);
            let lead = &self.src[ls..at];
            if lead.is_empty() || !lead.bytes().all(|c| matches!(c, b' ' | b'\t' | 0x0c)) {
                continue;
            }
            any_tab |= lead.contains('\t');
            any_space |= lead.contains(' ');
            leads.push((i, ls, at));
        }
        if !(any_tab && any_space) {
            return;
        }
        for (i, ls, at) in leads {
            let lead = &self.src[ls..at];
            if !lead.contains('\t') {
                continue;
            }
            let mut col = 0;
            for c in lead.bytes() {
                col = match c {
                    b'\t' => (col / 8 + 1) * 8,
                    b' ' => col + 1,
                    _ => 0,
                };
            }
            self.rep(ls, at, &" ".repeat(col));
            let t = self.t[i];
            self.log.push(Rewrite { line: t.line, col: 0, rule: "tabs" });
        }
    }
}
