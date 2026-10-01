# Viewport test game. The story only has to run; the cases are the labels view_*, reached by the gate plan (view.plan).
define n = Character(None, window_background="#000b", what_color="#fff")

image pattern = "images/pattern.png"
image circle = "images/circle.png"
image frame = "images/frame.png"
image tile = "images/tile.png"

label start:
    scene pattern with None
    n "This game draws a flat pattern on an odd-sized screen."
    n "A one pixel frame runs around the whole view."
    n "Red is the top edge, green the right, blue the bottom, yellow the left."
    n "Colour bars, a circle and four corner markers fill the middle."
    n "The view is a few pixels smaller than the window."
    n "The bars around it are the padding."
    n "Every case below shows the same kind of pattern in a new layout."
    n "This is the eighth line."
    n "And a ninth one, to keep a margin."
    n "The tenth line closes the story."
    jump view_pattern_case

label view_pattern_case:
    call screen view_pattern

label view_bars_case:
    call screen view_bars

label view_frames_case:
    call screen view_frames

label view_circle_case:
    call screen view_circle

label view_sub_case:
    call screen view_sub
