//! L3: stub unpickle plus the namemap walk (port of `savescan.StubUnpickler`, `extract_position`,
//! `check_position`).
//!
//! `find_class` never imports anything: every `GLOBAL` becomes an inert class id, `REDUCE` and
//! `NEWOBJ` build recording stubs, `BUILD` stores the state. The rebuilt graph is walked for the
//! `RollbackLog` entries, and the entries are checked against the game's node names the way
//! `rollback_core(0, on_load=True)` does (rollback.py L939-975).

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::pickle::{Decoder, Op};

/// A node name: a label string, or the `(filename, version, serial)` tuple of an unnamed statement.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Name {
    Str(String),
    Tup(String, i64, i64),
    /// Anything else. Never in a namemap.
    Other,
}

pub type NameMap = HashSet<Name>;

#[derive(Clone, Debug)]
enum V {
    None,
    Bool(bool),
    Int(i128),
    Str(Rc<str>),
    Other,
    Tuple(Rc<[V]>),
    Obj(u32),
    Class(u32),
    Mark,
}

#[derive(Debug, Default)]
struct Stub {
    class: u32,
    args: Vec<V>,
    state: Option<V>,
    items: Vec<V>,
    map: Vec<(V, V)>,
}

#[derive(Debug)]
enum Obj {
    List(Vec<V>),
    Dict(Vec<(V, V)>),
    Set(Vec<V>),
    Stub(Stub),
}

/// The object graph of one pickle.
pub struct Graph {
    objs: Vec<Obj>,
    classes: Vec<(String, String)>,
    root: V,
}

