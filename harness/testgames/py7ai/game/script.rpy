# Model upgrade test. The python block below works in Python 2 only: the string module lost join in Python 3, and no
# rewrite rule of the compat module covers it (the error handler only knows ordering). There is no patch in this
# folder: `player upgrade` has to write it. The script has 20 lines on purpose (see py7patch).
define config.name = "Synth7AI"
define config.save_directory = "synth7-ai-test"
define config.screen_width = 800
define config.screen_height = 600
define config.has_autosave = False
define config.has_quicksave = False
define config.developer = False
define e = Character("Eve", color="#a0e0a0")
default shown = ""
init python:
    import string
label start:
    scene black
    e "This game has one line that only Python 2 runs."
    e "The next node joins three words."
    python:
        words = ["alpha", "beta", "gamma"]
        shown = string.join(words, "-")
    e "Joined: [shown]."
    e "The model patch replaced the failing node."
    e "Line six."
    e "Line seven."
    e "Line eight."
    e "Line nine."
    e "Line ten."
    e "Line eleven."
    e "Line twelve."
    e "Line thirteen."
    e "Line fourteen."
    e "Line fifteen."
    e "Line sixteen."
    e "Line seventeen."
    e "Line eighteen."
    e "Line nineteen."
    e "Line twenty."
    return
