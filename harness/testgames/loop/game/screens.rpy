# Minimal screens: a main menu, the say window and the choice menu (the harness answers screens named main_menu and choice).
screen main_menu():
    tag menu
    textbutton "Start" action Start() xalign 0.5 yalign 0.5

screen say(who, what):
    window id "window":
        text what id "what"

screen choice(items):
    vbox:
        xalign 0.5
        yalign 0.5
        for i in items:
            textbutton i.caption action i.action
