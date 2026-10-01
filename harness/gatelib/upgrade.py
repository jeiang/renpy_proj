"""The AI upgrade pass (M6): error record -> bounded prompt -> OpenAI-compatible model -> proposed patch -> verification.

Design: issue #20 (resolution comment) and player/CONTRACTS.md "M6 contracts". One error is handled by `upgrade_error`:

1. build the context slice (`build_prompt`): traceback, the PyCode node to patch, the other game code in the traceback, the
   classes and functions the failing statement uses, variable types at the error, the compat rule list, under a token cap;
2. ask the model (`chat`): it answers JSON with `explanation` and either `edits` (exact old/new snippets) or `source`;
3. write the patch to a scratch library and verify it (`verify`): the pre-error save loads on the patched game and the
   failing node runs with no traceback; the full M3 gate passes; stock Ren'Py 7 saves still resume;
4. on a failure, send the failure back (up to `tries` attempts); then `needs-human`;
5. a verified patch is written to `<data>/patches/<fingerprint>/<error id>.toml` with the sidecar `<error id>.json`
   (state `proposed`). `player patches <game> accept <id>` makes it active. The loader ignores everything but `accepted`.

Nothing here writes into the repository. The API key comes from the environment or `<data>/upgrade.toml` and is never
logged: prompts and responses are stored, headers are not.
"""
import hashlib
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import time
import tomllib
import urllib.error
import urllib.request

from . import launch as L

# What the compatibility module already does, so the model does not repeat it and knows what a patch must be.
COMPAT_RULES = """\
The game was written for Ren'Py 7 (Python 2). The player runs it on Ren'Py 8 (Python 3) with a compatibility module.
The module already rewrites game code (every .rpy and loose .py file of the game, not patches) this way:
- `/` on two ints floors (Python 2 integer division); `round()` rounds half away from zero and returns a number as in Python 2;
- `dict.keys()/values()/items()`, `map`, `filter`, `zip` return lists; `has_key`, `iteritems`, `itervalues`, `iterkeys` work;
- `__metaclass__`, `__eq__` without `__hash__`, `cmp=` sorts and `exec` inside functions are handled;
- mixed-type ordering (`1 < "a"`) is fixed when it raises, by recompiling the block with Python 2 ordering;
- Python 2-only syntax (`print x`, `exec x`, backticks, `<>`, `ur''`, old octal, `except E, e`) is rewritten before compile.
Whatever raised the error below is something these rules did not cover.
A patch REPLACES the source of one Python block (`$` line, `python:` block or `init python:` block). Its new source is
compiled as plain Python 3 with none of the rules above: write Python 3 that is correct on its own, use `//` where
integer division is meant, `str` for text, and keep every name the rest of the game expects from that block (functions,
classes, variables defined in it must keep their names and signatures). Keep the block's indentation style: the source
is the block body without the `python:` line, dedented. Do not add imports of Python 2-only modules.
"""

SYSTEM = """\
You repair one Python block in a Ren'Py visual novel that was written for Ren'Py 7 (Python 2) and now runs on Ren'Py 8
(Python 3). You get one runtime error with its context. Answer with ONE JSON object and nothing else:
{"explanation": "<what failed and why, 1 to 4 sentences>",
 "edits": [{"old": "<exact text from the target block, unique in it>", "new": "<replacement>"}]}
Each "old" must occur exactly once in the target block (copy it from the numbered listing without the line numbers).
Make the smallest change that fixes the cause. If the block is short you may answer {"explanation": "...", "source": "<the whole new block>"} instead of "edits".
""" + "\n" + COMPAT_RULES

CHARS_PER_TOKEN = 3.5


def est_tokens(text):
    return int(len(text) / CHARS_PER_TOKEN) + 1


# ------------------------------------------------------------------------------------------------ config
def default_data_dir():
    home = pathlib.Path.home()
    if sys.platform == "darwin":
        return home / "Library/Application Support/renpy-player"
    if sys.platform.startswith("win"):
        return pathlib.Path(os.environ.get("APPDATA", home)) / "renpy-player"
    return pathlib.Path(os.environ.get("XDG_DATA_HOME") or home / ".local/share") / "renpy-player"


