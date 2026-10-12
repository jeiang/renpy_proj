init python:
    # Without gui.rpy no theme asks for a screen based main menu: ask for it.
    layout.screen_main_menu()

screen main_menu():
    tag menu
    add Solid("#203040")
    vbox:
        xalign 0.5
        yalign 0.5
        spacing 20
        textbutton "Start" action Start() xalign 0.5 text_size 48 background "#000a" text_color "#fff"
        textbutton "Quit" action Quit(confirm=False) xalign 0.5 text_size 48 background "#000a" text_color "#fff"
