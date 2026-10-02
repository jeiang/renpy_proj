# Port patch test. The python block below works in Python 2 only: a str has no decode method in Python 3, and no rewrite
# rule of the compat module can know that. The patch in patches/ replaces the block.
# The script has 20 lines on purpose: the gate saves a few lines in (auto-click keeps running while the save settles)
# and loads that save, so more than the resume lines must remain after the save point.
define config.name = "Synth7Patch"
define config.save_directory = "synth7-patch-test"
define config.screen_width = 800
define config.screen_height = 600
define config.has_autosave = False
define config.has_quicksave = False
define config.developer = False
define e = Character("Eve", color="#a0e0a0")
default shown = ""

label start:
    scene black
    e "This game has one line that only Python 2 runs."
    e "The next node decodes a str."
    python:
        shown = "cafe".decode("ascii")
    e "Decoded: [shown]."
    e "The patch replaced the failing node."
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
