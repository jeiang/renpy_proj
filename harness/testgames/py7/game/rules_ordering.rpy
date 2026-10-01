# Python 2 orders values of different types: None < numbers < other types. Python 3 raises TypeError. The compat module
# recompiles the failing code with Python 2 ordering and runs it again (init code and story code take different paths).
init python:
    init_sorted = sorted([3, "a", None, 2])
    init_lt = (1 < "a")

label case_ordering:
    python:
        story_sorted = sorted([2, "b", 1, "a", None])
        story_max = max(1, "a")
        story_lt = ("z" > 5)
        init_text = ",".join([str(x) for x in init_sorted])
        story_text = ",".join([str(x) for x in story_sorted])
    e "Ordering at init: [init_text], [init_lt]."
    e "Ordering in the story: [story_text], max [story_max], [story_lt]."
    return
