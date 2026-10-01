# The same rule in story code: the exception handler fixes the block and runs it again (one failing block per file).
# block_rollback makes the handler run the node again in place. With rollback allowed it goes back to the last checkpoint
# instead and the line before the failing node shows twice, which the executed-dialogue comparison with stock would flag.
label case_ordering:
    $ renpy.block_rollback()
    python:
        story_sorted = sorted([2, "b", 1, "a", None])
        story_max = max(1, "a")
        story_lt = ("z" > 5)
        story_text = ",".join([str(x) for x in story_sorted])
    e "Ordering in the story: [story_text], max [story_max], [story_lt]."
    return
