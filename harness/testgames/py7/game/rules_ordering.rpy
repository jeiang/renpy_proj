# The same rule in story code: the exception handler fixes the block and runs it again (one failing block per file).
label case_ordering:
    python:
        story_sorted = sorted([2, "b", 1, "a", None])
        story_max = max(1, "a")
        story_lt = ("z" > 5)
        story_text = ",".join([str(x) for x in story_sorted])
    e "Ordering in the story: [story_text], max [story_max], [story_lt]."
    return
