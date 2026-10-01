"""Stage timeouts: every launch is a sequence of stages, each confirmed by a line the injected script writes to
progress.txt (or, for lint, by the engine's own output), each with its own short timeout. A missed stage fails at once,
naming the stage, the last line seen and its age.

One table (DEFAULTS, seconds). A game overrides single entries in corpus.toml: `stages = { boot = 240 }`.
`--stage-scale F` multiplies the whole table (for measuring a slow machine or game; the measured times are in each launch's
`stage_times`).
"""

DEFAULTS = {
    "boot": 60,         # process start -> "boot" (engine init, scripts loaded, init blocks run). Longer only where measured.
    "menu": 60,         # "menu True": the main menu is on screen
    "ack": 10,          # a command sent -> "cmd-ack <cmd>"
    "done": 10,         # "cmd-ack" -> "cmd-done <cmd>" for commands that return (auto, click, advance, advance-to, exec)
    "save": 30,         # "cmd save" -> "saved <slot>"
    "start": 30,        # "cmd start" -> "label start"
    "jump": 30,         # "cmd jump X" -> "label X"
    "loaded": 30,       # "cmd load" -> "loaded" (config.after_load_callbacks)
    "first-say": 30,    # first "say N" after a load or a start
    "say": 15,          # the longest gap between two "say N" lines while advancing (per line)
    "movie-begin": 30,  # "cmd movie" -> "movie-begin"
    "movie-slack": 30,  # "video-result" must come within warm + secs + this
    "quit": 45,         # "cmd quit" -> the process exits
    "lint-boot": 60,    # lint: the first output (stdout, or log.txt)
    "lint": 900,        # lint: the "Statistics:" line
}


def table(game, scale=1.0):
    t = dict(DEFAULTS)
    t.update(game.get("stages", {}))
    return {k: v * scale for k, v in t.items()}


def completion(line):
    """-> (stage, predicate(progress lines after the ack) -> bool) that a command must satisfy after its "cmd-ack", or None
    (quit: the process exits; checked by the caller)."""
    w = line.split(None, 1)
    cmd, args = w[0], (w[1] if len(w) > 1 else "")
    if cmd in ("auto", "click", "advance", "advance-to", "exec"):
        return "done", lambda lines: ("cmd-done " + line) in lines
    if cmd == "save":
        return "save", lambda lines: ("saved %s" % args) in lines
    if cmd == "start":
        return "start", lambda lines: "label start" in lines
    if cmd == "jump":
        return "jump", lambda lines: ("label %s" % args) in lines
    if cmd == "load":
        return "loaded", lambda lines: "loaded" in lines
    if cmd == "movie":
        return "movie-begin", lambda lines: any(ln.startswith("movie-begin") for ln in lines)
    return None
