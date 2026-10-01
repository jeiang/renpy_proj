# Python 2 only syntax in init blocks. The compat module fixes it with the token-level fixer. One block per rule, so a
# failing rule is easy to find. Results go into one dictionary that the case label reads.
init python:
    synth_syntax = {}

init python:
    synth_syntax["backtick"] = `42` + `"s"`

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
    e "Backticks [sv['backtick']]; not-equal [sv['ne'][0]] and [sv['ne'][1]]."
    e "Octal [sv['octal']]; long [sv['long']]; raw string length [sv['ur']]."
    e "Raise with a value: [sv['raise']]; tuple parameter: [sv['tuple_param']]."
    return
