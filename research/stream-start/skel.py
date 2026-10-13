#!/usr/bin/env python3
"""Make a 'skeleton' copy of a loose-.rpy game: every top-level label whose body has no init-level statement is cut to
`return`. Investigation tool: the load of the result is a lower bound for any design that keeps label bodies off the
critical path. skel.py GAMEDIR  (edits GAMEDIR/game/**/*.rpy in place; use on a scratch copy)"""
import os, re, sys

INITISH = re.compile(r"^\s*(init\b|define\b|default\b|image\b|transform\b|screen\b|style\b|translate\b|testcase\b|python\s+early\b|rpy\b)")
LABEL = re.compile(r"^label\s+[.\w]+\s*(\(.*\))?\s*(hide\s*)?:\s*(#.*)?$")


def process(path):
    text = open(path, encoding="utf-8-sig", errors="surrogateescape").read()
    lines = text.split("\n")
    out = []
    i = 0
    n = len(lines)
    quote = None
    kept = cut = 0
    total_cut_lines = 0

    def top_level_start(ln):
        return ln[:1] not in (" ", "\t", "") and not ln.startswith("#")

    while i < n:
        ln = lines[i]
        if LABEL.match(ln):
            j = i + 1
            q = None
            body = []
            while j < n:
                l2 = lines[j]
                if q is None and top_level_start(l2):
                    break
                for m in re.finditer(r'"""|\'\'\'', l2):
                    t = m.group(0)
                    if q is None:
                        q = t
                    elif q == t:
                        q = None
                body.append(l2)
                j += 1
            # trailing blank / comment lines belong to the next statement visually; keep them
            while body and (body[-1].strip() == "" or body[-1].lstrip().startswith("#")) and not body[-1].startswith((" ", "\t")):
                j -= 1
                body.pop()
            if any(INITISH.match(b) for b in body) or not body:
                out.append(ln)
                out.extend(body)
                kept += 1
            else:
                out.append(ln)
                out.append("    return")
                cut += 1
                total_cut_lines += len(body)
            i = j
        else:
            out.append(ln)
            i += 1
    open(path, "w", encoding="utf-8", errors="surrogateescape").write("\n".join(out))
    return kept, cut, total_cut_lines, n


def main():
    root = sys.argv[1]
    K = C = L = N = 0
    for d, _, fs in os.walk(os.path.join(root, "game")):
        if os.sep + "cache" in d + os.sep:
            continue
        for f in fs:
            if f.endswith(".rpy"):
                p = os.path.join(d, f)
                # reflink copies share data: write via a new file so the original is untouched
                data = open(p, "rb").read()
                os.unlink(p)
                open(p, "wb").write(data)
                k, c, cl, n = process(p)
                K += k; C += c; L += cl; N += n
    print("labels kept %d, cut %d; lines cut %d of %d (%.0f%%)" % (K, C, L, N, 100.0 * L / max(N, 1)))


main()
