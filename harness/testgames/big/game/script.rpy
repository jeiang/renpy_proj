# Big test game: the hand-written part. gen/ holds the generated labels (about 54 000 lines in 120 files, built by
# ../build.py). `label start` enters the chain at the first label; the chain runs through every file in order.

define config.name = "Synth big"
define config.save_directory = "synth-big"
define config.screen_width = 1280
define config.screen_height = 720
define config.has_autosave = False
define config.has_quicksave = False
define config.rollback_enabled = False
define config.window_title = "Synth big"
define config.developer = False

define n = Character(None, what_color="#ffffff")

# One entry per label reached: the save grows with the route (the table in gen/store.rpy is the fixed part).
default big_state = {}

label start:
    jump f000_l00