def load_config(data, args):
    """Endpoint settings: flags, then the environment, then `<data>/upgrade.toml` (base_url, model, api_key_env or api_key).
    -> dict(base_url, model, key, source). The key is only read, never printed."""
    cfg = {}
    f = pathlib.Path(data) / "upgrade.toml"
    if f.exists():
        cfg = tomllib.loads(f.read_text())
    key = os.environ.get("PLAYER_UPGRADE_API_KEY") or (os.environ.get(cfg["api_key_env"]) if cfg.get("api_key_env") else None) or cfg.get("api_key")
    return {"base_url": args.endpoint or os.environ.get("PLAYER_UPGRADE_BASE_URL") or cfg.get("base_url"),
            "model": args.model or os.environ.get("PLAYER_UPGRADE_MODEL") or cfg.get("model"),
            "key": key, "timeout": int(cfg.get("timeout", 900)),
            "max_tokens": int(cfg.get("max_tokens", 4096))}


def endpoint_url(base):
    base = base.rstrip("/")
    return base if base.endswith("/chat/completions") else base + "/chat/completions"


def check_reachable(cfg):
    """-> None when the endpoint answers `GET <base>/models`, else the reason."""
    base = cfg["base_url"].rstrip("/")
    if base.endswith("/chat/completions"):
        base = base[: -len("/chat/completions")]
    req = urllib.request.Request(base + "/models", headers=_headers(cfg))
    try:
        with urllib.request.urlopen(req, timeout=15) as r:
            r.read(1)
        return None
    except urllib.error.HTTPError as e:
        return None if e.code in (401, 403, 404, 405) and e.code != 401 else "HTTP %d from %s/models" % (e.code, base)
    except Exception as e:  # noqa: BLE001
        return "%s: %s" % (type(e).__name__, e)


def _headers(cfg):
    h = {"Content-Type": "application/json"}
    if cfg.get("key"):
        h["Authorization"] = "Bearer " + cfg["key"]
    return h


def chat(cfg, messages):
    """One chat completion. -> (text, usage dict). Raises RuntimeError with the server's message."""
    body = json.dumps({"model": cfg["model"], "messages": messages, "temperature": 0.2, "max_tokens": cfg["max_tokens"],
                       "stream": False}).encode()
    req = urllib.request.Request(endpoint_url(cfg["base_url"]), data=body, headers=_headers(cfg), method="POST")
    try:
        with urllib.request.urlopen(req, timeout=cfg["timeout"]) as r:
            data = json.loads(r.read())
    except urllib.error.HTTPError as e:
        raise RuntimeError("model endpoint HTTP %d: %s" % (e.code, e.read(500).decode("utf-8", "replace"))) from None
    except Exception as e:  # noqa: BLE001
        raise RuntimeError("model endpoint: %s: %s" % (type(e).__name__, e)) from None
    try:
        return data["choices"][0]["message"]["content"], data.get("usage") or {}
    except (KeyError, IndexError, TypeError):
        raise RuntimeError("model endpoint answered without choices[0].message.content: %s" % str(data)[:300]) from None


# ------------------------------------------------------------------------------------------------ slice
def numbered(source, first_line, lo=None, hi=None):
    lines = source.split("\n")
    lo = 0 if lo is None else max(0, lo)
    hi = len(lines) if hi is None else min(len(lines), hi)
    out = []
    if lo > 0:
        out.append("     ... (%d lines before)" % lo)
    out += ["%5d  %s" % (first_line + i, lines[i]) for i in range(lo, hi)]
    if hi < len(lines):
        out.append("     ... (%d lines after)" % (len(lines) - hi))
    return "\n".join(out)


def window(source, first_line, fail_line, half):
    n = source.count("\n") + 1
    if n <= 2 * half + 1:
        return numbered(source, first_line)
    i = fail_line - first_line
    return numbered(source, first_line, i - half, i + half + 1)


