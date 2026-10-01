init python:
    import legacy_mod

label case_legacy:
    python:
        lm_text = legacy_mod.describe({"b": 1, "a": 2})
        lm_ratio = legacy_mod.ratio(9, 2)
        lm_print = legacy_mod.printed()
    e "The loose module says [lm_text] and [lm_ratio]."
    e "The print statements wrote [lm_print]."
    return
