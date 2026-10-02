# Save-trust test (Ren'Py 7). No gui, no theme and no confirm screen: the game never calls `layout.defaults()`, so
# `layout.yesno_prompt` exists only if the player provides it. The gate check `trust` writes a save, loads it under a
# new signing key and answers the trust question on the screen `_py2c_yesno`.
define config.name = "Synth7Trust"
define config.save_directory = "synth7-trust-test"
define config.screen_width = 800
define config.screen_height = 600
define config.has_autosave = False
define config.has_quicksave = False
define config.developer = False
define e = Character("Eve", color="#a0e0a0")
label start:
    scene black
    e "Line one."
    e "Line two."
    e "Line three."
    e "Line four."
    e "Line five."
    e "Line six."
    e "Line seven."
    e "Line eight."
    return
