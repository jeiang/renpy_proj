"""Stand-in for renpy.text.bidi (the stock module wraps GNU fribidi, which the player does not link).

It handles left-to-right text: `log2vis` returns the text unchanged and `get_embedding_levels` returns
level 0 for every character. Text that needs real bidirectional reordering (right-to-left characters, or
a right-to-left reading order) raises NotImplementedError instead of showing wrong output.
"""

import unicodedata

# Same names as the stock module. The numbers only need to differ from each other.
ON = 0x40
LTR = 0x110
RTL = 0x111
WLTR = 0x20
WRTL = 0x21

_RTL_CLASSES = frozenset(("R", "AL", "RLE", "RLO", "RLI"))


def _check(s, direction):
    if direction in (RTL, WRTL):
        raise NotImplementedError("right-to-left reading order needs fribidi, which this player build lacks")

    for ch in s:
        if ord(ch) > 0x58F and unicodedata.bidirectional(ch) in _RTL_CLASSES:
            raise NotImplementedError("right-to-left text needs fribidi, which this player build lacks: %r" % ch)

    if direction == ON:
        for ch in s:
            if unicodedata.bidirectional(ch) == "L":
                return LTR
        return WLTR

    return direction


def log2vis(s, direction=ON):
    direction = _check(s, direction)
    return s, direction


def get_embedding_levels(s, direction=ON):
    direction = _check(s, direction)
    return (0,) * len(s), direction
