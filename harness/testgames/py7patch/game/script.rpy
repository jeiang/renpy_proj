# Port patch test. The python block below works in Python 2 only: a str has no decode method in Python 3, and no rewrite
# rule of the compat module can know that. The patch in patches/ replaces the block.
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
    return
