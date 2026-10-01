# Arithmetic rules: `/` is integer division on integers, round() rounds half away from zero, cmp().
label case_math:
    python:
        q1 = 7 / 2
        q2 = -7 / 2
        q3 = 7 / 2.0
        q4 = 9
        q4 /= 2
        r1 = round(2.5)
        r2 = round(-0.5)
        r3 = round(1.5)
        c1 = cmp(1, 2)
        c2 = cmp("b", "a")
        big = long(5) ** 20
        ch = unichr(233)
    e "Division gives [q1], [q2], [q3] and [q4]."
    e "Round gives [r1], [r2] and [r3]."
    e "Compare gives [c1] and [c2]; long gives [big]; unichr gives [ch]."
    return