/// Unpickle `data` into stubs. Errors mean the real load would fail too (truncated data, bad opcode,
/// a `REDUCE` on something that is not a class).
pub fn stub_load(data: &[u8]) -> Result<Graph, String> {
    let mut d = Decoder::new(data);
    let mut objs: Vec<Obj> = Vec::new();
    let mut classes: Vec<(String, String)> = Vec::new();
    let mut class_ids: HashMap<(String, String), u32> = HashMap::new();
    let mut memo: HashMap<usize, V> = HashMap::new();
    let mut memo_n = 0usize;
    let mut stack: Vec<V> = Vec::with_capacity(64);

    fn pop(stack: &mut Vec<V>) -> Result<V, String> {
        stack
            .pop()
            .ok_or_else(|| "unpickling stack underflow".to_string())
    }
    fn pop_mark(stack: &mut Vec<V>) -> Result<Vec<V>, String> {
        let i = stack
            .iter()
            .rposition(|v| matches!(v, V::Mark))
            .ok_or_else(|| "MARK missing".to_string())?;
        let items = stack.split_off(i + 1);
        stack.pop();
        Ok(items)
    }
    fn new_obj(objs: &mut Vec<Obj>, o: Obj) -> V {
        objs.push(o);
        V::Obj((objs.len() - 1) as u32)
    }
    let mut class_of = |m: &str, n: &str, classes: &mut Vec<(String, String)>| -> u32 {
        let key = (m.to_string(), n.to_string());
        if let Some(&id) = class_ids.get(&key) {
            return id;
        }
        let id = classes.len() as u32;
        classes.push(key.clone());
        class_ids.insert(key, id);
        id
    };
    fn text(b: &[u8]) -> String {
        String::from_utf8_lossy(b).into_owned()
    }

    let root = loop {
        let op = d.next()?;
        match op {
            Op::Proto(_) | Op::Frame => {}
            Op::Stop => break pop(&mut stack)?,
            Op::Mark => stack.push(V::Mark),
            Op::Pop => {
                if matches!(stack.last(), Some(V::Mark)) {
                    pop_mark(&mut stack)?;
                } else {
                    pop(&mut stack)?;
                }
            }
            Op::PopMark => {
                pop_mark(&mut stack)?;
            }
            Op::Dup => {
                let t = stack.last().cloned().ok_or("unpickling stack underflow")?;
                stack.push(t);
            }
            Op::None => stack.push(V::None),
            Op::Bool(b) => stack.push(V::Bool(b)),
            Op::Int(i) => stack.push(V::Int(i)),
            Op::BigInt | Op::Float | Op::Bytes => stack.push(V::Other),
            Op::Str { bytes, .. } => {
                stack.push(V::Str(Rc::from(String::from_utf8_lossy(bytes).as_ref())))
            }
            Op::Py2Quoted(b) => stack.push(V::Str(Rc::from(String::from_utf8_lossy(&b).as_ref()))),
            Op::Tuple0 => stack.push(V::Tuple(Rc::from(Vec::new()))),
            Op::Tuple1 => {
                let a = pop(&mut stack)?;
                stack.push(V::Tuple(Rc::from(vec![a])));
            }
            Op::Tuple2 => {
                let b = pop(&mut stack)?;
                let a = pop(&mut stack)?;
                stack.push(V::Tuple(Rc::from(vec![a, b])));
            }
            Op::Tuple3 => {
                let c = pop(&mut stack)?;
                let b = pop(&mut stack)?;
                let a = pop(&mut stack)?;
                stack.push(V::Tuple(Rc::from(vec![a, b, c])));
            }
            Op::Tuple => {
                let items = pop_mark(&mut stack)?;
                stack.push(V::Tuple(Rc::from(items)));
            }
            Op::EmptyList => {
                let v = new_obj(&mut objs, Obj::List(Vec::new()));
                stack.push(v);
            }
            Op::List => {
                let items = pop_mark(&mut stack)?;
                let v = new_obj(&mut objs, Obj::List(items));
                stack.push(v);
            }
            Op::EmptyDict => {
                let v = new_obj(&mut objs, Obj::Dict(Vec::new()));
                stack.push(v);
            }
            Op::Dict => {
                let items = pop_mark(&mut stack)?;
                let pairs = items
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|c| (c[0].clone(), c[1].clone()))
                    .collect();
                let v = new_obj(&mut objs, Obj::Dict(pairs));
                stack.push(v);
            }
            Op::EmptySet => {
                let v = new_obj(&mut objs, Obj::Set(Vec::new()));
                stack.push(v);
            }
            Op::FrozenSet => {
                let items = pop_mark(&mut stack)?;
                let v = new_obj(&mut objs, Obj::Set(items));
                stack.push(v);
            }
            Op::Append => {
                let x = pop(&mut stack)?;
                push_items(&mut objs, stack.last(), vec![x]);
            }
            Op::Appends | Op::AddItems => {
                let xs = pop_mark(&mut stack)?;
                push_items(&mut objs, stack.last(), xs);
            }
            Op::SetItem => {
                let v = pop(&mut stack)?;
                let k = pop(&mut stack)?;
                set_items(&mut objs, stack.last(), vec![(k, v)]);
            }
            Op::SetItems => {
                let xs = pop_mark(&mut stack)?;
                let pairs = xs
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|c| (c[0].clone(), c[1].clone()))
                    .collect();
                set_items(&mut objs, stack.last(), pairs);
            }
            Op::Global(m, n) => {
                let id = class_of(&text(m), &text(n), &mut classes);
                stack.push(V::Class(id));
            }
            Op::StackGlobal => {
                let n = pop(&mut stack)?;
                let m = pop(&mut stack)?;
                match (m, n) {
                    (V::Str(m), V::Str(n)) => {
                        let id = class_of(&m, &n, &mut classes);
                        stack.push(V::Class(id));
                    }
                    _ => return Err("STACK_GLOBAL requires str".into()),
                }
            }
            Op::Inst(m, n) => {
                let args = pop_mark(&mut stack)?;
                let class = class_of(&text(m), &text(n), &mut classes);
                let v = new_obj(
                    &mut objs,
                    Obj::Stub(Stub {
                        class,
                        args,
                        ..Stub::default()
                    }),
                );
                stack.push(v);
            }
            Op::Obj => {
                let mut items = pop_mark(&mut stack)?;
                if items.is_empty() {
                    return Err("OBJ without a class".into());
                }
                let V::Class(class) = items.remove(0) else {
                    return Err("OBJ needs a class".into());
                };
                let v = new_obj(
                    &mut objs,
                    Obj::Stub(Stub {
                        class,
                        args: items,
                        ..Stub::default()
                    }),
                );
                stack.push(v);
            }
            Op::Reduce => {
                let args = pop(&mut stack)?;
                let callable = pop(&mut stack)?;
                let V::Class(class) = callable else {
                    return Err("REDUCE callable is not a class".into());
                };
                let args = match args {
                    V::Tuple(t) => t.to_vec(),
                    _ => return Err("REDUCE arguments are not a tuple".into()),
                };
                let v = new_obj(
                    &mut objs,
                    Obj::Stub(Stub {
                        class,
                        args,
                        ..Stub::default()
                    }),
                );
                stack.push(v);
            }
            Op::NewObj | Op::NewObjEx => {
                if matches!(op, Op::NewObjEx) {
                    pop(&mut stack)?;
                }
                pop(&mut stack)?;
                let callable = pop(&mut stack)?;
                let V::Class(class) = callable else {
                    return Err("NEWOBJ class is not a class".into());
                };
                let v = new_obj(
                    &mut objs,
                    Obj::Stub(Stub {
                        class,
                        ..Stub::default()
                    }),
                );
                stack.push(v);
            }
            Op::Build => {
                let state = pop(&mut stack)?;
                if let Some(V::Obj(i)) = stack.last()
                    && let Obj::Stub(s) = &mut objs[*i as usize]
                {
                    s.state = Some(state);
                }
            }
            Op::Put(i) => {
                let t = stack.last().cloned().ok_or("unpickling stack underflow")?;
                memo.insert(i, t);
            }
            Op::Memoize => {
                let t = stack.last().cloned().ok_or("unpickling stack underflow")?;
                memo.insert(memo_n, t);
                memo_n += 1;
            }
            Op::Get(i) => {
                let t = memo
                    .get(&i)
                    .cloned()
                    .ok_or_else(|| format!("memo value not found at index {i}"))?;
                stack.push(t);
            }
            Op::Unsupported(n) => return Err(format!("unsupported opcode {n}")),
        }
    };
    Ok(Graph {
        objs,
        classes,
        root,
    })
}

