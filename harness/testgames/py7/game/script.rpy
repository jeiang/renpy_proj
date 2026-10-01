# Story spine. Every case label prints its results with say statements: the executed-dialogue hashes of the gate then
# compare the Python 2 results of the stock engine with the ones of the player.
image synth logo = "logo.png"
default mode = "all"
default player = ""

label start:
    scene black
    show synth logo at truecenter
    e "This is the Ren'Py 7 synthetic game."
    e "It was compiled by the 7.4.11 SDK, so its scripts hold Python 2 pickles."
    $ player = renpy.input("What is your name?", length=12)
    $ player = player.strip() or "Nobody"
    e "Hello, [player]."

    menu:
        e "Which cases should I run?"

        "All cases":
            $ mode = "all"
        "Only the arithmetic":
            $ mode = "math"
        "None":
            $ mode = "none"

    e "Mode: [mode]."

    if mode != "none":
        call case_math
    if mode == "all":
        call case_containers
        call case_classes
        call case_exec
        call case_syntax_values
        call case_ordering
        call case_legacy
        call case_state
        call case_syntax_loose

    e "All cases ran."
    nar "The end."
    return
