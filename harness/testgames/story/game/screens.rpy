# A minimal main menu. The harness needs a screen named main_menu with a Start button.
init python:
    # Without gui.rpy no theme asks for a screen based main menu: ask for it.
    layout.screen_main_menu()

screen main_menu():
    tag menu
    add Solid("#18243c")
    add "bg harbor" alpha 0.35
    vbox:
        xalign 0.5
        yalign 0.5
        spacing 24
        text "SYNTH STORY" size 72 xalign 0.5 color "#f2e6c9"
        textbutton "Start" action Start() xalign 0.5 text_size 40
        textbutton "Quit" action Quit(confirm=False) xalign 0.5 text_size 40
