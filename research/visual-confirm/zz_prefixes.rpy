# Extra probe for Lucky_Paradox on 8.1+: restore the pre-8.1 default search prefixes (see research/renpy7-on-8) so Movie(play="Personajes/...") resolves inside images.rpa.
init 1000 python:
    config.search_prefixes = ["", "images/"]
