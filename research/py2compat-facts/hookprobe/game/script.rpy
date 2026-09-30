init python:
    HP_INIT = 7 / 2          # init python: exec mode
    def compute(a):
        return a / 2
define hp_def = 9 / 2        # define: eval mode
default hp_default = 11 / 2  # default: eval mode
default hp_result = None

screen hp_screen():
    if 7 / 2 > 3:            # screen `if` expression
        add Solid("#fff", xsize=7 / 2, ysize=10)
    python:
        hp_scr = 5 / 2       # screen python block
transform hp_atl:
    xpos 7 / 2               # ATL expression
    linear 1.0 ypos 9 / 2

label main_menu:
    return

label start:
    $ hp_line = 13 / 2                     # `$` line
    "one"
    "two"
    $ hp_result = compute(9)               # ok in py2, 4 in py2 int div; 4.5 in py3 (rewritten to 4)
    python:
        print 'py2print'                    # py2 print statement: SyntaxError -> fix_tokens hook
    "three"
    $ hp_first = {1: "a"}.keys()[0]        # py2 list, py3 dict_keys: TypeError at runtime -> exception handler
    "three-b"
    "four [hp_result]"
    return
