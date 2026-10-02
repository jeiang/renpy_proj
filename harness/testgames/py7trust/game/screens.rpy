# Minimal screens. Without a gui, Ren'Py 7 jumps straight to `start` (no main menu) and shows menus and input with its old
# layout code. The gate driver needs the screens main_menu, choice and input.
init python:
    layout.screen_main_menu()

screen main_menu():
    tag menu
    add Solid("#203040")
    vbox:
        xalign 0.5
        yalign 0.5
        spacing 20
        text "SYNTH 7 TRUST" size 48 xalign 0.5
        textbutton "Start" action Start() xalign 0.5
        textbutton "Quit" action Quit(confirm=False) xalign 0.5

screen choice(items):
    window:
        style "menu_window"
        xalign 0.5
        yalign 0.5
        vbox:
            style "menu"
            spacing 8
            for i in items:
                textbutton i.caption action i.action

screen input(prompt):
    window:
        style "input_window"
        vbox:
            xalign 0.5
            yalign 0.5
            text prompt
            input id "input"
