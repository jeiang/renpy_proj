# RTL check for the `text` crate. Put this file and DejaVuSans.ttf (from renpy/common) in a scratch game's
# game/ folder, then run `player <scratch game>`. The window shows Hebrew, Arabic and mixed-direction lines.
# Expected: Hebrew and Arabic read from right to left, right-aligned, Arabic letters joined, the
# numbers and the Latin word inside them in left-to-right order, brackets mirrored.
define config.rtl = True
define gui.text_font = "DejaVuSans.ttf"
define config.window_title = "rtl check"

screen rtl_demo():
    frame:
        xfill True
        yfill True
        background "#203040"
        vbox:
            xfill True
            spacing 18
            text "Hebrew: שלום עולם (123) abc" size 40 xalign 1.0 color "#ffffff"
            text "Arabic: مرحبا بالعالم 2024 ok" size 40 xalign 1.0 color "#ffe080"
            text "Mixed: abc אבג (def) 456" size 40 xalign 1.0 color "#a0ffa0" outlines [(2, "#000000", 0, 0)]
            text "Left to right line stays put." size 40 xalign 0.0 color "#ffffff"

label start:
    show screen rtl_demo
    $ renpy.pause(600)
    return
