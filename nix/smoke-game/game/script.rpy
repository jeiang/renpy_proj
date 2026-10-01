# Synthetic test game for checks.<system>.player-smoke. Original, no third-party material.
define e = Character("Tester")

label start:
    e "This is the first line."
    menu:
        "Take the left path.":
            e "Left."
        "Take the right path.":
            e "Right."
    e "The end."
    return