def build_prompt(err, token_cap):
    """-> (messages, info). Sections are dropped or shrunk, lowest priority first, until the prompt fits `token_cap`."""
    t = err["patch_target"]
    exc = err["exception"]
    game_frames = [f for f in err["frames"] if f.get("game")]
    inner = game_frames[-1] if game_frames else {}
    half = 400
    use_defs = list(err.get("used_defs") or [])
    other = [f for f in game_frames if f.get("pycode") and f["pycode"]["sha1"] != t["sha1"]]
    tb_text = err.get("traceback", "")
    notes = []

    def render():
        parts = ["## Error", "%s: %s" % (exc["type"], exc["message"]),
                 "Ren'Py %s; compat module diagnosis: %s" % (err.get("renpy"), err.get("classify")), "",
                 "## Traceback (Ren'Py's report)", tb_text.strip(), "",
                 "## Target block to patch: %s line %d (the failing line is %d, in function `%s`)" % (t["file"], t["line"], t["frame_line"], t["func"]),
                 "Mode: %s. Lines are numbered as in the script file." % t["mode"],
                 window(t["source"], t["line"], t["frame_line"], half)]
        if inner.get("stmt"):
            parts += ["", "Failing statement: `%s`" % inner["stmt"].strip()]
        if inner.get("locals_types"):
            parts += ["Variable types in the failing frame: " + ", ".join("%s: %s" % kv for kv in sorted(inner["locals_types"].items())[:40])]
        if other:
            parts += ["", "## Other game code in the traceback"]
            for f in other:
                p = f["pycode"]
                src = p.get("source")
                parts.append("%s line %d, in `%s`, statement: `%s`" % (f["file"], f["line"], f["func"], f.get("stmt", "").strip()))
                if src:
                    parts.append(window(src, p["line"], f["line"], 25))
        if use_defs:
            parts += ["", "## Classes and functions the failing statement uses"]
            for d in use_defs:
                parts.append("%s `%s` (defined in %s line %d):" % (d["kind"], d["name"], d["block_file"], d["block_line"]))
                parts.append(window(d["block_source"], d["block_line"], d["line"], 30))
        if notes:
            parts += [""] + notes
        parts += ["", "Fix the cause with the smallest change. Answer with the JSON object only."]
        return "\n".join(parts)

    user = render()
    for step in range(12):
        if est_tokens(SYSTEM) + est_tokens(user) <= token_cap:
            break
        if use_defs:
            use_defs.pop()
        elif other:
            other.pop()
        elif half > 30:
            half = max(30, half // 2)
            notes[:] = ["(The target block is shown only around the failing line; \"old\" must come from the lines shown.)"]
        else:
            tb_text = "\n".join(tb_text.strip().splitlines()[-25:])
        user = render()
    info = {"tokens_est": est_tokens(SYSTEM) + est_tokens(user), "token_cap": token_cap, "defs": len(use_defs), "other_nodes": len(other),
            "window_half": half, "fits": est_tokens(SYSTEM) + est_tokens(user) <= token_cap}
    return [{"role": "system", "content": SYSTEM}, {"role": "user", "content": user}], info


def prompt_hash(messages):
    return hashlib.sha256(json.dumps(messages, sort_keys=True).encode()).hexdigest()[:16]


# ------------------------------------------------------------------------------------------------ patch
def parse_answer(text):
    """-> dict from the model's text (fences and prose around the JSON are tolerated). Raises ValueError."""
    t = text.strip()
    m = re.search(r"```(?:json)?\s*(.*?)```", t, re.S)
    if m and "{" in m.group(1):
        t = m.group(1).strip()
    a, b = t.find("{"), t.rfind("}")
    if a < 0 or b < a:
        raise ValueError("the answer holds no JSON object")
    try:
        d = json.loads(t[a:b + 1])
    except json.JSONDecodeError as e:
        raise ValueError("the answer is not valid JSON: %s" % e) from None
    if not isinstance(d, dict) or not isinstance(d.get("explanation"), str):
        raise ValueError('the JSON object needs a string "explanation"')
    return d


def apply_answer(answer, original):
    """-> new source. Raises ValueError with a message for the model."""
    if isinstance(answer.get("source"), str):
        new = answer["source"]
    else:
        edits = answer.get("edits")
        if not isinstance(edits, list) or not edits:
            raise ValueError('answer needs a non-empty "edits" list or a "source" string')
        new = original
        for i, e in enumerate(edits, 1):
            if not isinstance(e, dict) or not isinstance(e.get("old"), str) or not isinstance(e.get("new"), str):
                raise ValueError('edit %d needs string fields "old" and "new"' % i)
            n = new.count(e["old"])
            if n != 1:
                raise ValueError("edit %d: \"old\" occurs %d times in the block (it must occur exactly once): %r" % (i, n, e["old"][:120]))
            new = new.replace(e["old"], e["new"])
    if new.strip() == original.strip():
        raise ValueError("the patch does not change the block")
    return new


def py3_check(source, mode):
    """Compile as plain Python 3. -> None or an error text."""
    import textwrap
    src = textwrap.dedent(source).strip("\n") + "\n"
    try:
        compile(src, "<patch>", "eval" if mode == "eval" else "exec")
    except SyntaxError as e:
        return "SyntaxError: %s (line %s) in the new source" % (e.msg, e.lineno)
    return None


def toml_string(s):
    if "'''" not in s and "\r" not in s and all(c >= " " or c in "\n\t" for c in s):
        return "'''\n" + s + "'''"
    out = []
    for c in s:
        out.append({"\\": "\\\\", '"': '\\"', "\n": "\n", "\t": "\\t", "\r": "\\r"}.get(c, c if c >= " " else "\\u%04x" % ord(c)))
    return '"""\n' + "".join(out) + '"""'


def patch_toml(target, new_source, comment):
    src = new_source if new_source.endswith("\n") else new_source + "\n"
    text = "# %s\n[[patch]]\nfile = %s\nline = %d\noriginal_hash = \"sha1:%s\"\nsource = %s\n" % (
        comment.replace("\n", " ")[:200], json.dumps(target["file"]), target["line"], target["sha1"][:12], toml_string(src))
    back = tomllib.loads(text)["patch"][0]["source"]
    if back != src:
        raise RuntimeError("TOML round trip changed the patch source")
    return text


# ------------------------------------------------------------------------------------------------ verification
def sanitize_save_dir(name):
    return re.sub(r"[^A-Za-z0-9._ -]+", "_", name)


def write_seed(seed, fp, pid, toml_text, state, err_dir, err):
    """A scratch player data folder: the patch library (patch + sidecar) and the error's saves."""
    shutil.rmtree(seed, ignore_errors=True)
    pd = seed / "patches" / fp
    pd.mkdir(parents=True)
    if toml_text is not None:
        (pd / (pid + ".toml")).write_text(toml_text)
        (pd / (pid + ".json")).write_text(json.dumps({"state": state}))
    sd = seed / "saves" / sanitize_save_dir(err.get("save_dir") or err.get("save_directory") or "game")
    sd.mkdir(parents=True)
    for f in (err_dir / "saves").iterdir():
        shutil.copy2(f, sd / f.name)
    if (err_dir / "tokens").exists():   # the signature keys the saves were written with
        shutil.copytree(err_dir / "tokens", seed / "saves" / "tokens")


def save_slot(err):
    """The slot to load: the rolling save made at the failing node, else the save made when the error happened."""
    slot = (err.get("pre_save") or {}).get("slot") or err.get("error_save")
    return slot


def load_run(ctx, name, err, with_patch, timeout=180):
    """Load the error's save in the player and run the failing node. -> dict(ok, line, reason, applied, launch)."""
    node = err["node"]
    slot = save_slot(err)
    plan = L.parse_plan("wait menu True\ncmd verify %d %d %s\ncmd load %s\nwait verify %d\nend\n" % (
        node["line"], timeout, node["file"], slot, timeout + 30))
    r = L.launch(ctx, name, plan=plan, timeout=timeout + 240, keep_saves=False)
    line = r["progress"] and next((ln for ln in reversed(r["progress"]) if ln.startswith("verify-")), None)
    d = ctx.out / name
    patches_json = None
    for p in d.rglob("patches.json"):
        try:
            patches_json = json.loads(p.read_text())
        except ValueError:
            pass
    res = {"ok": line == "verify-ok", "line": line, "aborted": r.get("aborted"), "patches": patches_json,
           "sweep_ok": r["sweep_ok"], "traceback": (r.get("traceback") or "")[:1500]}
    errs = sorted((d / "deep").glob("err-*.json")) if (d / "deep").exists() else []
    if errs:
        e = json.loads(errs[0].read_text())
        res["error"] = "%s: %s" % (e["exception"]["type"], e["exception"]["message"][:300])
        res["error_frames"] = [(f["file"], f["line"], f["func"]) for f in e["frames"] if f.get("game")][-4:]
    return res


def stock_baseline(out_root, key, game_arg, game_args):
    d = out_root / ("%s-stock" % key)
    if (d / "checks" / "probe.json").exists():
        return d
    r = subprocess.run([sys.executable, str(L.HARNESS / "gate.py"), "run", "--engine", "stock", "--game", game_arg, "--tier", "full", "--out", str(d)] + game_args)
    return d if (d / "checks" / "probe.json").exists() else None


def run_gate(game_arg, player_bin, seed, out, baseline, extra):
    """The full M3 gate on the patched player. -> (passed, summary dict)."""
    cmd = [sys.executable, str(L.HARNESS / "gate.py"), "run", "--engine", "player", "--player-bin", str(player_bin), "--game", game_arg,
           "--tier", "full", "--with-proposed", "--seed-data", str(seed), "--out", str(out)]
    if baseline:
        cmd += ["--baseline", str(baseline)]
    r = subprocess.run(cmd + extra, capture_output=True, text=True)
    res = out / "result.json"
    summary = {"rc": r.returncode}
    if res.exists():
        summary.update(json.loads(res.read_text()))
        checks = {}
        for p in sorted((out / "checks").glob("*.json")):
            c = json.loads(p.read_text())
            checks[p.stem] = {"status": c["status"], "problems": c.get("problems")}
        summary["checks_detail"] = checks
    else:
        summary["stderr"] = r.stderr[-800:]
    return r.returncode == 0 and summary.get("status") == "pass", summary
