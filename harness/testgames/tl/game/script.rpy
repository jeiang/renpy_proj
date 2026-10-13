# Translation test game. The plan (tl.plan) runs start, then jumps to each to_* label, which switches the language
# with renpy.change_language and continues in that language's own label. Each label has its own lines, so every
# translated line has a unique identifier in tl/<lang>/script.rpy. _() runs again in each part, so the translated
# string is the one in the language that is active there.
define e = Character("Eve")

label start:
    e "Line one is in English."
    $ title = _("Welcome to the test")
    e "[title]"
    $ langs = ", ".join(sorted(renpy.known_languages()))
    e "Known languages: [langs]"
    e "Next the script switches to French."
    jump to_fr

label to_fr:
    $ renpy.change_language("fr")
    jump french_part

label french_part:
    e "Line two is shown after the switch to French."
    $ title = _("Welcome to the test")
    e "[title]"
    jump to_en

label to_en:
    $ renpy.change_language(None)
    jump english_part

label english_part:
    e "Line three is back in English."
    $ title = _("Welcome to the test")
    e "[title]"
    jump to_de

label to_de:
    $ renpy.change_language("de")
    jump german_part

label german_part:
    e "Line four is shown in German."
    $ title = _("Welcome to the test")
    e "[title]"
    jump to_en