fn push_items(objs: &mut [Obj], target: Option<&V>, xs: Vec<V>) {
    if let Some(V::Obj(i)) = target {
        match &mut objs[*i as usize] {
            Obj::List(l) | Obj::Set(l) => l.extend(xs),
            Obj::Stub(s) => s.items.extend(xs),
            Obj::Dict(_) => {}
        }
    }
}

fn set_items(objs: &mut [Obj], target: Option<&V>, xs: Vec<(V, V)>) {
    if let Some(V::Obj(i)) = target {
        match &mut objs[*i as usize] {
            Obj::Dict(d) => d.extend(xs),
            Obj::Stub(s) => s.map.extend(xs),
            _ => {}
        }
    }
}

/// A key of a persistent `_seen_*` field: a node name, or `hash64(name)` (Ren'Py 8.4+).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeenKey {
    Name(Name),
    Hash(u64),
}

/// One `Rollback` entry of the saved log.
#[derive(Debug, Clone)]
pub struct RbEntry {
    /// `Context.current`; `None` when the entry has no context.
    pub current: Option<Name>,
    pub return_stack: Vec<Name>,
    pub hard: bool,
    pub checkpoint: bool,
}

/// Node names a save references (`savescan.extract_position`).
#[derive(Debug, Default)]
pub struct Position {
    /// `Context.current` of every `renpy.execution.Context` stub.
    pub contexts: Vec<Option<Name>>,
    /// Entries of every `RollbackLog`, oldest first.
    pub rollbacks: Vec<RbEntry>,
}

fn truthy(v: &Option<V>) -> bool {
    match v {
        Some(V::Bool(b)) => *b,
        Some(V::Int(i)) => *i != 0,
        Some(V::None) | None => false,
        Some(_) => true,
    }
}

impl Graph {
    fn class_name(&self, v: &V) -> Option<&(String, String)> {
        match v {
            V::Obj(i) => match &self.objs[*i as usize] {
                Obj::Stub(s) => Some(&self.classes[s.class as usize]),
                _ => None,
            },
            _ => None,
        }
    }

    fn stub(&self, v: &V) -> Option<&Stub> {
        match v {
            V::Obj(i) => match &self.objs[*i as usize] {
                Obj::Stub(s) => Some(s),
                _ => None,
            },
            _ => None,
        }
    }

    fn dict_get(&self, v: &V, key: &str) -> Option<V> {
        let V::Obj(i) = v else { return None };
        let Obj::Dict(d) = &self.objs[*i as usize] else {
            return None;
        };
        d.iter()
            .rev()
            .find(|(k, _)| matches!(k, V::Str(s) if &**s == key))
            .map(|(_, v)| v.clone())
    }

