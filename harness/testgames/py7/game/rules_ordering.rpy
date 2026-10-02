# Python 2 orders values of different types: None < numbers < other types. Python 3 raises TypeError. The compat module
# recompiles the failing code with Python 2 ordering and runs it again. Several failing blocks stand in this one file on
# purpose: every module-level code object has co_firstlineno 1, so each block must have its own fix key.

# Init code takes the retry path (no rollback in the init phase).
init python:
    init_sorted = sorted([3, "a", None, 2])

init python:
    init_lt = (1 < "a")

label case_ordering_init:
    python:
        init_text = ",".join([str(x) for x in init_sorted])
    e "Ordering at init: [init_text], [init_lt]."
    return

# The same rule in story code: the exception handler fixes the block and runs it again.
# block_rollback makes the handler run the node again in place. With rollback allowed it goes back to the last checkpoint
# instead and the line before the failing node shows twice, which the executed-dialogue comparison with stock would flag.
label case_ordering:
    $ renpy.block_rollback()
    python:
        story_sorted = sorted([2, "b", 1, "a", None])
    $ renpy.block_rollback()
    python:
        story_max = max(1, "a")
        story_lt = ("z" > 5)
        story_text = ",".join([str(x) for x in story_sorted])
    e "Ordering in the story: [story_text], max [story_max], [story_lt]."
    return
