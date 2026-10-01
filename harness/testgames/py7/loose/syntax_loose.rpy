# Loose script (not compiled at build time): the player parses it with its Ren'Py 8 parser, which must take the Ren'Py 7
# leniencies: a colon with no block after `scene ... with t:` and `show x:`, and a screen property with no value.
image synth_back = Solid("#243a5e")
image synth_card = Solid("#c8a032")

screen synth_button():
    imagebutton:
        idle "synth_card"
        focus_mask
        action NullAction()
        xpos 300
        ypos 200
        xysize (200, 100)

screen synth_auto_unused():
    imagebutton auto:
        action NullAction()

label case_syntax_loose:
    scene synth_back with fade:
    show synth_card:
    e "The loose script parsed: a colon with no block was accepted."
    show screen synth_button
    e "A screen property with no value was accepted."
    hide screen synth_button
    scene black
    return