    /// `_state_of(o).get(key)`: a state is a dict, or a `(dict|None, slots dict)` tuple where the slots win.
    fn state_get(&self, s: &Stub, key: &str) -> Option<V> {
        match s.state.as_ref()? {
            V::Tuple(t) if t.len() == 2 => {
                let dict_or_none = matches!(t[0], V::None)
                    || matches!(&t[0], V::Obj(i) if matches!(self.objs[*i as usize], Obj::Dict(_)));
                if !dict_or_none {
                    return None;
                }
                self.dict_get(&t[1], key)
                    .or_else(|| self.dict_get(&t[0], key))
            }
            d => self.dict_get(d, key),
        }
    }

    fn seq(&self, v: &V) -> Vec<V> {
        match v {
            V::Tuple(t) => t.to_vec(),
            V::Obj(i) => match &self.objs[*i as usize] {
                Obj::List(l) | Obj::Set(l) => l.clone(),
                _ => Vec::new(),
            },
            _ => Vec::new(),
        }
    }

    fn name_of(v: &V) -> Name {
        match v {
            V::Str(s) => Name::Str(s.to_string()),
            V::Tuple(t) if t.len() == 3 => match (&t[0], &t[1], &t[2]) {
                (V::Str(f), V::Int(a), V::Int(b)) => match (i64::try_from(*a), i64::try_from(*b)) {
                    (Ok(a), Ok(b)) => Name::Tup(f.to_string(), a, b),
                    _ => Name::Other,
                },
                _ => Name::Other,
            },
            _ => Name::Other,
        }
    }

    fn opt_name(v: Option<V>) -> Option<Name> {
        match v {
            None | Some(V::None) => None,
            Some(v) => Some(Self::name_of(&v)),
        }
    }

    /// Depth-first over the reachable graph, in the order of `savescan.walk`; `visit` gets each stub once.
    fn walk(&self, mut visit: impl FnMut(&Stub, &(String, String))) {
        let mut seen = vec![false; self.objs.len()];
        let mut stack = vec![self.root.clone()];
        while let Some(x) = stack.pop() {
            match &x {
                V::Tuple(t) => stack.extend(t.iter().cloned()),
                V::Obj(i) => {
                    let i = *i as usize;
                    if seen[i] {
                        continue;
                    }
                    seen[i] = true;
                    match &self.objs[i] {
                        Obj::Stub(s) => {
                            visit(s, &self.classes[s.class as usize]);
                            stack.extend(s.args.iter().cloned());
                            if let Some(st) = &s.state {
                                stack.push(st.clone());
                            }
                            stack.extend(s.items.iter().cloned());
                            stack.extend(s.map.iter().map(|(_, v)| v.clone()));
                        }
                        Obj::List(l) | Obj::Set(l) => stack.extend(l.iter().cloned()),
                        Obj::Dict(d) => stack.extend(d.iter().map(|(_, v)| v.clone())),
                    }
                }
                _ => {}
            }
        }
    }

    /// Node names in the graph of a save's `(roots, log)` pickle.
    pub fn extract_position(&self) -> Position {
        let mut pos = Position::default();
        let mut logs: Vec<Vec<V>> = Vec::new();
        self.walk(|s, (m, n)| {
            if m == "renpy.execution" && n == "Context" {
                pos.contexts
                    .push(Self::opt_name(self.state_get(s, "current")));
            }
            // renpy.python.* in 7.x, renpy.rollback.* in 8.x.
            if n == "RollbackLog" && m.starts_with("renpy.") {
                logs.push(
                    self.state_get(s, "log")
                        .map(|l| self.seq(&l))
                        .unwrap_or_default(),
                );
            }
        });
        for log in logs {
            for rb in log {
                let Some(rb) = self.stub(&rb) else { continue };
                let ctx = self.state_get(rb, "context").and_then(|c| self.stub(&c));
                let current = ctx.and_then(|c| Self::opt_name(self.state_get(c, "current")));
                let return_stack = ctx
                    .and_then(|c| self.state_get(c, "return_stack"))
                    .map(|v| self.seq(&v).iter().map(Self::name_of).collect())
                    .unwrap_or_default();
                pos.rollbacks.push(RbEntry {
                    current,
                    return_stack,
                    hard: truthy(&self.state_get(rb, "hard_checkpoint")),
                    checkpoint: truthy(&self.state_get(rb, "checkpoint")),
                });
            }
        }
        pos
    }

