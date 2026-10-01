"""A loose Python 2 module in game/. Python 2 compiles it as it is; the player's import hook must make Python 3 take it:
print statements (plain, to a file, with a trailing comma), division, dict.keys() as a list."""
import StringIO
import sys


def describe(d):
    keys = d.keys()
    keys.sort()
    print "legacy module loaded"
    return "%s %s" % (",".join(keys), 7 / 2)


def ratio(a, b):
    return a / b


def printed():
    buf = StringIO.StringIO()
    print >>buf, "a", 1
    print >>buf, "b",
    print >>buf, "c"
    print >>buf
    print >>buf, "d",
    print >>buf
    for n in (1, 2, 3):
        print >>buf, n,
    print >>buf
    print
    return buf.getvalue().replace("\n", "|")
