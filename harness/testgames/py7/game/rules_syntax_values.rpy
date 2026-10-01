# Python 2 only syntax in init blocks. The compat module fixes it with the token-level fixer. One block per rule, so a
# failing rule is easy to find. Results go into one dictionary that the case label reads.
init python:
    synth_syntax = {}

init python:
    synth_syntax["backtick"] = `42` + `3.5`

init python:
    synth_syntax["ne"] = (1 <> 2, 2 <> 2)

init python:
    synth_syntax["octal"] = 0755

init python:
    synth_syntax["long"] = 10L * 3

init python:
    synth_syntax["ur"] = len(ur"a\nb")

init python:
    try:
        raise ValueError, "boom"
    except ValueError, err:
        synth_syntax["raise"] = str(err)

init python:
    def sum_pair((a, b)):
        return a + b

    synth_syntax["tuple_param"] = sum_pair((3, 4))

label case_syntax_values:
    python:
        sv = synth_syntax
        v_backtick = sv["backtick"]
        v_ne1 = sv["ne"][0]
        v_ne2 = sv["ne"][1]
        v_octal = sv["octal"]
        v_long = sv["long"]
        v_ur = sv["ur"]
        v_raise = sv["raise"]
        v_tuple = sv["tuple_param"]
    e "Backticks [v_backtick]; not-equal [v_ne1] and [v_ne2]."
    e "Octal [v_octal]; long [v_long]; raw string length [v_ur]."
    e "Raise with a value: [v_raise]; tuple parameter: [v_tuple]."
    return
