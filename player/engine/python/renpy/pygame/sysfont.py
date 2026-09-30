# Player stand-in for renpy.pygame.sysfont (stock file is LGPL, not carried over;
# see ARCHITECTURE.md, Licensing). Only renpy/text/font.py uses it: it calls
# initsysfonts() and walks Sysfonts.values() -> {(bold, italic): path}.

import os
import sys

# name -> {(bold, italic): path}
Sysfonts = {}

_EXTENSIONS = (".ttf", ".ttc", ".otf")
_initialized = False


def _font_dirs():
    home = os.path.expanduser("~")
    if sys.platform == "darwin":
        return ["/System/Library/Fonts", "/Library/Fonts", os.path.join(home, "Library/Fonts")]
    if sys.platform == "win32":
        dirs = [os.path.join(os.environ.get("WINDIR", r"C:\Windows"), "Fonts")]
        local = os.environ.get("LOCALAPPDATA")
        if local:
            dirs.append(os.path.join(local, "Microsoft", "Windows", "Fonts"))
        return dirs
    data = os.environ.get("XDG_DATA_HOME") or os.path.join(home, ".local/share")
    return ["/usr/share/fonts", "/usr/local/share/fonts", os.path.join(data, "fonts"), os.path.join(home, ".fonts")]


def _style(stem):
    s = stem.lower()
    bold = "bold" in s or s.endswith(("bd", "-b"))
    italic = "italic" in s or "oblique" in s or s.endswith(("it", "-i"))
    return bold, italic


def _key(stem):
    s = stem.lower()
    for word in ("bold", "italic", "oblique", "regular"):
        s = s.replace(word, "")
    return "".join(c for c in s if c.isalnum())


def initsysfonts():
    """Scans the platform font folders once and fills Sysfonts."""
    global _initialized
    if _initialized:
        return
    _initialized = True

    for top in _font_dirs():
        for root, _dirs, files in os.walk(top):
            for fn in files:
                stem, ext = os.path.splitext(fn)
                if ext.lower() not in _EXTENSIONS:
                    continue
                styles = Sysfonts.setdefault(_key(stem), {})
                styles.setdefault(_style(stem), os.path.join(root, fn))


def get_fonts():
    initsysfonts()
    return list(Sysfonts)


def match_font(name, bold=False, italic=False):
    """Returns the path of the best match for a comma-separated name list, or None."""
    initsysfonts()
    for n in name.split(","):
        styles = Sysfonts.get(_key(n.strip()))
        if styles:
            return styles.get((bool(bold), bool(italic))) or next(iter(styles.values()))
    return None


def SysFont(name, size, bold=False, italic=False, constructor=None):
    raise NotImplementedError("pygame.font is not part of the player; Ren'Py loads fonts through renpy.text.font.")
