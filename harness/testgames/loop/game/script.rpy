# Loop test game: loops on purpose, so the harness loop guard can be shown to detect a loop. Our own content.
#   say_loop:    12 distinct lines, then back to the first line, forever (no menu, no screen).
#   screen_loop: a screen with a button that returns to the same screen, and a button that leaves.
define config.name = "Loop test"
define config.save_directory = "loop-test"
define config.screen_width = 1280
define config.screen_height = 720
define config.has_autosave = False
define config.has_quicksave = False
define config.window_title = "Loop test"
define config.developer = False

define n = Character(None)

screen loop_hub():
    vbox:
        xalign 0.5
        yalign 0.5
        spacing 20
        textbutton "Again" action Jump("screen_loop")
        textbutton "Leave" action Jump("leave")

label start:
    n "Loop test game. The first menu item loops on purpose."
    menu:
        "Say loop":
            jump say_loop
        "Screen loop":
            jump screen_loop

label say_loop:
    n "Round line 1."
    n "Round line 2."
    n "Round line 3."
    n "Round line 4."
    n "Round line 5."
    n "Round line 6."
    n "Round line 7."
    n "Round line 8."
    n "Round line 9."
    n "Round line 10."
    n "Round line 11."
    n "Round line 12."
    jump say_loop

label screen_loop:
    call screen loop_hub
    jump screen_loop

label leave:
    n "You left the loop."
    return
