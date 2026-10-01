# Python 2 orders values of different types: None < numbers < other types. Python 3 raises TypeError. The compat module
# recompiles the failing code with Python 2 ordering and runs it again. Init code takes the retry path.
# One failing block per file: the story case is in rules_ordering.rpy.
init python:
    init_sorted = sorted([3, "a", None, 2])
    init_lt = (1 < "a")

label case_ordering_init:
    python:
        init_text = ",".join([str(x) for x in init_sorted])
    e "Ordering at init: [init_text], [init_lt]."
    return
