#!/usr/bin/env python3
"""Build the big test game: about 54 000 lines of generated script over 120 files, plus the hand-written start, screen and
route. Loads script at scale (thousands of labels, menus, `define`, `default`, `init python`, translations) and makes
the save file grow (a 3000-entry store table, and one state entry per label reached).

    python3 harness/testgames/build.py big

The generated scripts (<dest>/game/gen/*.rpy) are build output. They are not committed. The generator is deterministic: the
same SEED gives the same bytes. `build()` raises if the total line count is below MIN_LINES.
"""
import pathlib
import random
import shutil
import sys

HERE = pathlib.Path(__file__).resolve().parent
SEED = 20260930
FILES = 120
LABELS = 40                  # labels per file; the chain runs through all files in order (FILES * LABELS = 4800 labels)
MENU_EVERY = 5               # every fifth label of a file ends in a menu with two choices (the first one moves forward)
TABLE = 3000                 # entries of `default big_table` (saved with every save)
TL_FILES = 5                 # translate english strings: the dialogue of the first five files
MIN_LINES = 50000

SUBJECTS = ["lamp", "river", "clerk", "archive", "gate", "orchard", "signal", "lantern", "harbour", "courier", "kettle",
            "mirror", "compass", "meadow", "tower", "ledger"]
VERBS = ["watches", "carries", "counts", "answers", "repairs", "follows", "remembers", "opens", "measures", "sorts"]
OBJECTS = ["the letter", "a copper key", "the north road", "three crates", "the blue map", "a folded note",
           "the morning bell", "the iron hinge", "a bundle of reeds", "the eastern wall"]
TAILS = ["before the rain", "under the bridge", "with no hurry", "at the second bell", "beside the old well",
         "until the lamps go out", "in the quiet hour", "as the wind turns"]


def label_name(f, k):
    return "f%03d_l%02d" % (f, k)


def sentence(rng, f, k, j):
    """Dialogue text. No quotes, brackets or braces: Ren'Py reads [ and { in text."""
    return "%s, line %d: the %s %s %s %s." % (label_name(f, k), j, rng.choice(SUBJECTS), rng.choice(VERBS),
                                             rng.choice(OBJECTS), rng.choice(TAILS))


def file_header(f, rng):
    color = "#%02x%02x%02x" % (rng.randrange(60, 256), rng.randrange(60, 256), rng.randrange(60, 256))
    a, b, m = rng.randrange(2, 97), rng.randrange(1, 997), rng.choice([97, 251, 997])
    return [
        "# SynthBig generated file %03d (seed %d). Build output: do not edit. Source: harness/testgames/big/build.py." % (f, SEED),
        "",
        'define f%03d_who = Character("Speaker %03d", what_color="%s")' % (f, f, color),
        "",
        "init python:",
        "    def f%03d_score(x):" % f,
        '        """Deterministic arithmetic used by the labels of this file."""',
        "        return (x * %d + %d) %% %d" % (a, b, m),
        "",
        "",
    ]


def generate(dest_gen):
    """Write gen/fNNN.rpy, gen/store.rpy and gen/tl.rpy. Returns the number of lines written."""
    rng = random.Random(SEED)
    dest_gen.mkdir(parents=True, exist_ok=True)
    names = [label_name(f, k) for f in range(FILES) for k in range(LABELS)]
    total = 0
    tl_pairs = []
    for f in range(FILES):
        lines = file_header(f, rng)
        for k in range(LABELS):
            g = f * LABELS + k
            name = label_name(f, k)
            nxt = names[g + 1] if g + 1 < len(names) else None
            lines.append("label %s:" % name)
            lines.append('    $ big_state["%s"] = f%03d_score(%d)' % (name, f, k))
            for j in range(5 + rng.randrange(3)):
                text = sentence(rng, f, k, j)
                lines.append('    f%03d_who "%s"' % (f, text))
                if f < TL_FILES:
                    tl_pairs.append((text, text + " (translated)"))
            if nxt is None:
                lines.append("    return")
            elif k % MENU_EVERY == MENU_EVERY - 1:
                skip = names[min(g + 2, len(names) - 1)]
                lines += [
                    "    menu:",
                    '        "Go on.":',
                    "            jump %s" % nxt,
                    '        "Skip one label.":',
                    "            jump %s" % skip,
                ]
            else:
                lines.append("    jump %s" % nxt)
            lines.append("")
        lines.append("")
        (dest_gen / ("f%03d.rpy" % f)).write_text("\n".join(lines) + "\n", encoding="utf-8")
        total += len(lines) + 1

    # One store table: a list of ints, saved with every save. Lines of 100 entries keep the file readable.
    table = [rng.randrange(0, 1 << 20) for _ in range(TABLE)]
    store = ["# SynthBig store table (seed %d). Build output." % SEED, "default big_table = ["]
    for i in range(0, TABLE, 100):
        store.append("    " + ", ".join(str(v) for v in table[i:i + 100]) + ",")
    store += ["]", ""]
    (dest_gen / "store.rpy").write_text("\n".join(store) + "\n", encoding="utf-8")
    total += len(store)

    tl = ["# SynthBig translations of the dialogue of the first %d files (build output)." % TL_FILES, "",
          "translate english strings:", ""]
    for old, new in tl_pairs:
        tl += ['    old "%s"' % old, '    new "%s"' % new, ""]
    (dest_gen / "tl.rpy").write_text("\n".join(tl) + "\n", encoding="utf-8")
    total += len(tl)
    return total


def copy_game(dest):
    shutil.rmtree(dest / "game", ignore_errors=True)
    shutil.copytree(HERE / "game", dest / "game", ignore=shutil.ignore_patterns("__pycache__", "*.pyc", "cache", "saves"))


def build(dest):
    dest = pathlib.Path(dest)
    copy_game(dest)
    lines = generate(dest / "game" / "gen")
    if lines < MIN_LINES:
        raise SystemExit("big: %d generated lines, below %d" % (lines, MIN_LINES))
    print("[big] %d generated lines in %d files" % (lines, FILES + 2), flush=True)


if __name__ == "__main__":
    build(HERE.parent / "build" / "big")
    sys.exit(0)
