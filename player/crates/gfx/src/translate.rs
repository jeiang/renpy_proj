//! Ren'Py GLSL 1.20 / ESSL 1.00 (as assembled by `gl2shadercache.source()`) to Vulkan-flavoured GLSL 450 that
//! naga's GLSL front end accepts. Token-level rewriter, no GLSL AST. Design: research/renderer/README.md section 4.

use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq)]
pub enum Tok {
    Id(String),
    Num(String),
    Punct(String),
    Pp(String), // whole preprocessor line
}
#[derive(Clone, Debug)]
pub struct T {
    pub t: Tok,
    pub line: usize,
}

pub fn lex(src: &str) -> Vec<T> {
    let b: Vec<char> = src.chars().collect();
    let (mut i, mut line) = (0usize, 1usize);
    let mut out = vec![];
    let mut bol = true;
    while i < b.len() {
        let c = b[i];
        if c == '\n' {
            line += 1;
            i += 1;
            bol = true;
            continue;
        }
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '/' && i + 1 < b.len() && b[i + 1] == '/' {
            while i < b.len() && b[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && i + 1 < b.len() && b[i + 1] == '*' {
            i += 2;
            while i + 1 < b.len() && !(b[i] == '*' && b[i + 1] == '/') {
                if b[i] == '\n' {
                    line += 1;
                }
                i += 1;
            }
            i += 2;
            continue;
        }
        if c == '#' && bol {
            let s = i;
            while i < b.len() && b[i] != '\n' {
                i += 1;
            }
            out.push(T {
                t: Tok::Pp(b[s..i].iter().collect()),
                line,
            });
            continue;
        }
        bol = false;
        if c.is_alphabetic() || c == '_' {
            let s = i;
            while i < b.len() && (b[i].is_alphanumeric() || b[i] == '_') {
                i += 1;
            }
            out.push(T {
                t: Tok::Id(b[s..i].iter().collect()),
                line,
            });
            continue;
        }
        if c.is_ascii_digit() || (c == '.' && i + 1 < b.len() && b[i + 1].is_ascii_digit()) {
            let s = i;
            while i < b.len()
                && (b[i].is_alphanumeric()
                    || b[i] == '.'
                    || ((b[i] == '+' || b[i] == '-') && matches!(b[i - 1], 'e' | 'E')))
            {
                i += 1;
            }
            out.push(T {
                t: Tok::Num(b[s..i].iter().collect()),
                line,
            });
            continue;
        }
        let two: String = b[i..(i + 2).min(b.len())].iter().collect();
        if [
            "==", "!=", "<=", ">=", "&&", "||", "^^", "+=", "-=", "*=", "/=", "++", "--", "<<",
            ">>",
        ]
        .contains(&two.as_str())
        {
            out.push(T {
                t: Tok::Punct(two),
                line,
            });
            i += 2;
        } else {
            out.push(T {
                t: Tok::Punct(c.to_string()),
                line,
            });
            i += 1;
        }
    }
    out
}

#[derive(Clone, Debug, PartialEq)]
pub struct Decl {
    pub storage: String, // uniform | attribute | varying
    pub ty: String,
    pub name: String,
    pub array: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Stage {
    Vertex,
    Fragment,
}

/// Names that exist as built-ins in GLSL 4.50 but not in 1.20/ESSL 1.00 (a game may define its own),
/// plus 4.50 keywords/reserved words that were legal identifiers in 1.20.
const RESERVED: &[&str] = &[
    "round",
    "roundEven",
    "trunc",
    "sinh",
    "cosh",
    "tanh",
    "asinh",
    "acosh",
    "atanh",
    "isnan",
    "isinf",
    "inverse",
    "determinant",
    "modf",
    "frexp",
    "ldexp",
    "fma",
    "texture",
    "textureLod",
    "textureProj",
    "textureGrad",
    "texelFetch",
    "textureSize",
    "textureOffset",
    "packHalf2x16",
    "unpackHalf2x16",
    "bitCount",
    "findLSB",
    "findMSB",
    "clamp_",
    "mix_",
    "sample",
    "patch",
    "subroutine",
    "common",
    "partition",
    "active",
    "filter",
    "input",
    "output",
    "noperspective",
    "smooth",
    "flat",
    "layout",
    "coherent",
    "volatile",
    "restrict",
    "readonly",
    "writeonly",
    "resource",
    "shared",
    "buffer",
    "atomic_uint",
    "uint",
    "uvec2",
    "uvec3",
    "uvec4",
    "double",
    "dvec2",
    "dvec3",
    "dvec4",
    "precise",
    "sampler",
    "image2D",
    "texture2D",
    "row_major",
    "column_major",
    "packed",
    "goto",
    "inline",
    "noinline",
    "public",
    "static",
    "extern",
    "external",
    "interface",
    "long",
    "short",
    "half",
    "fixed",
    "unsigned",
    "superp",
    "template",
    "this",
    "namespace",
    "using",
    "union",
    "enum",
    "typedef",
    "sizeof",
    "cast",
    "asm",
    "class",
    "hvec2",
    "fvec2",
    "min3",
    "max3",
    "median3",
    "abs_",
];
const TEX_FUNCS: &[(&str, &str)] = &[
    ("texture2D", "texture"),
    ("texture2DLod", "textureLod"),
    ("texture2DProj", "textureProj"),
    ("texture2DProjLod", "textureProjLod"),
];

#[derive(Debug)]
pub struct Translated {
    pub glsl: String,
    pub decls: Vec<Decl>,
}

pub struct Program {
    pub vs: String,
    pub fs: String,
    pub uniforms: Vec<Decl>, // non-sampler, std140 block order
    pub samplers: Vec<String>,
    pub attributes: Vec<(String, Decl, u32)>,
    pub varyings: Vec<(String, u32)>,
}

fn parse_decl(toks: &[T]) -> Result<Decl, String> {
    let words: Vec<String> = toks
        .iter()
        .map(|t| match &t.t {
            Tok::Id(s) | Tok::Num(s) | Tok::Punct(s) => s.clone(),
            _ => String::new(),
        })
        .collect();
    let mut i = 0;
    if words[i] == "invariant" {
        i += 1;
    }
    let storage = words[i].clone();
    i += 1;
    if matches!(words[i].as_str(), "highp" | "mediump" | "lowp") {
        i += 1;
    }
    let ty = words[i].clone();
    i += 1;
    let mut array = None;
    if words.get(i).map(|s| s.as_str()) == Some("[") {
        array = words[i + 1].parse().ok();
        i += 3;
    }
    let name = words.get(i).ok_or("no name")?.clone();
    i += 1;
    if words.get(i).map(|s| s.as_str()) == Some("[") {
        array = words[i + 1].parse().ok();
        i += 3;
    }
    if i != words.len() {
        return Err(format!("spurious tokens in declaration of {name}"));
    }
    Ok(Decl {
        storage,
        ty,
        name,
        array,
    })
}

struct Split {
    decls: Vec<Decl>,
    body: Vec<T>,
}

fn split(toks: Vec<T>) -> Result<Split, String> {
    let (mut decls, mut body) = (vec![], vec![]);
    let (mut i, mut depth) = (0, 0i32);
    while i < toks.len() {
        if let Tok::Pp(p) = &toks[i].t {
            let w = p.trim_start_matches('#').trim();
            if w.starts_with("version") || w.starts_with("extension") {
                i += 1;
                continue;
            }
        }
        if depth == 0
            && let Tok::Id(s) = &toks[i].t
        {
            if matches!(
                s.as_str(),
                "uniform" | "attribute" | "varying" | "invariant"
            ) {
                let mut j = i;
                while j < toks.len() && toks[j].t != Tok::Punct(";".into()) {
                    j += 1;
                }
                decls.push(parse_decl(&toks[i..j])?);
                i = j + 1;
                continue;
            }
            if s == "precision" {
                while i < toks.len() && toks[i].t != Tok::Punct(";".into()) {
                    i += 1;
                }
                i += 1;
                continue;
            }
        }
        if toks[i].t == Tok::Punct("{".into()) {
            depth += 1;
        }
        if toks[i].t == Tok::Punct("}".into()) {
            depth -= 1;
        }
        body.push(toks[i].clone());
        i += 1;
    }
    Ok(Split { decls, body })
}

pub fn is_id(t: &T, s: &str) -> bool {
    matches!(&t.t, Tok::Id(x) if x == s)
}
fn is_p(t: &T, s: &str) -> bool {
    matches!(&t.t, Tok::Punct(x) if x == s)
}

fn emit(toks: &[T], start_line: usize) -> String {
    let mut s = String::new();
    let mut line = start_line;
    for t in toks {
        while line < t.line {
            s.push('\n');
            line += 1;
        }
        match &t.t {
            Tok::Id(x) | Tok::Num(x) | Tok::Punct(x) => {
                s.push_str(x);
                s.push(' ');
            }
            Tok::Pp(x) => {
                s.push_str(x);
                s.push('\n');
                line += 1;
            }
        }
    }
    s
}

fn matching_paren(toks: &[T], open: usize) -> usize {
    let mut d = 0;
    for (j, t) in toks.iter().enumerate().skip(open) {
        if is_p(t, "(") {
            d += 1;
        }
        if is_p(t, ")") {
            d -= 1;
            if d == 0 {
                return j;
            }
        }
    }
    toks.len() - 1
}

pub const NUMERIC_TYPES: &[&str] = &[
    "float", "vec2", "vec3", "vec4", "int", "ivec2", "ivec3", "ivec4", "bool", "bvec2", "bvec3",
    "bvec4", "mat2", "mat3", "mat4",
];

pub fn translate(vs: &str, fs: &str) -> Result<Program, String> {
    let v = split(lex(vs))?;
    let f = split(lex(fs))?;

    // Like glGetUniformLocation / glGetAttribLocation, ignore declared variables that no code uses.
    let used: BTreeSet<&str> = v
        .body
        .iter()
        .chain(f.body.iter())
        .filter_map(|t| match &t.t {
            Tok::Id(s) => Some(s.as_str()),
            _ => None,
        })
        .collect();

    // Union of declarations.
    let mut uniforms: BTreeMap<String, Decl> = BTreeMap::new();
    let mut samplers: BTreeSet<String> = BTreeSet::new();
    let mut attrs: BTreeMap<String, Decl> = BTreeMap::new();
    let mut vary: BTreeMap<String, Decl> = BTreeMap::new();
    for d in v.decls.iter().chain(f.decls.iter()) {
        if d.storage != "varying" && !used.contains(d.name.as_str()) {
            continue;
        }
        match d.storage.as_str() {
            "uniform" if d.ty == "sampler2D" => {
                samplers.insert(d.name.clone());
            }
            "uniform" => {
                uniforms.insert(d.name.clone(), d.clone());
            }
            "attribute" => {
                attrs.insert(d.name.clone(), d.clone());
            }
            "varying" => {
                vary.insert(d.name.clone(), d.clone());
            }
            _ => unreachable!(),
        }
    }
    // Link check the way glLinkProgram does: fragment varyings must be declared by the vertex shader.
    for d in f.decls.iter().filter(|d| d.storage == "varying") {
        if !v
            .decls
            .iter()
            .any(|x| x.storage == "varying" && x.name == d.name)
        {
            return Err(format!(
                "link error: fragment varying {} not declared in vertex shader",
                d.name
            ));
        }
    }
    // Ren'Py sources arrive with the variable lines sorted by name; keep a stable order.
    let mut prog = Program {
        vs: String::new(),
        fs: String::new(),
        uniforms: vec![],
        samplers: samplers.iter().cloned().collect(),
        attributes: vec![],
        varyings: vec![],
    };

    // Reserved per-pass uniforms appended by the translator (not visible to game shaders).
    let mut block = String::from("layout(std140, set = 0, binding = 0) uniform RenpyUniforms {\n");
    for d in uniforms.values() {
        let ty = match d.ty.as_str() {
            "mat2" => "vec4".to_string(),
            "bool" => "int".to_string(),
            "bvec2" => "ivec2".into(),
            "bvec3" => "ivec3".into(),
            "bvec4" => "ivec4".into(),
            t => t.to_string(),
        };
        let arr = d.array.map(|n| format!("[{n}]")).unwrap_or_default();
        // Tightly packed element types (float/int/bool/vec2...) cannot be array elements in a WebGPU uniform buffer
        // (stride must be 16; measured: Metal silently drops draws with `float u[5]`), so each element gets a vec4 slot.
        let ty = if d.array.is_some() && matches!(d.ty.as_str(), "float" | "vec2") {
            "vec4".to_string()
        } else if d.array.is_some() && matches!(d.ty.as_str(), "int" | "ivec2" | "bool" | "bvec2") {
            "ivec4".to_string()
        } else {
            ty
        };
        block.push_str(&format!("    {ty} {}{arr};\n", d.name));
        prog.uniforms.push(d.clone());
    }
    block.push_str("    float renpy_flip_y;\n    vec2 renpy_target_size;\n};\n");
    let mut decls_glsl = block.clone();
    for (i, s) in samplers.iter().enumerate() {
        decls_glsl.push_str(&format!("layout(set = 1, binding = {}) uniform texture2D {s}_t;\nlayout(set = 1, binding = {}) uniform sampler {s}_s;\n", 2 * i, 2 * i + 1));
    }
    for (i, (n, d)) in attrs.iter().enumerate() {
        prog.attributes.push((n.clone(), d.clone(), i as u32));
    }
    for (i, n) in vary.keys().enumerate() {
        prog.varyings.push((n.clone(), i as u32));
    }

    // User-defined global function names -> mangled (avoids 4.50 built-in redefinition).
    let mut user_fns: BTreeSet<String> = BTreeSet::new();
    for body in [&v.body, &f.body] {
        let mut depth = 0;
        for i in 0..body.len() {
            if is_p(&body[i], "{") {
                depth += 1;
            }
            if is_p(&body[i], "}") {
                depth -= 1;
            }
            if depth == 0
                && i >= 2
                && is_p(&body[i], "(")
                && let (Tok::Id(name), Tok::Id(_)) = (&body[i - 1].t, &body[i - 2].t)
                && name != "main"
            {
                user_fns.insert(name.clone());
            }
        }
    }

    for (stage, split, out) in [
        (Stage::Vertex, &v, &mut prog.vs),
        (Stage::Fragment, &f, &mut prog.fs),
    ] {
        let mut g = String::from("#version 450\n");
        g.push_str(&decls_glsl);
        for (n, d, loc) in &prog.attributes {
            if stage == Stage::Vertex {
                g.push_str(&format!(
                    "layout(location = {loc}) in {} {n}{};\n",
                    d.ty,
                    d.array.map(|a| format!("[{a}]")).unwrap_or_default()
                ));
            }
        }
        for (n, loc) in &prog.varyings {
            let d = &vary[n];
            let arr = d.array.map(|a| format!("[{a}]")).unwrap_or_default();
            match stage {
                Stage::Vertex => g.push_str(&format!(
                    "layout(location = {loc}) out {} {n}{arr};\n",
                    d.ty
                )),
                Stage::Fragment => {
                    if f.decls
                        .iter()
                        .any(|x| x.storage == "varying" && &x.name == n)
                    {
                        g.push_str(&format!("layout(location = {loc}) in {} {n}{arr};\n", d.ty))
                    }
                }
            }
        }
        if stage == Stage::Fragment {
            g.push_str("layout(location = 0) out vec4 renpy_FragColor;\nvec4 renpy_FragCoord;\n");
        }
        let cx = Cx {
            stage,
            samplers: &samplers,
            uniforms: &uniforms,
            user_fns: &user_fns,
        };
        let spec = specialize_samplers(&split.body, &samplers)?;
        let o = rewrite(&spec, &cx)?;
        g.push_str(&emit(&o, 1));
        // Epilogue: GL clip space -> WebGPU (z in [0,1]); flip y for render-to-texture passes.
        match stage {
            Stage::Vertex => g.push_str("\nvoid main() {\n    renpy_main();\n    gl_Position.z = (gl_Position.z + gl_Position.w) * 0.5;\n    gl_Position.y = gl_Position.y * renpy_flip_y;\n}\n"),
            Stage::Fragment => g.push_str("\nvoid main() {\n    renpy_FragCoord = vec4(gl_FragCoord.x, renpy_flip_y > 0.0 ? renpy_target_size.y - gl_FragCoord.y : gl_FragCoord.y, gl_FragCoord.z, gl_FragCoord.w);\n    renpy_main();\n}\n"),
        }
        *out = g;
    }
    Ok(prog)
}

struct Cx<'a> {
    stage: Stage,
    samplers: &'a BTreeSet<String>,
    uniforms: &'a BTreeMap<String, Decl>,
    user_fns: &'a BTreeSet<String>,
}

fn id(s: &str, line: usize) -> T {
    T {
        t: Tok::Id(s.into()),
        line,
    }
}
fn pu(s: &str, line: usize) -> T {
    T {
        t: Tok::Punct(s.into()),
        line,
    }
}
fn num(s: &str, line: usize) -> T {
    T {
        t: Tok::Num(s.into()),
        line,
    }
}

/// Split the tokens strictly inside the parens at `open` into top-level comma separated argument ranges.
fn split_args(toks: &[T], open: usize) -> (Vec<(usize, usize)>, usize) {
    let close = matching_paren(toks, open);
    let (mut args, mut start, mut d) = (vec![], open + 1, 0);
    for (k, t) in toks.iter().enumerate().take(close).skip(open + 1) {
        if is_p(t, "(") || is_p(t, "[") {
            d += 1;
        }
        if is_p(t, ")") || is_p(t, "]") {
            d -= 1;
        }
        if d == 0 && is_p(t, ",") {
            args.push((start, k));
            start = k + 1;
        }
    }
    if close > open + 1 {
        args.push((start, close));
    }
    (args, close)
}

/// GLSL 4.50 (Vulkan) cannot pass a combined sampler2D through user functions after texture/sampler splitting,
/// so every user function with a `sampler2D` parameter is cloned per call site that passes a sampler uniform
/// (the parameter is deleted and its uses replaced by the uniform's name). Iterates for nested calls.
fn specialize_samplers(body: &[T], samplers: &BTreeSet<String>) -> Result<Vec<T>, String> {
    let mut toks = body.to_vec();
    for _round in 0..8 {
        // Find top-level function definitions with sampler2D params.
        let mut defs: BTreeMap<String, (usize, usize, usize, Vec<String>)> = BTreeMap::new(); // name -> (def start, body end, open paren, params ids)
        let mut depth = 0;
        let mut i = 0;
        while i < toks.len() {
            if is_p(&toks[i], "{") {
                depth += 1;
            }
            if is_p(&toks[i], "}") {
                depth -= 1;
            }
            if depth == 0
                && i >= 2
                && is_p(&toks[i], "(")
                && matches!(toks[i - 1].t, Tok::Id(_))
                && matches!(toks[i - 2].t, Tok::Id(_))
            {
                let (args, close) = split_args(&toks, i);
                if close + 1 < toks.len() && is_p(&toks[close + 1], "{") {
                    let mut params = vec![];
                    let mut has = false;
                    for (a, b) in &args {
                        let ws: Vec<&T> = toks[*a..*b].iter().filter(|t| !matches!(&t.t, Tok::Id(x) if matches!(x.as_str(), "in" | "highp" | "mediump" | "lowp"))).collect();
                        if ws.len() == 2 && is_id(ws[0], "sampler2D") {
                            has = true;
                            if let Tok::Id(n) = &ws[1].t {
                                params.push(n.clone());
                            } else {
                                params.push(String::new());
                            }
                        } else {
                            params.push(String::new());
                        }
                    }
                    if has {
                        let mut d = 0;
                        let mut e = close + 1;
                        loop {
                            if is_p(&toks[e], "{") {
                                d += 1
                            }
                            if is_p(&toks[e], "}") {
                                d -= 1;
                                if d == 0 {
                                    break;
                                }
                            }
                            e += 1;
                        }
                        if let Tok::Id(n) = &toks[i - 1].t {
                            defs.insert(n.clone(), (i - 2, e, i, params));
                        }
                        i = e;
                    }
                }
            }
            i += 1;
        }
        if defs.is_empty() {
            return Ok(toks);
        }
        // Rewrite calls (outside the defs themselves, and inside clones) and emit clones.
        let mut clones: Vec<T> = vec![];
        let mut made: BTreeMap<String, String> = BTreeMap::new();
        let mut out: Vec<T> = vec![];
        let mut i = 0;
        let mut skipped: Vec<(usize, usize)> = defs.values().map(|d| (d.0, d.1)).collect();
        skipped.sort();
        while i < toks.len() {
            if let Some((s, e)) = skipped.iter().find(|(s, _)| *s == i) {
                let _ = e;
                let _ = s;
                i = skipped.iter().find(|(s, _)| *s == i).unwrap().1 + 1;
                continue;
            }
            if let Tok::Id(n) = &toks[i].t
                && let (Some((ds, de, _, params)), true) =
                    (defs.get(n), i + 1 < toks.len() && is_p(&toks[i + 1], "("))
            {
                let (args, close) = split_args(&toks, i + 1);
                let mut binding: BTreeMap<String, String> = BTreeMap::new();
                let mut keep = vec![];
                for (k, (a, b)) in args.iter().enumerate() {
                    if !params[k].is_empty() {
                        match &toks[*a].t {
                            Tok::Id(s) if b - a == 1 && samplers.contains(s) => {
                                binding.insert(params[k].clone(), s.clone());
                            }
                            _ => {
                                return Err(format!(
                                    "sampler2D argument to {n} must be a sampler uniform (line {})",
                                    toks[*a].line
                                ));
                            }
                        }
                    } else {
                        keep.push((*a, *b));
                    }
                }
                let key = format!(
                    "{n}__{}",
                    binding.values().cloned().collect::<Vec<_>>().join("_")
                );
                if !made.contains_key(&key) {
                    // clone def tokens [ds..=de] with params removed
                    let def = &toks[*ds..=*de];
                    let open = def.iter().position(|t| is_p(t, "(")).unwrap();
                    let (dargs, dclose) = split_args(def, open);
                    let mut c: Vec<T> = def[..open - 1].to_vec();
                    c.push(id(&key, def[open - 1].line));
                    c.push(pu("(", def[open].line));
                    let mut first = true;
                    for (k, (a, b)) in dargs.iter().enumerate() {
                        if params[k].is_empty() {
                            if !first {
                                c.push(pu(",", def[*a].line));
                            }
                            first = false;
                            c.extend_from_slice(&def[*a..*b]);
                        }
                    }
                    c.push(pu(")", def[dclose].line));
                    for t in &def[dclose + 1..] {
                        match &t.t {
                            Tok::Id(x)
                                if binding.contains_key(x)
                                    && !(matches!(c.last(), Some(l) if is_p(l, "."))) =>
                            {
                                c.push(id(&binding[x], t.line))
                            }
                            _ => c.push(t.clone()),
                        }
                    }
                    clones.extend(c);
                    made.insert(key.clone(), key.clone());
                    let _ = de;
                }
                out.push(id(&key, toks[i].line));
                out.push(pu("(", toks[i].line));
                for (j, (a, b)) in keep.iter().enumerate() {
                    if j > 0 {
                        out.push(pu(",", toks[*a].line));
                    }
                    out.extend_from_slice(&toks[*a..*b]);
                }
                out.push(pu(")", toks[close].line));
                i = close + 1;
                continue;
            }
            out.push(toks[i].clone());
            i += 1;
        }
        // clones go first, so they precede their callers; they may themselves contain calls -> next round
        let mut next = clones;
        next.extend(out);
        toks = next;
    }
    Err("sampler2D function specialization did not converge (recursion?)".into())
}

fn rewrite(toks: &[T], cx: &Cx) -> Result<Vec<T>, String> {
    let mut o: Vec<T> = vec![];
    let mut i = 0;
    while i < toks.len() {
        let t = &toks[i];
        let line = t.line;
        if let Tok::Id(name) = &t.t {
            if i > 0 && is_p(&toks[i - 1], ".") {
                o.push(t.clone());
                i += 1;
                continue;
            }
            if matches!(name.as_str(), "highp" | "mediump" | "lowp" | "invariant") {
                i += 1;
                continue;
            }
            if name == "main" && i + 1 < toks.len() && is_p(&toks[i + 1], "(") {
                o.push(id("renpy_main", line));
                i += 1;
                continue;
            }
            if name == "gl_FragColor" {
                o.push(id("renpy_FragColor", line));
                i += 1;
                continue;
            }
            if name == "gl_FragCoord" {
                o.push(id("renpy_FragCoord", line));
                i += 1;
                continue;
            }
            if let Some((_, new)) = TEX_FUNCS.iter().find(|(o, _)| o == name) {
                // texture2D(S, uv[, bias]) -> texture(sampler2D(S_t, S_s), uv[, bias]); vertex stage: implicit lod 0 -> textureLod(.., 0.0)
                let (args, close) = split_args(toks, i + 1);
                let first = match (&toks[args[0].0].t, args[0].1 - args[0].0) {
                    (Tok::Id(s), 1) => s.clone(),
                    _ => String::new(),
                };
                if !cx.samplers.contains(&first) {
                    return Err(format!(
                        "{name}: first argument must be a sampler uniform (line {line})"
                    ));
                }
                let implicit = *new == "texture" || *new == "textureProj";
                let vertex_implicit = cx.stage == Stage::Vertex && implicit;
                let nn = if vertex_implicit {
                    if *new == "texture" {
                        "textureLod"
                    } else {
                        "textureProjLod"
                    }
                } else {
                    new
                };
                o.push(id(nn, line));
                o.push(pu("(", line));
                for x in [
                    id("sampler2D", line),
                    pu("(", line),
                    id(&format!("{first}_t"), line),
                    pu(",", line),
                    id(&format!("{first}_s"), line),
                    pu(")", line),
                ] {
                    o.push(x);
                }
                for (a, b) in &args[1..] {
                    o.push(pu(",", line));
                    o.extend(rewrite(&toks[*a..*b], cx)?);
                }
                if vertex_implicit && args.len() == 2 {
                    o.push(pu(",", line));
                    o.push(num("0.0", line));
                }
                o.push(pu(")", line));
                i = close + 1;
                continue;
            }
            if let Some(d) = cx.uniforms.get(name) {
                if d.array.is_some()
                    && matches!(
                        d.ty.as_str(),
                        "float" | "vec2" | "int" | "ivec2" | "bool" | "bvec2"
                    )
                    && i + 1 < toks.len()
                    && is_p(&toks[i + 1], "[")
                {
                    // u[expr]  ->  u[expr].x / .xy   (bool: (u[expr].x != 0))
                    let mut k = i + 1;
                    let mut dd = 0;
                    loop {
                        if is_p(&toks[k], "[") {
                            dd += 1
                        }
                        if is_p(&toks[k], "]") {
                            dd -= 1;
                            if dd == 0 {
                                break;
                            }
                        }
                        k += 1;
                    }
                    let sw = if d.ty.ends_with('2') { "xy" } else { "x" };
                    let isbool = d.ty.starts_with('b');
                    if isbool {
                        o.push(pu("(", line));
                    }
                    o.push(id(name, line));
                    o.push(pu("[", line));
                    o.extend(rewrite(&toks[i + 2..k], cx)?);
                    o.push(pu("]", line));
                    o.push(pu(".", line));
                    o.push(id(sw, line));
                    if isbool {
                        if sw == "x" {
                            o.push(pu("!=", line));
                            o.push(num("0", line));
                        }
                        o.push(pu(")", line));
                    }
                    i = k + 1;
                    continue;
                }
                if d.ty == "bool" {
                    for x in [
                        pu("(", line),
                        id(name, line),
                        pu("!=", line),
                        num("0", line),
                        pu(")", line),
                    ] {
                        o.push(x);
                    }
                    i += 1;
                    continue;
                }
                if d.ty.starts_with("bvec") {
                    let n = &d.ty[4..];
                    for x in [
                        id("notEqual", line),
                        pu("(", line),
                        id(name, line),
                        pu(",", line),
                        id(&format!("ivec{n}"), line),
                        pu("(", line),
                        num("0", line),
                        pu(")", line),
                        pu(")", line),
                    ] {
                        o.push(x);
                    }
                    i += 1;
                    continue;
                }
                if d.ty == "mat2" && d.array.is_none() {
                    for x in [
                        id("mat2", line),
                        pu("(", line),
                        id(name, line),
                        pu(".", line),
                        id("xy", line),
                        pu(",", line),
                        id(name, line),
                        pu(".", line),
                        id("zw", line),
                        pu(")", line),
                    ] {
                        o.push(x);
                    }
                    i += 1;
                    continue;
                }
            }
            if cx.user_fns.contains(name)
                || (RESERVED.contains(&name.as_str()) && !NUMERIC_TYPES.contains(&name.as_str()))
            {
                o.push(id(&format!("rp_{name}"), line));
                i += 1;
                continue;
            }
        }
        o.push(t.clone());
        i += 1;
    }
    Ok(o)
}
