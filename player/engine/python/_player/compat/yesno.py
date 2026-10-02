"""`layout.yesno_prompt` for Ren'Py 7 games that never call `layout.defaults()`.

Ren'Py 7 filled `layout.yesno_prompt` only when the game used a theme, `gui.init` or a layout function that provides it.
Ren'Py 8 asks the trust question for a save with an unknown signing key (`savetoken.check_load`) through that name. A
Ren'Py 7 game without it would stop with an `AttributeError` at `renpy.load`. After init, if the name is still absent, this
provides it: the game's own `confirm` or `yesno_prompt` screen when it has one (as `layout.screen_yesno_prompt` does), else a
plain screen of the player. The answer is the user's: nothing here accepts a save.
"""

import renpy

SCREEN = "_py2c_yesno"


def _screen(message=None, yes_action=None, no_action=None, **scope):
    ui = renpy.ui
    store = renpy.store

    ui.frame(xalign=0.5, yalign=0.5, background=store.Solid("#000d"), padding=(30, 24), xmaximum=int(renpy.config.screen_width * 0.8))
    ui.vbox(spacing=24)
    ui.text(message or "", xalign=0.5, color="#ffffff", text_align=0.5)
    ui.hbox(xalign=0.5, spacing=60)
    ui.textbutton("Yes", clicked=yes_action, xminimum=120, text_color="#ffffff", text_hover_color="#ffdd66", text_xalign=0.5)
    ui.textbutton("No", clicked=no_action, xminimum=120, text_color="#ffffff", text_hover_color="#ffdd66", text_xalign=0.5)
    ui.close()
    ui.close()


def yesno_prompt(screen, message):
    """Show the question and wait for Yes (True) or No (False). `screen` is unused, as in stock."""

    store = renpy.store

    if renpy.config.confirm_screen and renpy.exports.has_screen("confirm"):
        name = "confirm"
    elif renpy.exports.has_screen("yesno_prompt"):
        name = "yesno_prompt"
    else:
        name = SCREEN

    try:
        renpy.exports.show_screen(name, message=message, yes_action=store.Return(True), no_action=store.Return(False))
        return renpy.ui.interact()
    finally:
        renpy.exports.hide_screen(name)


def install():
    renpy.display.screen.define_screen(SCREEN, _screen, zorder="1900", modal="True")

    layout = getattr(renpy.store, "layout", None)

    if layout is not None and not hasattr(layout, "yesno_prompt"):
        layout.yesno_prompt = yesno_prompt
