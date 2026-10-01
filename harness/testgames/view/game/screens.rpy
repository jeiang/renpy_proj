init python:
    # Without gui.rpy no theme asks for a screen based main menu: ask for it.
    layout.screen_main_menu()

screen main_menu():
    tag menu
    add "pattern"
    vbox:
        xalign 0.5
        yalign 0.5
        spacing 20
        textbutton "Start" action Start() xalign 0.5 text_size 48 background "#000a" text_color "#fff"
        textbutton "Quit" action Quit(confirm=False) xalign 0.5 text_size 48 background "#000a" text_color "#fff"

# One screen per case. Each holds still. Sizes and positions are odd or fractional on purpose.

screen view_pattern():
    add "pattern"

screen view_bars():
    # Seven bars. The widths are fractions of 1277, so the edges fall between pixels.
    add Solid("#000")
    hbox:
        xysize (1.0, 1.0)
        for c in ("#fff", "#ff0", "#0ff", "#0f0", "#f0f", "#f00", "#00f"):
            add Solid(c) xsize (1.0 / 7.0) ysize 1.0
    add Solid("#f00") xpos 0 ypos 0 xysize (1, 719)
    add Solid("#0f0") xpos 1276 ypos 0 xysize (1, 719)
    add Solid("#00f") xpos 0 ypos 0 xysize (1277, 1)
    add Solid("#ff0") xpos 0 ypos 718 xysize (1277, 1)

screen view_frames():
    add Solid("#303040")
    add Frame("frame", 7, 7) xpos 11 ypos 13 xysize (301, 203)
    add Frame("frame", 7, 7, tile=True) xpos 333 ypos 13 xysize (297, 201)
    add Frame("frame", 7, 7) xalign 0.37 yalign 0.91 xysize (451, 333)
    add Tile("tile") xpos 700 ypos 40 xysize (211, 157)
    add Frame("frame", 7, 7) xpos 1277 - 133 ypos 719 - 61 xysize (131, 59)

screen view_circle():
    add Solid("#101820")
    add "circle" xalign 0.5 yalign 0.5
    add "circle" xpos 100 ypos 100 zoom 0.5
    add "circle" xanchor 1.0 yanchor 1.0 xpos 1277 ypos 719 zoom 0.33

screen view_sub():
    add Solid("#203020")
    # Fractional positions and zoom, drawn with subpixel placement.
    add Transform("circle", subpixel=True, xpos=401.5, ypos=100.5, zoom=0.7)
    add Transform("circle", subpixel=True, xpos=700.25, ypos=300.75, zoom=0.3)
    add Transform("pattern", crop=(11, 7, 513, 301), subpixel=True, xpos=33.5, ypos=401.5)
    add Transform("pattern", crop_relative=True, crop=(0.25, 0.25, 0.3333, 0.3333), xpos=810, ypos=401)