    /// The keys of a `Persistent` field such as `_seen_ever` (dict keys, set or list items).
    pub fn persistent_keys(&self, field: &str) -> Vec<SeenKey> {
        let Some(root) = self.stub(&self.root) else {
            return Vec::new();
        };
        let Some(v) = self.state_get(root, field) else {
            return Vec::new();
        };
        let keys = |v: &V| -> Vec<V> {
            match v {
                V::Obj(i) => match &self.objs[*i as usize] {
                    Obj::Dict(d) => d.iter().map(|(k, _)| k.clone()).collect(),
                    Obj::List(l) | Obj::Set(l) => l.clone(),
                    Obj::Stub(s) => s
                        .map
                        .iter()
                        .map(|(k, _)| k.clone())
                        .chain(s.items.iter().cloned())
                        .collect(),
                },
                V::Tuple(t) => t.to_vec(),
                _ => Vec::new(),
            }
        };
        keys(&v)
            .into_iter()
            .map(|k| match k {
                V::Int(i) => SeenKey::Hash(i as u128 as u64),
                other => SeenKey::Name(Self::name_of(&other)),
            })
            .collect()
    }

    pub fn root_class(&self) -> Option<String> {
        self.class_name(&self.root).map(|(m, n)| format!("{m}.{n}"))
    }

    pub fn field_count(&self) -> usize {
        self.stub(&self.root).map_or(0, |s| match &s.state {
            Some(V::Tuple(t)) if t.len() == 2 => [&t[0], &t[1]]
                .iter()
                .map(|d| match d {
                    V::Obj(i) => match &self.objs[*i as usize] {
                        Obj::Dict(d) => d.len(),
                        _ => 0,
                    },
                    _ => 0,
                })
                .sum(),
            Some(V::Obj(i)) => match &self.objs[*i as usize] {
                Obj::Dict(d) => d.len(),
                _ => 0,
            },
            _ => 0,
        })
    }
}

impl Name {
    /// Python `str(name)`, the input of `hash64`.
    pub fn py_str(&self) -> Option<String> {
        match self {
            Name::Str(s) => Some(s.clone()),
            Name::Tup(f, a, b) => Some(format!("({}, {}, {})", py_repr(f), a, b)),
            Name::Other => None,
        }
    }
}

fn py_repr(s: &str) -> String {
    let q = if s.contains('\'') && !s.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::with_capacity(s.len() + 2);
    out.push(q);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == q => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\x{:02x}", c as u32))
            }
            c => out.push(c),
        }
    }
    out.push(q);
    out
}

/// FNV-1a 64 over the UCS4 code points (`renpy/astsupport.pyx` `hash64`).
pub fn hash64(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for c in s.chars() {
        h ^= c as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Result3 {
    Ok,
    ResumesEarlier,
    LoadFails,
}

impl Result3 {
    pub fn as_str(self) -> &'static str {
        match self {
            Result3::Ok => "ok",
            Result3::ResumesEarlier => "resumes-earlier",
            Result3::LoadFails => "load-fails",
        }
    }
}

/// Verdict of `check_position`.
#[derive(Debug, Clone)]
pub struct Verdict {
    pub result: Result3,
    pub entries: usize,
    pub entries_missing: usize,
    pub contexts_missing: usize,
    pub dropped_entries: Option<usize>,
    pub dropped_checkpoints: usize,
    pub return_stack_broken: usize,
}

/// Mirror `rollback_core(0, on_load=True)`: the first entry from the newest end whose node exists is the
/// stop point; everything newer is discarded.
pub fn check_position(pos: &Position, namemap: &NameMap) -> Verdict {
    let known = |n: &Name| namemap.contains(n);
    let rbs = &pos.rollbacks;
    let mut v = Verdict {
        result: Result3::LoadFails,
        entries: rbs.len(),
        entries_missing: rbs
            .iter()
            .filter(|r| r.current.as_ref().is_some_and(|c| !known(c)))
            .count(),
        contexts_missing: pos
            .contexts
            .iter()
            .filter(|c| c.as_ref().is_some_and(|c| !known(c)))
            .count(),
        dropped_entries: None,
        dropped_checkpoints: 0,
        return_stack_broken: 0,
    };
    let (mut dropped, mut dropped_cp) = (0usize, 0usize);
    for r in rbs.iter().rev() {
        if r.current.as_ref().is_some_and(known) {
            v.result = if dropped == 0 {
                Result3::Ok
            } else {
                Result3::ResumesEarlier
            };
            v.dropped_entries = Some(dropped);
            v.dropped_checkpoints = dropped_cp;
            v.return_stack_broken = r.return_stack.iter().filter(|n| !known(n)).count();
            return v;
        }
        dropped += 1;
        dropped_cp += usize::from(r.hard || r.checkpoint);
    }
    v
}
