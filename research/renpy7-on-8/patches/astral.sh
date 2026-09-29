# py2 `__metaclass__ = IterRegistry` is a silent no-op on py3 (`for x in Battle` -> TypeError: 'type' object is not iterable).
# Move it to the class header: class X(object, metaclass=IterRegistry). 4 classes, 4 files.
perl -0pi -e 's/(class (?:Battle|Decks|NPC|Trader))\(object\):((?:[^\n]*\n){0,4}?[ \t]+)__metaclass__ = IterRegistry/$1(object, metaclass=IterRegistry):$2pass/' combat/battle/Battle.rpy combat/decks/Decks.rpy variables/classes/NPC.rpy variables/classes/Trader.rpy
# 8.5.3 (not 8.3.2) raises on `background "black gradient bg"` (style cardLevelUp_frame): no such image file exists, "black" is the
# built-in solid, and newer engines no longer accept the leftover attributes. 7.8.2/8.3.2 tolerated it. Use plain black.
perl -pi -e 's/^(\s*background )"black gradient bg"/$1"#000"/' combat/cards/CardManager.rpy
